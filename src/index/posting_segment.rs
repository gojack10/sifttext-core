//! Initial bulk load of positional FTS5 segments. There is no prior row to
//! delete, merge or read. Use SQLite's real porter/unicode61 tokenizer, keep
//! bounded runs of row-ordered occurrences, then write the native leaves in
//! lexical order. Incremental edits still use unmodified FTS5. Format:
//! SQLite fts5_index.c (structure, segment leaf, doclist and page index).
use crate::{Result, require};
use rusqlite::{Connection, Statement, ffi, params};
use rustc_hash::FxHashMap;
use std::{
    ffi::{c_char, c_int, c_void},
    ptr,
};

struct Tokenizer {
    methods: ffi::fts5_tokenizer,
    raw: *mut ffi::Fts5Tokenizer,
}
impl Tokenizer {
    fn open(db: &Connection) -> Result<Self> {
        // The statement borrows the API pointer slot only through sqlite3_step.
        // SQLite owns the API; the tokenizer instance is ours until Drop.
        unsafe {
            let mut stmt = ptr::null_mut();
            let rc = ffi::sqlite3_prepare_v2(
                db.handle(),
                c"SELECT fts5(?1)".as_ptr(),
                -1,
                &mut stmt,
                ptr::null_mut(),
            );
            require(rc == ffi::SQLITE_OK, "FTS5 API unavailable")?;
            let mut api: *mut ffi::fts5_api = ptr::null_mut();
            let bound = ffi::sqlite3_bind_pointer(
                stmt,
                1,
                (&mut api as *mut *mut ffi::fts5_api).cast(),
                c"fts5_api_ptr".as_ptr(),
                None,
            );
            let ok = bound == ffi::SQLITE_OK && ffi::sqlite3_step(stmt) == ffi::SQLITE_ROW;
            let rc = ffi::sqlite3_finalize(stmt);
            require(
                ok && rc == ffi::SQLITE_OK && !api.is_null(),
                "missing FTS5 API",
            )?;
            let mut context = ptr::null_mut();
            let mut methods: ffi::fts5_tokenizer = std::mem::zeroed();
            let rc = (*api).xFindTokenizer.ok_or("no FTS5 tokenizer lookup")?(
                api,
                c"porter".as_ptr(),
                &mut context,
                &mut methods,
            );
            require(
                rc == ffi::SQLITE_OK && methods.xTokenize.is_some() && methods.xDelete.is_some(),
                "missing porter tokenizer",
            )?;
            let mut arg = c"unicode61".as_ptr();
            let mut raw = ptr::null_mut();
            let rc = methods.xCreate.ok_or("missing tokenizer constructor")?(
                context, &mut arg, 1, &mut raw,
            );
            require(
                rc == ffi::SQLITE_OK && !raw.is_null(),
                "cannot create porter unicode61 tokenizer",
            )?;
            Ok(Self { methods, raw })
        }
    }
}
impl Drop for Tokenizer {
    fn drop(&mut self) {
        unsafe { (self.methods.xDelete.unwrap())(self.raw) }
    }
}

