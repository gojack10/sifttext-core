use crate::{
    Policy,
    index::{self, Mode},
    leviathan_search::{self, Document},
};
use serde_json::json;
use std::{fs, path::Path};

fn id(n: usize) -> String {
    format!("70000000-0000-4000-8000-{n:012}")
}
fn fixture(p: &Path, x: &str, y: &str, extra: bool) {
    fs::create_dir_all(p.join("trees")).unwrap();
    for n in 1..=if extra { 5 } else { 4 } {
        let dir = p.join("nodes").join(id(n));
        fs::create_dir_all(dir.join("children")).unwrap();
        let fields = json!({"id":id(n),"name":if n==2{x}else if n==3{y}else{"root"},"micro_summary":"Exact source summary Ω", "status":"in_progress", "see_also":[]});
        let text = "---\n".to_owned()
            + &fields
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| format!("{k}: {v}\n"))
                .collect::<String>()
            + "---\nAuthored body.\n";
        fs::write(dir.join("node.md"), text).unwrap();
        let children: Vec<_> = if n == 1 {
            (2..=if extra { 5 } else { 4 }).map(id).collect()
        } else {
            vec![]
        };
        fs::write(
            dir.join("children.order"),
            if children.is_empty() {
                String::new()
            } else {
                children.join("\n") + "\n"
            },
        )
        .unwrap();
        for child in children {
            let link = dir.join("children").join(&child);
            if !link.exists() {
                std::os::unix::fs::symlink(format!("../../{child}"), link).unwrap();
            }
        }
    }
    let root = p.join("trees").join(id(1));
    if !root.exists() {
        std::os::unix::fs::symlink(format!("../nodes/{}", id(1)), root).unwrap();
    }
}
fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
fn combined_dependencies_and_opaque_byte_bound_records() {
    let dirs: Vec<_> = (0..4).map(|_| tempfile::tempdir().unwrap()).collect();
    let (main, x, combined, full) = (
        dirs[0].path(),
        dirs[1].path(),
        dirs[2].path(),
        dirs[3].path(),
    );
    fixture(main, "old X", "changed Y", false);
    fixture(x, "changed X", "old Y", false);
    fixture(combined, "changed X", "changed Y", true);
    fixture(full, "changed X", "changed Y", true);
    let policy = policy();
    for p in [main, x] {
        assert_eq!(
            index::refresh(&policy, p, None, Mode::Full, None)
                .unwrap()
                .status,
            "current"
        );
    }
    let m = index::prepare_reuse(&policy, main, None).unwrap().unwrap();
    let x = index::prepare_reuse(&policy, x, None).unwrap().unwrap();
    // A handle owns verified values, not live SQLite pages. Tampering after
    // verification cannot inject a forged record into the combined candidate.
    rusqlite::Connection::open(main.join(".sifttext/index.sqlite"))
        .unwrap()
        .execute("UPDATE nodes SET record='{}'", [])
        .unwrap();
    crate::coherent_capture::take_work();
    let (got, reuse) =
        index::refresh_prepared(&policy, combined, None, &m, Some(&x), None).unwrap();
    let capture_work = crate::coherent_capture::take_work();
    // Freshness barriers reread bytes without reparsing the captured nodes.
    // Record reuse does not replace the shared capture or add a second parser.
    assert_eq!(capture_work.parses, 5);
    assert!(capture_work.inventories > 0);
    assert_eq!(got.status, "current");
    assert_eq!(reuse.accepted_main, 2);
    assert_eq!(reuse.prepared_worktree, 1);
    assert_eq!(reuse.computed, 2);
    assert_eq!(reuse.changed_dependencies, 1);
    assert_eq!(reuse.search_bodies_reused, 4);
    let expected = index::refresh(&policy, full, None, Mode::Full, None).unwrap();
    assert_eq!(got.nodes, expected.nodes);
    assert_eq!(got.links, expected.links);
    assert_eq!(got.version, expected.version);
    assert!(index::prepare_reuse(&policy, main, None).unwrap().is_none());
    let node = combined.join("nodes").join(id(2)).join("node.md");
    fs::write(
        &node,
        fs::read_to_string(&node)
            .unwrap()
            .replace("changed X", "uncommitted X"),
    )
    .unwrap();
    let (now, reuse) =
        index::refresh_prepared(&policy, combined, None, &m, Some(&x), None).unwrap();
    assert_eq!(now.status, "current");
    assert_eq!(reuse.prepared_worktree, 0);
    assert_eq!(reuse.computed, 3);
    assert!(now.nodes.iter().any(|r| r["name"] == "uncommitted X"));
}

