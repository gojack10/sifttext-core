//! Load sorted records into empty SQLite b-trees in a private, closed database.
//! SQLite creates the catalog and small tables. The large, known-order tables
//! need neither page searches nor online balancing. Pages are standard format 3;
//! the normal SQLite pager owns all reads, WAL writes and recovery after the load.
use crate::{Result, require};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    os::unix::fs::FileExt,
    path::Path,
};

pub(super) const PAGE: usize = 16384;

pub(super) enum Field<'a> {
    Null,
    Int(u64),
    Text(&'a str),
    Blob(&'a [u8]),
}
use Field::*;
fn vint_len(x: u64) -> usize {
    ((64 - x.leading_zeros()) as usize)
        .max(1)
        .div_ceil(7)
        .min(9)
}
fn vint(dst: &mut Vec<u8>, mut x: u64) {
    if x < 128 {
        dst.push(x as u8);
        return;
    }
    let mut b = [0u8; 9];
    let mut j = 8;
    if x > 0x00ff_ffff_ffff_ffff {
        b[j] = x as u8;
        x >>= 8;
    } else {
        b[j] = (x & 127) as u8;
        x >>= 7;
    }
    while x != 0 {
        j -= 1;
        b[j] = 128 | (x & 127) as u8;
        x >>= 7;
    }
    dst.extend_from_slice(&b[j..]);
}
fn integer_width(v: u64) -> usize {
    match v {
        0 | 1 => 0,
        2..=127 => 1,
        128..=32767 => 2,
        32768..=8388607 => 3,
        8388608..=2147483647 => 4,
        2147483648..=140737488355327 => 6,
        _ => 8,
    }
}
fn serial(f: &Field<'_>) -> (u64, usize) {
    match f {
        Null => (0, 0),
        Int(0) => (8, 0),
        Int(1) => (9, 0),
        Int(v) => {
            let n = integer_width(*v);
            (
                match n {
                    6 => 5,
                    8 => 6,
                    _ => n as u64,
                },
                n,
            )
        }
        Text(s) => (13 + 2 * s.len() as u64, s.len()),
        Blob(s) => (12 + 2 * s.len() as u64, s.len()),
    }
}
pub(super) fn record(fields: &[Field<'_>]) -> Vec<u8> {
    let mut n = 1;
    let mut data = 0;
    for f in fields {
        let (ty, w) = serial(f);
        n += vint_len(ty);
        data += w;
    }
    // Our widest table has nine columns and each source field is bounded.
    assert!(n < 128);
    let mut v = Vec::with_capacity(n + data);
    v.push(n as u8);
    for f in fields {
        vint(&mut v, serial(f).0);
    }
    for f in fields {
        match f {
            Text(s) => v.extend_from_slice(s.as_bytes()),
            Blob(b) => v.extend_from_slice(b),
            Int(x) => v.extend_from_slice(&x.to_be_bytes()[8 - integer_width(*x)..]),
            Null => (),
        }
    }
    v
}
fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes(b.try_into().unwrap())
}
struct Page {
    bytes: Box<[u8; PAGE]>,
    slots: usize,
    end: usize,
    head: usize,
}
impl Page {
    fn new(tag: u8) -> Self {
        let mut p = Self {
            bytes: Box::new([0; PAGE]),
            slots: 0,
            end: PAGE,
            head: if tag == 5 || tag == 2 { 12 } else { 8 },
        };
        p.bytes[0] = tag;
        p
    }
    fn fits(&self, len: usize) -> bool {
        self.end >= self.head + (self.slots + 1) * 2 + len.max(4)
    }
    fn insert(&mut self, cell: &[u8]) {
        assert!(self.fits(cell.len()));
        self.end -= cell.len().max(4);
        self.bytes[self.end..self.end + cell.len()].copy_from_slice(cell);
        let i = self.head + 2 * self.slots;
        self.bytes[i..i + 2].copy_from_slice(&(self.end as u16).to_be_bytes());
        self.slots += 1;
    }
    fn finish(mut self, right: Option<u32>) -> Box<[u8; PAGE]> {
        self.bytes[3..5].copy_from_slice(&(self.slots as u16).to_be_bytes());
        self.bytes[5..7].copy_from_slice(&(self.end as u16).to_be_bytes());
        if let Some(r) = right {
            self.bytes[8..12].copy_from_slice(&r.to_be_bytes());
        }
        self.bytes
    }
}
pub(super) struct Image {
    file: File,
    pages: u32,
    roots: BTreeMap<String, u32>,
    catalog_pages: u32,
    first_pending: u32,
    pending: Vec<u8>,
}
impl Image {
    pub(super) fn open(path: &Path, roots: BTreeMap<String, u32>) -> Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let mut h = [0u8; 100];
        file.read_exact_at(&mut h, 0)?;
        let pages = be32(&h[28..32]);
        require(
            &h[..16] == b"SQLite format 3\0"
                && u16::from_be_bytes([h[16], h[17]]) as usize == PAGE
                && h[18..21] == [1, 1, 0]
                && be32(&h[52..56]) == 0
                && u64::from(pages) * PAGE as u64 == file.metadata()?.len(),
            "unsupported private page layout",
        )?;
        Ok(Self { file, pages, roots, catalog_pages: pages, first_pending: 0,
            pending: Vec::with_capacity(1 << 20) })
    }
    fn reserve(&mut self) -> Result<u32> {
        self.pages = self.pages.checked_add(1).ok_or("database page overflow")?;
        Ok(self.pages)
    }
    fn drain(&mut self) -> Result<()> {
        if self.pending.is_empty() { return Ok(()); }
        let offset = u64::from(self.first_pending - 1) * PAGE as u64;
        self.file.write_all_at(&self.pending, offset)?;
        // Schedule complete runs, including overflow pages. The durable barrier
        // is the publisher's sync before activation, not this writeback hint.
        #[cfg(target_os = "linux")] {
            use std::os::fd::AsRawFd;
            unsafe { libc::sync_file_range(self.file.as_raw_fd(), offset as i64,
                self.pending.len() as i64, libc::SYNC_FILE_RANGE_WRITE); }
        }
        self.pending.clear();
        Ok(())
    }
    fn write(&mut self, n: u32, b: &[u8]) -> Result<()> {
        if n <= self.catalog_pages {
            self.file.write_all_at(b, u64::from(n - 1) * PAGE as u64)?;
        } else {
            if self.pending.is_empty() { self.first_pending = n; }
            require(n == self.first_pending + (self.pending.len() / PAGE) as u32 && b.len() == PAGE,
                "noncontiguous bulk pages")?;
            self.pending.extend_from_slice(b);
            if self.pending.len() >= 1 << 20 { self.drain()?; }
        }
        Ok(())
    }
    fn append(&mut self, b: &[u8]) -> Result<u32> {
        let n = self.reserve()?;
        self.write(n, b)?;
        Ok(n)
    }
    fn root(&mut self, name: &str, tag: u8) -> Result<u32> {
        let n = self.roots.remove(name).ok_or("missing bulk table")?;
        let mut hdr = [0; 8];
        self.file
            .read_exact_at(&mut hdr, u64::from(n - 1) * PAGE as u64)?;
        require(
            n > 1 && hdr[0] == tag && hdr[3..5] == [0, 0],
            "nonempty bulk root",
        )?;
        Ok(n)
    }
    fn cell(&mut self, data: &[u8], row: Option<u64>) -> Result<Vec<u8>> {
        let n = data.len();
        let min = (PAGE - 12) * 32 / 255 - 23;
        let max = if row.is_some() {
            PAGE - 35
        } else {
            (PAGE - 12) * 64 / 255 - 23
        };
        let local = if n <= max {
            n
        } else {
            let k = min + (n - min) % (PAGE - 4);
            if k <= max { k } else { min }
        };
        let mut v = Vec::with_capacity(24 + local);
        vint(&mut v, n as u64);
        if let Some(r) = row {
            vint(&mut v, r);
        }
        v.extend_from_slice(&data[..local]);
        if local < n {
            let first = self.pages + 1;
            v.extend_from_slice(&first.to_be_bytes());
            let mut pieces = data[local..].chunks(PAGE - 4).peekable();
            while let Some(chunk) = pieces.next() {
                let pg = self.reserve()?;
                let mut buf = [0; PAGE];
                let next = if pieces.peek().is_some() { pg + 1 } else { 0 };
                buf[..4].copy_from_slice(&next.to_be_bytes());
                buf[4..4 + chunk.len()].copy_from_slice(chunk);
                self.write(pg, &buf)?;
            }
        }
        Ok(v)
    }
    // Ordinary table: separators repeat the maximum integer key of their left child.
    pub(super) fn table(
        &mut self,
        name: &str,
        rows: impl IntoIterator<Item = (u64, Vec<u8>)>,
    ) -> Result<()> {
        let root = self.root(name, 13)?;
        let mut level = Vec::new();
        let mut page = Page::new(13);
        let mut last = 0;
        for (id, row) in rows {
            require(id > last, "unordered bulk rowid")?;
            let c = self.cell(&row, Some(id))?;
            if !page.fits(c.len()) {
                level.push((self.append(&*page.finish(None))?, last));
                page = Page::new(13);
            }
            page.insert(&c);
            last = id;
        }
        if level.is_empty() {
            return self.write(root, &*page.finish(None));
        }
        level.push((self.append(&*page.finish(None))?, last));
        loop {
            let groups = level.len().div_ceil(512);
            let mut next = Vec::with_capacity(groups);
            for g in 0..groups {
                let kids = &level[g * level.len() / groups..(g + 1) * level.len() / groups];
                let mut p = Page::new(5);
                for &(child, max) in &kids[..kids.len() - 1] {
                    let mut c = Vec::with_capacity(13);
                    c.extend_from_slice(&child.to_be_bytes());
                    vint(&mut c, max);
                    p.insert(&c);
                }
                let &(right, max) = kids.last().unwrap();
                let b = p.finish(Some(right));
                if groups == 1 {
                    return self.write(root, &*b);
                }
                next.push((self.append(&*b)?, max));
            }
            level = next;
        }
    }
    // Index (also WITHOUT ROWID): a boundary record belongs to exactly one
    // page. Promote it rather than duplicating its payload in the leaf.
    pub(super) fn index(
        &mut self,
        name: &str,
        sorted: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<()> {
        let root = self.root(name, 10)?;
        let mut children = Vec::new();
        let mut keys = Vec::new();
        let mut group: Vec<Vec<u8>> = Vec::new();
        let mut used = 8;
        let leaf = |cells: &[Vec<u8>]| {
            let mut p = Page::new(10);
            for c in cells {
                p.insert(c);
            }
            p.finish(None)
        };
        for row in sorted {
            let cell = self.cell(&row, None)?;
            let cost = 2 + cell.len().max(4);
            if used + cost > PAGE {
                keys.push(group.pop().ok_or("oversized index cell")?);
                children.push(self.append(&*leaf(&group))?);
                group.clear();
                used = 8;
            }
            group.push(cell);
            used += cost;
        }
        if children.is_empty() {
            return self.write(root, &*leaf(&group));
        }
        children.push(self.append(&*leaf(&group))?);
        loop {
            let mut next = Vec::new();
            let mut promoted = Vec::new();
            let mut start = 0;
            let mut used = 12;
            let branch = |i: usize, j: usize| {
                let mut p = Page::new(2);
                for k in i..j {
                    let mut c = Vec::with_capacity(4 + keys[k].len());
                    c.extend_from_slice(&children[k].to_be_bytes());
                    c.extend_from_slice(&keys[k]);
                    p.insert(&c);
                }
                p.finish(Some(children[j]))
            };
            for (i, sep) in keys.iter().enumerate() {
                let cost = 6 + sep.len();
                if used + cost > PAGE {
                    let pivot = i - 1;
                    next.push(self.append(&*branch(start, pivot))?);
                    promoted.push(keys[pivot].clone());
                    start = i;
                    used = 12;
                }
                used += cost;
            }
            let b = branch(start, keys.len());
            if next.is_empty() {
                return self.write(root, &*b);
            }
            next.push(self.append(&*b)?);
            keys = promoted;
            children = next;
        }
    }
    pub(super) fn finish(mut self) -> Result<()> {
        self.drain()?;
        let mut h = [0u8; 100];
        self.file.read_exact_at(&mut h, 0)?;
        let change = be32(&h[24..28]).wrapping_add(1).to_be_bytes();
        h[24..28].copy_from_slice(&change);
        h[92..96].copy_from_slice(&change);
        h[28..32].copy_from_slice(&self.pages.to_be_bytes());
        self.file.write_all_at(&h, 0)?;
        // This inode still lives under a durable rollback intent. Its publisher
        // must flush the entire finished generation before activating it.
        Ok(())
    }
}