// The input packet, number of rows, text length and page number are bounded
// below 2^32. In that range FTS5 uses ordinary big-endian base-128 varints.
#[inline]
fn var(buf: &mut Vec<u8>, value: u32) {
    if value < 128 {
        buf.push(value as u8);
        return;
    }
    let mut tmp = [0u8; 5];
    let mut i = 4;
    let mut v = value;
    tmp[i] = (v & 127) as u8;
    while v >= 128 {
        v >>= 7;
        i -= 1;
        tmp[i] = 128 | ((v & 127) as u8);
    }
    buf.extend_from_slice(&tmp[i..]);
}
fn pop_var(input: &mut &[u8]) -> u32 {
    let mut n = 0;
    loop {
        let (&b, tail) = input.split_first().unwrap();
        *input = tail;
        n = n << 7 | u32::from(b & 127);
        if b < 128 {
            return n;
        }
    }
}
#[derive(Default)]
struct Term {
    data: Vec<u8>,
    row: u32,
    column: u32,
    position: u32,
    size_at: usize,
}
impl Term {
    #[inline]
    fn close_row(&mut self) {
        if self.row == 0 {
            return;
        }
        let start = self.size_at + 1;
        let len = (self.data.len() - start) as u32 * 2; // even: not a delete
        if len < 128 {
            self.data[self.size_at] = len as u8;
        } else {
            let mut tmp = Vec::with_capacity(5);
            var(&mut tmp, len);
            let end = self.data.len();
            self.data.resize(end + tmp.len() - 1, 0);
            self.data.copy_within(start..end, self.size_at + tmp.len());
            self.data[self.size_at..self.size_at + tmp.len()].copy_from_slice(&tmp);
        }
    }
    #[inline]
    fn record(&mut self, row: u32, col: u32, pos: u32) {
        if self.row != row {
            self.close_row();
            var(&mut self.data, row - self.row);
            self.size_at = self.data.len();
            self.data.push(0);
            self.row = row;
            self.column = 0;
            self.position = 0;
        }
        if self.column != col {
            self.data.push(1);
            var(&mut self.data, col);
            self.column = col;
            self.position = 0;
        }
        var(&mut self.data, pos - self.position + 2);
        self.position = pos;
    }
}
#[derive(Default)]
struct Terms {
    stems: FxHashMap<Box<[u8]>, usize>,
    spellings: FxHashMap<Box<[u8]>, usize>,
    lists: Vec<Term>,
}
struct Locations<'a> {
    terms: &'a mut Terms,
    row: u32,
    column: u32,
    count: u32,
    budget: &'a mut usize,
    last: usize,
}
impl Locations<'_> {
    #[inline]
    fn append(&mut self, id: usize, flags: c_int) {
        if flags & ffi::FTS5_TOKEN_COLOCATED == 0 || self.count == 0 { self.count += 1; }
        self.terms.lists[id].record(self.row, self.column, self.count - 1);
        self.last = id;
        *self.budget += 3;
    }
}
// SQLite calls back synchronously; token storage is valid only for this call.
unsafe extern "C" fn token(
    context: *mut c_void,
    flags: c_int,
    word: *const c_char,
    len: c_int,
    _: c_int,
    _: c_int,
) -> c_int {
    if word.is_null() || len < 0 {
        return ffi::SQLITE_ERROR;
    }
    let ctx = unsafe { &mut *context.cast::<Locations<'_>>() };
    let bytes = unsafe { std::slice::from_raw_parts(word.cast::<u8>(), (len as usize).min(32768)) };
    let id = match ctx.terms.stems.get(bytes) {
        Some(id) => *id,
        None => {
            let id = ctx.terms.lists.len();
            ctx.terms.lists.push(Term::default());
            ctx.terms.stems.insert(bytes.into(), id);
            *ctx.budget += bytes.len() + 112;
            id
        }
    };
    ctx.append(id, flags);
    ffi::SQLITE_OK
}

impl Tokenizer {
    fn native(&self, ctx: &mut Locations<'_>, bytes: &[u8]) -> Result<()> {
        let rc = unsafe { (self.methods.xTokenize.unwrap())(self.raw,
            (ctx as *mut Locations<'_>).cast(), ffi::FTS5_TOKENIZE_DOCUMENT,
            bytes.as_ptr().cast(), c_int::try_from(bytes.len())?, Some(token)) };
        require(rc == ffi::SQLITE_OK, "bulk tokenization failed")
    }
    fn column(&self, ctx: &mut Locations<'_>, text: &[u8]) -> Result<()> {
        // unicode61 with its default categories treats every non-alphanumeric
        // ASCII byte as a separator. Never split a Unicode run on its interior:
        // combining marks and the actual folding rules still belong to SQLite.
        for run in text.split(|b| b.is_ascii() && !b.is_ascii_alphanumeric()).filter(|s| !s.is_empty()) {
            if run.len() <= 80 && run.is_ascii() {
                if let Some(&id) = ctx.terms.spellings.get(run) { ctx.append(id, 0); continue; }
                let before = ctx.count;
                self.native(ctx, run)?;
                require(ctx.count == before + 1, "unexpected ASCII token boundary")?;
                ctx.terms.spellings.insert(run.into(), ctx.last);
                *ctx.budget += run.len() + 32;
            } else { self.native(ctx, run)?; }
        }
        Ok(())
    }
}

// Two posting leaves and the row/cell headers fit in one native page.
const PAGE: usize = super::page_image::PAGE / 2 - 256;
struct Leaves<'a> {
    block: Statement<'a>,
    separator: Statement<'a>,
    segment: u32,
    page: u32,
    data: Vec<u8>,
    offsets: Vec<u8>,
    prev_offset: usize,
    previous: Vec<u8>,
    // First row starting on a continuation page is absolute, not a delta.
    needs_absolute: bool,
}
impl<'a> Leaves<'a> {
    fn new(db: &'a Connection, segment: u32) -> Result<Self> {
        Ok(Self {
            block: db.prepare("INSERT INTO search_fts_data(id,block) VALUES(?1,?2)")?,
            separator: db
                .prepare("INSERT INTO search_fts_idx(segid,term,pgno) VALUES(?1,?2,?3)")?,
            segment,
            page: 1,
            data: vec![0; 4],
            offsets: Vec::new(),
            prev_offset: 0,
            previous: Vec::new(),
            needs_absolute: true,
        })
    }
    fn flush(&mut self) -> Result<()> {
        if self.data.len() == 4 {
            return Ok(());
        }
        let len = u16::try_from(self.data.len())?;
        self.data[2..4].copy_from_slice(&len.to_be_bytes());
        self.data.extend_from_slice(&self.offsets);
        self.block.execute(params![
            (i64::from(self.segment) << 37) | i64::from(self.page),
            &self.data
        ])?;
        self.page += 1;
        self.data.truncate(4);
        self.data.fill(0);
        self.offsets.clear();
        self.prev_offset = 0;
        self.previous.clear();
        self.needs_absolute = true;
        Ok(())
    }
    fn term(&mut self, key: &[u8], mut rows: &[u8]) -> Result<()> {
        // Termless continuation leaves belong to the preceding list. Only
        // leaves with a first term need a separator in the sparse index.
        if self.data.len() > 4
            && (self.offsets.is_empty()
                || self.data.len() + self.offsets.len() + key.len() + 16 >= PAGE)
        {
            self.flush()?;
        }
        let offset = self.data.len();
        let prefix = if self.offsets.is_empty() {
            let split: &[u8] = if self.page == 1 { &[] } else { key };
            self.separator
                .execute(params![self.segment, split, i64::from(self.page) * 2])?;
            0
        } else {
            let p = self
                .previous
                .iter()
                .zip(key)
                .take_while(|(a, b)| a == b)
                .count();
            var(&mut self.data, p as u32);
            p
        };
        var(&mut self.offsets, (offset - self.prev_offset) as u32);
        self.prev_offset = offset;
        var(&mut self.data, (key.len() - prefix) as u32);
        self.data.extend_from_slice(&key[prefix..]);
        self.previous.clear();
        self.previous.extend_from_slice(key);
        self.needs_absolute = false;
        let mut row = 0;
        let mut first = true;
        while !rows.is_empty() {
            let delta = pop_var(&mut rows);
            row += delta;
            let size = pop_var(&mut rows);
            let (mut positions, rest) = rows.split_at((size / 2) as usize);
            rows = rest;
            if !first && self.data.len() + self.offsets.len() + positions.len() + 12 > PAGE {
                self.flush()?;
            }
            let absolute = self.needs_absolute;
            if absolute {
                let off = self.data.len() as u16;
                self.data[..2].copy_from_slice(&off.to_be_bytes());
                self.needs_absolute = false;
            }
            var(&mut self.data, if first || absolute { row } else { delta });
            var(&mut self.data, size);
            first = false;
            // Even very long terms and position lists stay below FTS5's u16
            // leaf offset limit. Like the native writer, split only at a varint.
            while self.data.len() + positions.len() > PAGE.max(self.data.len() + 4) {
                let mut n = PAGE.saturating_sub(self.data.len()).min(positions.len());
                while n > 0 && positions[n - 1] >= 128 {
                    n -= 1;
                }
                self.data.extend_from_slice(&positions[..n]);
                positions = &positions[n..];
                self.flush()?;
            }
            self.data.extend_from_slice(positions);
        }
        Ok(())
    }
}

pub(super) struct Bulk<'a> {
    db: &'a Connection,
    tokenizer: Tokenizer,
    terms: Terms,
    sizes: Statement<'a>,
    totals: [u32; 4],
    rows: u32,
    budget: usize,
    leaves: Vec<u32>,
}
impl<'a> Bulk<'a> {
    pub(super) fn new(db: &'a Connection) -> Result<Self> {
        Ok(Self {
            db,
            tokenizer: Tokenizer::open(db)?,
            terms: Terms::default(),
            sizes: db.prepare("INSERT INTO search_fts_docsize(id,sz) VALUES(?1,?2)")?,
            totals: [0; 4],
            rows: 0,
            budget: 0,
            leaves: Vec::new(),
        })
    }
    pub(super) fn document(&mut self, rowid: i64, columns: [&str; 4]) -> Result<()> {
        self.rows += 1;
        require(i64::from(self.rows) == rowid, "unordered search document")?;
        let mut sizes = Vec::with_capacity(12);
        for (i, col) in columns.into_iter().enumerate() {
            let mut ctx = Locations {
                terms: &mut self.terms,
                row: self.rows,
                column: i as u32,
                count: 0,
                budget: &mut self.budget,
                last: 0,
            };
            self.tokenizer.column(&mut ctx, col.as_bytes())?;
            self.totals[i] += ctx.count;
            var(&mut sizes, ctx.count);
        }
        self.sizes.execute(params![rowid, sizes])?;
        // No partial run is queryable. Both the final and intermediate runs
        // below are complete FTS5 segments with disjoint ranges of rowids.
        if self.budget >= 20 << 20 {
            self.flush()?;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        if self.terms.stems.is_empty() {
            return Ok(());
        }
        let segment = u32::try_from(self.leaves.len() + 1)?;
        require(segment < 65536, "too many search segments")?;
        let mut writer = Leaves::new(self.db, segment)?;
        self.terms.spellings.clear();
        let mut terms: Vec<_> = self.terms.stems.drain().collect();
        terms.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        let mut key = vec![b'0']; // native FTS5_MAIN_PREFIX
        for (word, index) in terms {
            let term = &mut self.terms.lists[index];
            key.truncate(1);
            key.extend_from_slice(&word);
            term.close_row();
            writer.term(&key, &term.data)?;
        }
        writer.flush()?;
        self.leaves.push(writer.page - 1);
        self.terms.lists.clear();
        self.budget = 0;
        Ok(())
    }
    pub(super) fn finish(mut self) -> Result<()> {
        self.flush()?;
        let mut stats = Vec::new();
        var(&mut stats, self.rows);
        for count in self.totals {
            var(&mut stats, count);
        }
        self.db
            .execute("UPDATE search_fts_data SET block=?1 WHERE id=1", [stats])?;
        let mut structure: Vec<u8> =
            self.db
                .query_row("SELECT block FROM search_fts_data WHERE id=10", [], |r| {
                    r.get(0)
                })?;
        // Refuse an unexpected format instead of overwriting an existing
        // structure. Preserve the SQLite configuration cookie (first 4 bytes).
        require(
            structure.get(4..) == Some(&[0, 0, 0]),
            "expected empty FTS5 structure",
        )?;
        structure.truncate(4);
        let n = self.leaves.len() as u32;
        var(&mut structure, 1);
        var(&mut structure, n);
        var(&mut structure, self.leaves.iter().sum());
        var(&mut structure, 0);
        var(&mut structure, n);
        for (i, pages) in self.leaves.into_iter().enumerate() {
            var(&mut structure, (i + 1) as u32);
            var(&mut structure, 1);
            var(&mut structure, pages);
        }
        self.db.execute(
            "UPDATE search_fts_data SET block=?1 WHERE id=10",
            [structure],
        )?;
        Ok(())
    }
}
