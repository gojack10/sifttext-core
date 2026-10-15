use crate::{
    Policy,
    index::{self, Mode},
    outlines,
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

fn id(n: usize) -> String {
    format!("71000000-0000-4000-8000-{n:012}")
}
fn fixture() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir(d.path().join("trees")).unwrap();
    for n in 1..=6 {
        let p = d.path().join("nodes").join(id(n));
        fs::create_dir_all(p.join("children")).unwrap();
        let children = match n {
            1 => vec![2, 3],
            2 => vec![4],
            5 => vec![6],
            _ => vec![],
        };
        let fields = json!({"id":id(n),"name":format!("Node {n}"),"micro_summary":format!("Summary {n}"),
            "status":"in_progress","scope":"Initial scope","see_also":[]});
        fs::write(
            p.join("node.md"),
            "---\n".to_owned()
                + &fields
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}\n"))
                    .collect::<String>()
                + "---\nOriginal body.\n",
        )
        .unwrap();
        fs::write(
            p.join("children.order"),
            children.iter().map(|n| id(*n) + "\n").collect::<String>(),
        )
        .unwrap();
        for c in children {
            std::os::unix::fs::symlink(format!("../../{}", id(c)), p.join("children").join(id(c)))
                .unwrap();
        }
    }
    for n in [1, 5] {
        std::os::unix::fs::symlink(
            format!("../nodes/{}", id(n)),
            d.path().join("trees").join(id(n)),
        )
        .unwrap();
    }
    d
}
fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}
fn refresh(p: &Path) -> index::View {
    let view = index::refresh(&policy(), p, None, Mode::Incremental, None).unwrap();
    assert_eq!(view.status, "current");
    view
}
fn field(p: &Path, n: usize, key: &str, v: Value) {
    let file = p.join("nodes").join(id(n)).join("node.md");
    let text = fs::read_to_string(&file).unwrap();
    fs::write(
        file,
        text.lines()
            .map(|l| {
                if l.starts_with(&(key.to_owned() + ":")) {
                    format!("{key}: {v}\n")
                } else {
                    l.to_owned() + "\n"
                }
            })
            .collect::<String>(),
    )
    .unwrap();
}
fn snapshot(p: &Path, n: usize) -> Vec<(Vec<u8>, u64, i64, i64)> {
    [
        "outline.lean",
        "outline.full.lean",
        "outline.lean.meta.json",
        "outline.full.lean.meta.json",
    ]
    .iter()
    .map(|name| {
        let f = p.join("nodes").join(id(n)).join(name);
        let m = fs::metadata(&f).unwrap();
        (fs::read(f).unwrap(), m.ino(), m.mtime(), m.mtime_nsec())
    })
    .collect()
}
#[test]
fn body_scope_and_inactive_reason_advance_global_freshness_without_navigation_writes() {
    let d = fixture();
    let p = d.path();
    let initial = refresh(p);
    let before: Vec<_> = (1..=6).map(|n| snapshot(p, n)).collect();
    field(p, 4, "scope", json!("Changed scope"));
    let f = p.join("nodes").join(id(4)).join("node.md");
    fs::write(
        &f,
        fs::read_to_string(&f)
            .unwrap()
            .replace("Original body.", "New body."),
    )
    .unwrap();
    assert_eq!(
        outlines::inspect(&policy(), p, None).unwrap().status,
        "stale"
    );
    let after = refresh(p);
    assert_ne!(initial.version, after.version);
    assert_eq!(
        after.nodes.iter().find(|r| r["id"] == id(4)).unwrap()["content"],
        "New body.\n"
    );
    assert_eq!(
        outlines::inspect(&policy(), p, None).unwrap().version,
        after.version
    );
    assert_eq!(before, (1..=6).map(|n| snapshot(p, n)).collect::<Vec<_>>());
    assert!(!p.join(".sifttext/outline-stage").exists());
}
#[test]
fn prepared_refresh_advances_body_freshness_without_replacing_navigation() {
    let d = fixture();
    let p = d.path();
    let initial = refresh(p);
    let prepared = index::prepare_reuse(&policy(), p, None).unwrap().unwrap();
    let before: Vec<_> = (1..=6).map(|n| snapshot(p, n)).collect();
    let file = p.join("nodes").join(id(4)).join("node.md");
    fs::write(
        &file,
        fs::read_to_string(&file)
            .unwrap()
            .replace("Original body.", "Prepared boundary body."),
    )
    .unwrap();
    let (view, reuse) = index::refresh_prepared(&policy(), p, None, &prepared, None, None).unwrap();
    assert_eq!(view.status, "current");
    assert_ne!(view.version, initial.version);
    assert_eq!(reuse.accepted_main, 5);
    assert_eq!(reuse.computed, 1);
    assert_eq!(reuse.changed_rows, 1);
    assert_eq!(
        view.nodes.iter().find(|r| r["id"] == id(4)).unwrap()["content"],
        "Prepared boundary body.\n"
    );
    assert_eq!(
        outlines::inspect(&policy(), p, None).unwrap().version,
        view.version
    );
    assert_eq!(before, (1..=6).map(|n| snapshot(p, n)).collect::<Vec<_>>());
    assert!(!p.join(".sifttext/outline-stage").exists());
}
#[test]
fn summary_and_subtree_move_update_dependencies_not_unrelated_branches() {
    let d = fixture();
    let p = d.path();
    refresh(p);
    let root = snapshot(p, 1);
    let unrelated: Vec<_> = [3, 5, 6].iter().map(|n| snapshot(p, *n)).collect();
    field(p, 4, "micro_summary", json!("Different displayed summary"));
    refresh(p);
    assert_ne!(snapshot(p, 1), root);
    assert_eq!(
        unrelated,
        [3, 5, 6]
            .iter()
            .map(|n| snapshot(p, *n))
            .collect::<Vec<_>>()
    );
    let subtree: Vec<_> = [2, 4].iter().map(|n| snapshot(p, *n)).collect();
    let old = p.join("nodes").join(id(1));
    let new = p.join("nodes").join(id(5));
    fs::rename(
        old.join("children").join(id(2)),
        new.join("children").join(id(2)),
    )
    .unwrap();
    fs::write(old.join("children.order"), id(3) + "\n").unwrap();
    fs::write(new.join("children.order"), id(6) + "\n" + &id(2) + "\n").unwrap();
    refresh(p);
    assert_eq!(
        subtree,
        [2, 4].iter().map(|n| snapshot(p, *n)).collect::<Vec<_>>()
    );
    let before: Vec<_> = (1..=6)
        .map(|n| snapshot(p, n).into_iter().map(|s| s.0).collect::<Vec<_>>())
        .collect();
    let full = index::refresh(&policy(), p, None, Mode::Full, None).unwrap();
    assert_eq!(full.status, "current");
    assert_eq!(
        before,
        (1..=6)
            .map(|n| snapshot(p, n).into_iter().map(|s| s.0).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    );
}
#[test]
fn forged_identity_and_costs_are_not_reused_even_with_resealed_manifest() {
    let d = fixture();
    let p = d.path();
    refresh(p);
    let before = snapshot(p, 4);
    let relative = PathBuf::from(format!("nodes/{}/outline.full.lean", id(4)));
    let file = p.join(&relative);
    let text = fs::read_to_string(&file).unwrap();
    let forged = text
        .lines()
        .map(|l| {
            if l.starts_with("source_version:") {
                "source_version:forged\n".to_owned()
            } else {
                l.to_owned() + "\n"
            }
        })
        .collect::<String>();
    fs::write(&file, &forged).unwrap();
    let db = rusqlite::Connection::open(p.join(".sifttext/index.sqlite")).unwrap();
    let value: String = db
        .query_row("SELECT value FROM metadata WHERE key='outlines'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let mut manifest: Value = serde_json::from_str(&value).unwrap();
    manifest["files"][relative.to_str().unwrap()] = json!(crate::sha256(forged.as_bytes()));
    db.execute(
        "UPDATE metadata SET value=?1 WHERE key='outlines'",
        [manifest.to_string()],
    )
    .unwrap();
    drop(db);
    assert_eq!(
        outlines::inspect(&policy(), p, None).unwrap().status,
        "stale"
    );
    refresh(p);
    assert_eq!(
        snapshot(p, 4).into_iter().map(|s| s.0).collect::<Vec<_>>(),
        before.into_iter().map(|s| s.0).collect::<Vec<_>>()
    );
    let sidecar = file.with_file_name("outline.full.lean.meta.json");
    let mut meta: Value = serde_json::from_slice(&fs::read(&sidecar).unwrap()).unwrap();
    meta["tokens"] = json!(1);
    fs::write(&sidecar, meta.to_string() + "\n").unwrap();
    assert_eq!(
        outlines::inspect(&policy(), p, None).unwrap().status,
        "stale"
    );
    refresh(p);
    assert_ne!(
        serde_json::from_slice::<Value>(&fs::read(sidecar).unwrap()).unwrap()["tokens"],
        1
    );
}
