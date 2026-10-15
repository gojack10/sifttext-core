use crate::{
    Policy,
    index::{self, Mode},
    leviathan_search::{self, Document},
    search::{self, Ranking, Request},
};
use rusqlite::Connection;
use serde_json::json;
use std::{collections::BTreeSet, fs, path::Path};

#[test]
fn postings_reuse_incremental_change_and_invocation_settings() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let mut docs = vec![
        Document {
            id: "one",
            title: "Login",
            body: "reset users",
            permitted: true,
            boost: 1.0,
        },
        Document {
            id: "two",
            title: "Other",
            body: "login",
            permitted: false,
            boost: 1.0,
        },
        Document {
            id: "three",
            title: "Untouched",
            body: "Unchanged",
            permitted: false,
            boost: 1.0,
        },
    ];
    let original = leviathan_search::retrieve(&tx, &docs, "login", 10, 0, 2.0).unwrap();
    assert_eq!(original.0, 1);
    // integrity-check has no persisted page mutation (native file test below),
    // although SQLite counts its control INSERT as one affected row.
    let before = tx.total_changes();
    leviathan_search::retrieve(&tx, &docs, "reset", 10, 0, 2.0).unwrap();
    assert_eq!(tx.total_changes() - before, 1);
    docs[0].permitted = false;
    docs[1].permitted = true;
    docs[1].boost = 2.5;
    let before = tx.total_changes();
    let changed_scope = leviathan_search::retrieve(&tx, &docs, "login", 10, 0, 3.0).unwrap();
    assert_eq!(changed_scope.1[0].text.id, "two");
    assert_eq!(tx.total_changes() - before, 1);
    docs[1].body = "unique changed record";
    let before = tx.total_changes();
    assert_eq!(
        leviathan_search::retrieve(&tx, &docs, "unique", 10, 0, 2.0)
            .unwrap()
            .0,
        1
    );
    let incremental = tx.total_changes() - before;
    tx.execute_batch("DROP TABLE search_fts").unwrap();
    let before = tx.total_changes();
    leviathan_search::retrieve(&tx, &docs, "unique", 10, 0, 2.0).unwrap();
    assert!(incremental < tx.total_changes() - before);
    docs.remove(1);
    assert_eq!(
        leviathan_search::retrieve(&tx, &docs, "unique", 10, 0, 2.0)
            .unwrap()
            .0,
        0
    );
    assert_eq!(
        tx.query_row("SELECT count(*) FROM search_fts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

fn id(n: usize) -> String {
    format!("60000000-0000-4000-8000-{n:012}")
}
#[test]
fn warm_native_index_is_byte_identical_and_full_refresh_agrees() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("trees")).unwrap();
    for n in 1..=2 {
        let p = dir.path().join("nodes").join(id(n));
        fs::create_dir_all(p.join("children")).unwrap();
        let fields = json!({"id":id(n),"name":"Login Ω", "micro_summary":"Summary \"Ω\"\tnewline\nwith \\ slash", "status":"in_progress","see_also":[], "warnings":if n==1 {"Quoted \"warning\"\ttext\nline"} else {""}});
        let text = "---\n".to_owned()
            + &fields
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| format!("{k}: {v}\n"))
                .collect::<String>()
            + "---\nReset users.\n";
        fs::write(p.join("node.md"), text).unwrap();
        fs::write(
            p.join("children.order"),
            if n == 1 { id(2) + "\n" } else { String::new() },
        )
        .unwrap();
    }
    std::os::unix::fs::symlink(
        format!("../../{}", id(2)),
        dir.path()
            .join("nodes")
            .join(id(1))
            .join("children")
            .join(id(2)),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        format!("../nodes/{}", id(1)),
        dir.path().join("trees").join(id(1)),
    )
    .unwrap();
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let readable = BTreeSet::from([id(1), id(2)]);
    let ranking = Ranking::default();
    let request = Request {
        store: dir.path(),
        base_store: None,
        query: "login",
        under_node_id: None,
        readable_ids: &readable,
        limit: 10,
        offset: 0,
        ranking: &ranking,
        pause_before_accept: None,
    };
    let first = search::search(&policy, &request).unwrap();
    assert_eq!(first.status, "current");
    let db = dir.path().join(".sifttext/index.sqlite");
    let bytes = fs::read(&db).unwrap();
    let stage = dir.path().join(".sifttext/outline-stage");
    assert!(!stage.exists());
    let warm = search::search(&policy, &request).unwrap();
    assert_eq!(first.version, warm.version);
    assert_eq!(bytes, fs::read(&db).unwrap());
    assert!(!stage.exists());
    // Exercise reuse directly so a decode failure cannot hide as safe regeneration.
    let view = index::refresh(&policy, dir.path(), None, Mode::Incremental, None).unwrap();
    let c = Connection::open(&db).unwrap();
    let manifest: String = c
        .query_row("SELECT value FROM metadata WHERE key='outlines'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let store = crate::Store::read(dir.path()).unwrap();
    let reused = crate::outlines::Publication::reuse(
        &store,
        &view.nodes,
        view.version.as_deref().unwrap(),
        &serde_json::from_str(&manifest).unwrap(),
    )
    .unwrap();
    reused.unchanged(dir.path()).unwrap();
    assert!(reused.reused());
    reused.publish(dir.path()).unwrap();
    assert!(!stage.exists());
    let manifest = serde_json::from_str(&manifest).unwrap();
    // Closed-path reuse must retain acceptance-time identity and byte checks
    // for both outlines and sidecars, not just trust their cached hashes.
    for name in ["outline.full.lean", "outline.lean.meta.json"] {
        let target = dir.path().join("nodes").join(id(1)).join(name);
        for replace in [false, true] {
            let publication = crate::outlines::Publication::reuse(
                &store,
                &view.nodes,
                view.version.as_deref().unwrap(),
                &manifest,
            )
            .unwrap();
            let bytes = fs::read(&target).unwrap();
            if replace {
                let replacement = target.with_extension("replacement");
                fs::write(&replacement, &bytes).unwrap();
                fs::rename(replacement, &target).unwrap();
            } else {
                fs::write(&target, b"changed reused bytes").unwrap();
            }
            assert!(publication.unchanged(dir.path()).is_err());
            assert!(publication.publish(dir.path()).is_err());
            fs::write(&target, bytes).unwrap();
            assert!(!stage.exists());
        }
    }
    let full = index::refresh(&policy, dir.path(), None, Mode::Full, None).unwrap();
    assert_eq!(full.nodes, view.nodes);
    assert_eq!(full.version, view.version);
}
