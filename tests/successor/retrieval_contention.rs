use crate::{Policy, coherent_capture, index, search};
use rusqlite::{Connection, TransactionBehavior};
use serde_json::json;
use std::{collections::BTreeSet, fs, path::Path};

fn store(path: &Path) -> String {
    let id = "61000000-0000-4000-8000-000000000001";
    let dir = path.join("nodes").join(id);
    fs::create_dir_all(dir.join("children")).unwrap();
    fs::create_dir(path.join("trees")).unwrap();
    fs::write(dir.join("children.order"), "").unwrap();
    fs::write(
        dir.join("node.md"),
        format!("---\nid: {}\nname: \"Login\"\nmicro_summary: \"Reset users\"\nstatus: \"in_progress\"\nsee_also: []\n---\nLogin reset.\n", json!(id)),
    ).unwrap();
    std::os::unix::fs::symlink(format!("../nodes/{id}"), path.join("trees").join(id)).unwrap();
    id.to_owned()
}

// Deterministic control: at the FIRST inventory, another SQLite connection must
// already be excluded. Before the repair this succeeds in acquiring the writer
// lock, proving that safe_derived can traverse a live publisher's staging files.
#[test]
fn query_captures_only_under_the_publisher_lock() {
    let dir = tempfile::tempdir().unwrap();
    let id = store(dir.path());
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    index::refresh(&policy, dir.path(), None, index::Mode::Full, None).unwrap();
    let db = dir.path().join(".sifttext/index.sqlite");
    coherent_capture::on_inventory(move || {
        let mut contender = Connection::open(db).unwrap();
        contender.busy_timeout(std::time::Duration::ZERO).unwrap();
        assert!(
            contender
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .is_err(),
            "inventory ran before the publisher lock"
        );
    });
    let readable = BTreeSet::from([id]);
    let ranking = search::Ranking::default();
    let result = search::search(
        &policy,
        &search::Request {
            store: dir.path(),
            base_store: None,
            query: "login",
            under_node_id: None,
            readable_ids: &readable,
            limit: 10,
            offset: 0,
            ranking: &ranking,
            pause_before_accept: None,
        },
    )
    .unwrap();
    assert_eq!(result.status, "current");
    assert_eq!(result.total_matches, 1);
}

#[test]
fn local_search_discovers_scope_under_lock_without_broadening_filtered_access() {
    let dir = tempfile::tempdir().unwrap();
    let id = store(dir.path());
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    index::refresh(&policy, dir.path(), None, index::Mode::Full, None).unwrap();
    let db = dir.path().join(".sifttext/index.sqlite");
    coherent_capture::on_inventory(move || {
        let mut contender = Connection::open(db).unwrap();
        contender.busy_timeout(std::time::Duration::ZERO).unwrap();
        assert!(
            contender
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .is_err(),
            "local scope discovery inventoried before the publisher lock"
        );
    });
    let out = search::local_search(&policy, dir.path(), "login", Some(&id[..8])).unwrap();
    assert_eq!(out.status, "current");
    assert_eq!(out.total_matches, 1);
    assert_eq!(out.hits[0].id, id);
    assert!(search::local_search(&policy, dir.path(), "login", Some("missing")).is_err());
    let out = search::search(
        &policy,
        &search::Request {
            store: dir.path(),
            base_store: None,
            query: "login",
            under_node_id: None,
            readable_ids: &BTreeSet::new(),
            limit: 20,
            offset: 0,
            ranking: &search::Ranking::default(),
            pause_before_accept: None,
        },
    )
    .unwrap();
    assert_eq!(out.status, "current");
    assert_eq!(out.total_matches, 0);
    assert!(out.hits.is_empty() && out.cards.is_empty());
}

#[test]
fn inspection_captures_only_under_the_publisher_lock() {
    let dir = tempfile::tempdir().unwrap();
    store(dir.path());
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    index::refresh(&policy, dir.path(), None, index::Mode::Full, None).unwrap();
    let db = dir.path().join(".sifttext/index.sqlite");
    coherent_capture::on_inventory(move || {
        let mut contender = Connection::open(db).unwrap();
        contender.busy_timeout(std::time::Duration::ZERO).unwrap();
        assert!(
            contender
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .is_err(),
            "inspection inventory ran before the publisher lock"
        );
    });
    assert_eq!(
        crate::outlines::inspect(&policy, dir.path(), None)
            .unwrap()
            .status,
        "current"
    );
}

#[test]
fn disappearing_stage_entry_is_not_an_authored_change() {
    let dir = tempfile::tempdir().unwrap();
    store(dir.path());
    let stage = dir.path().join(".sifttext/outline-stage");
    fs::create_dir_all(&stage).unwrap();
    let file = stage.join("temporary");
    fs::write(&file, "generated bytes").unwrap();
    // Exactly the publisher's normal list -> persist/unlink -> stat interleaving.
    let listed = crate::entries(&stage).unwrap();
    fs::remove_file(file).unwrap();
    let error = crate::outlines::safe_derived(&listed[0]).unwrap_err();
    assert_eq!(
        error.downcast_ref::<std::io::Error>().unwrap().kind(),
        std::io::ErrorKind::NotFound
    );
    assert!(crate::Store::read(dir.path()).is_ok());
}