#[test]
fn prepared_refresh_preserves_coherent_capture_race_rejection() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path(), "X", "Y", false);
    let p = policy();
    index::refresh(&p, dir.path(), None, Mode::Full, None).unwrap();
    let prepared = index::prepare_reuse(&p, dir.path(), None).unwrap().unwrap();
    let db = dir.path().join(".sifttext/index.sqlite");
    let before = fs::read(&db).unwrap();
    let source = dir.path().join("nodes").join(id(2)).join("node.md");
    crate::coherent_capture::on_inventory(move || {
        let replacement = source.with_extension("replacement");
        fs::write(&replacement, fs::read(&source).unwrap()).unwrap();
        fs::rename(&replacement, &source).unwrap();
    });
    let (view, reuse) =
        index::refresh_prepared(&p, dir.path(), None, &prepared, None, None).unwrap();
    assert_ne!(view.status, "current");
    assert_eq!(reuse.accepted_main, 0);
    assert_eq!(reuse.computed, 0);
    assert_eq!(fs::read(&db).unwrap(), before);
    assert_eq!(
        index::refresh_prepared(&p, dir.path(), None, &prepared, None, None)
            .unwrap()
            .0
            .status,
        "current"
    );
}

#[test]
fn incompatible_or_forged_persistent_data_has_no_reuse_receipt() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path(), "X", "Y", false);
    let p = policy();
    for key in ["schema", "processing", "version"] {
        assert_eq!(
            index::refresh(&p, dir.path(), None, Mode::Full, None)
                .unwrap()
                .status,
            "current"
        );
        let c = rusqlite::Connection::open(dir.path().join(".sifttext/index.sqlite")).unwrap();
        c.execute("UPDATE metadata SET value='forged' WHERE key=?1", [key])
            .unwrap();
        assert!(
            index::prepare_reuse(&p, dir.path(), None)
                .unwrap()
                .is_none()
        );
    }
    index::refresh(&p, dir.path(), None, Mode::Full, None).unwrap();
    let c = rusqlite::Connection::open(dir.path().join(".sifttext/index.sqlite")).unwrap();
    c.execute("UPDATE nodes SET name='forged'", []).unwrap();
    assert!(
        index::prepare_reuse(&p, dir.path(), None)
            .unwrap()
            .is_none()
    );
    drop(c);
    fs::remove_file(dir.path().join(".sifttext/index.sqlite")).unwrap();
    assert!(
        index::prepare_reuse(&p, dir.path(), None)
            .unwrap()
            .is_none()
    );
}

#[test]
fn inserted_identity_keeps_unrelated_postings_and_browse_order() {
    let mut c = rusqlite::Connection::open_in_memory().unwrap();
    let tx = c.transaction().unwrap();
    let a = Document {
        id: "a",
        title: "alpha",
        body: "exact alpha",
        permitted: true,
        boost: 1.0,
    };
    let b = Document {
        id: "b",
        title: "beta",
        body: "exact beta",
        permitted: true,
        boost: 1.0,
    };
    let middle = Document {
        id: "aa",
        title: "middle",
        body: "exact middle",
        permitted: true,
        boost: 1.0,
    };
    leviathan_search::retrieve(&tx, &[a, b], "exact", 10, 0, 2.0).unwrap();
    let before: String = tx
        .query_row("SELECT body FROM search_fts WHERE rowid=2", [], |r| {
            r.get(0)
        })
        .unwrap();
    let docs = [
        Document {
            id: "a",
            title: "alpha",
            body: "exact alpha",
            permitted: true,
            boost: 1.0,
        },
        middle,
        Document {
            id: "b",
            title: "beta",
            body: "exact beta",
            permitted: true,
            boost: 1.0,
        },
    ];
    let result = leviathan_search::retrieve(&tx, &docs, "", 10, 0, 2.0).unwrap();
    assert_eq!(
        result
            .1
            .iter()
            .map(|c| c.text.id.as_str())
            .collect::<Vec<_>>(),
        ["b", "aa", "a"]
    );
    assert_eq!(
        before,
        tx.query_row::<String, _, _>("SELECT body FROM search_fts WHERE rowid=2", [], |r| r
            .get(0))
            .unwrap()
    );
    assert_eq!(
        tx.query_row::<String, _, _>("SELECT id FROM search_ids WHERE rowid=2", [], |r| r.get(0))
            .unwrap(),
        "b"
    );
}
