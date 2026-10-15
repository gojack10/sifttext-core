//! Read-only, checked one-hop projection of a bounded native file snapshot.
use crate::coherent_capture::{Capture, Relationships};
use crate::{Entry, Policy, Result, require, uuid};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub micro_summary: String,
    pub path: PathBuf,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Related {
    #[serde(flatten)]
    pub node: Identity,
    pub reason: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Neighborhood {
    pub parent: Option<Identity>,
    pub children: Vec<Identity>,
    pub outgoing: Vec<Related>,
    pub incoming: Vec<Related>,
}

#[derive(Debug, Serialize)]
pub struct Projection {
    pub version: String,
    pub neighborhood: Neighborhood,
}

/// Inspection-only barrier: compute first, then recheck the source before returning.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PauseBeforeAccept {
    pub ready: PathBuf,
    pub release: PathBuf,
}

impl PauseBeforeAccept {
    fn wait(&self, store: &Path) -> Result<()> {
        for path in [&self.ready, &self.release] {
            let parent = fs::canonicalize(path.parent().ok_or("missing barrier parent")?)?;
            require(!parent.starts_with(store), "barrier must be outside store")?;
        }
        require(self.ready != self.release, "barrier paths must differ")?;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.ready)?
            .write_all(b"captured\n")?;
        let start = Instant::now();
        while !self.release.try_exists()? {
            require(
                start.elapsed() < Duration::from_secs(10),
                "inspection barrier expired",
            )?;
            thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
}

fn logical_entry(path: &Path) -> bool {
    let parts: Vec<_> = path.components().collect();
    parts.first().is_some_and(|p| p.as_os_str() == "trees")
        || (parts.len() >= 3
            && parts[0].as_os_str() == "nodes"
            && ["node.md", "children.order", "children"]
                .iter()
                .any(|p| parts[2].as_os_str() == *p))
}

/// Uses the existing native parser and checked graph policy, not an alternate
/// graph schema. `readable_ids` is explicit bounded projection authority, not
/// universal access enforcement. The selected node itself must be readable.
pub fn neighborhood(
    policy: &Policy,
    path: &Path,
    id: &str,
    readable_ids: &BTreeSet<String>,
    pause: Option<&PauseBeforeAccept>,
) -> Result<Projection> {
    require(
        uuid(id) && readable_ids.iter().all(|id| uuid(id)),
        "invalid projection UUID",
    )?;
    require(readable_ids.contains(id), "node unavailable")?;
    let capture = Capture::read(path, None)?;
    let store = &capture.store;
    policy.validate(store)?;
    let relationships = Relationships::new(store);
    let node = store.nodes.get(id).ok_or("node unavailable")?;
    let identity = |id: &str| {
        let n = &store.nodes[id];
        Identity {
            id: n.id.clone(),
            name: n.name.clone(),
            micro_summary: n.micro_summary.clone(),
            path: store.path.join("nodes").join(id).join("node.md"),
        }
    };
    let parent = relationships
        .parents
        .get(id)
        .filter(|parent| readable_ids.contains(**parent))
        .map(|id| identity(id));
    let children = store.children[id]
        .iter()
        .filter(|id| readable_ids.contains(*id))
        .map(|id| identity(id))
        .collect();
    let outgoing = node
        .see_also
        .iter()
        .filter(|r| readable_ids.contains(&r.target))
        .map(|r| Related {
            node: identity(&r.target),
            reason: r.reason.clone(),
        })
        .collect();
    let incoming = relationships
        .incoming
        .get(id)
        .into_iter()
        .flatten()
        .filter(|(source, _)| readable_ids.contains(*source))
        .map(|(source, reason)| Related {
            node: identity(source),
            reason: (*reason).to_owned(),
        })
        .collect();
    // Exact authored bytes/order/relationship entries, relative to the store;
    // generated metadata, resources and absolute worktree location are excluded.
    let mut hash = Sha256::new();
    hash.update(b"sifttext-neighborhood-v1\0");
    for (p, e) in store.inventory.iter().filter(|(p, _)| logical_entry(p)) {
        let path = p.to_str().ok_or("non-UTF-8 snapshot path")?;
        hash.update((path.len() as u64).to_le_bytes());
        hash.update(path.as_bytes());
        let (kind, bytes) = match e {
            Entry::File(b) => (0, b.as_slice()),
            Entry::Link(p) => (1, p.to_str().ok_or("non-UTF-8 snapshot link")?.as_bytes()),
            Entry::Dir => (2, &[][..]),
        };
        hash.update([kind]);
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    let version = format!("{:x}", hash.finalize());
    let projection = Projection {
        version,
        neighborhood: Neighborhood {
            parent,
            children,
            outgoing,
            incoming,
        },
    };
    if let Some(pause) = pause {
        pause.wait(&store.path)?;
    }
    capture.unchanged_all()?;
    Ok(projection)
}
