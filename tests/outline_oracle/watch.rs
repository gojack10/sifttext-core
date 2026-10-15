// Shared native directory events retain the transient-stage assertion.
#[path = "../common/directory_events.rs"]
mod directory_events;

#[test]
fn body_only_refresh_never_creates_a_transient_outline_stage() {
    let dir = TempDir::new().unwrap();
    let store = fs::canonicalize(dir.path()).unwrap().join("store");
    materialize(&store, &dependency_specimen());
    let (initial, _) = incremental_call(&store);
    let generated = outline_identity(&store);
    let cache = store.join(".sifttext");
    let stage = cache.join("outline-stage");

    // A planted create/remove pair proves this is a live event observer, not a
    // final-state or polling check that silently misses transient directories.
    let control = directory_events::Watch::new(&cache);
    fs::create_dir(&stage).unwrap();
    fs::remove_dir(&stage).unwrap();
    assert!(
        control.finish().contains(&stage),
        "watcher missed planted transient stage"
    );

    let watch = directory_events::Watch::new(&cache);
    upsert_dependency_field(&store, 3, "scope", json!("Changed authored scope only."));
    let source = store.join("nodes").join(incremental_id(3)).join("node.md");
    fs::write(
        &source,
        fs::read_to_string(&source)
            .unwrap()
            .replace("Original authored body.", "Changed body only."),
    )
    .unwrap();
    let authored = authored_snapshot(&store);
    let (ok, stale) = native(&store, "outline_inspect", None);
    assert!(!ok);
    assert_eq!(stale["status"], "stale");
    let (current, trace) = incremental_call(&store);
    let events = watch.finish();
    assert!(
        !events.contains(&stage),
        "body-only refresh created a transient stage: {events:?}"
    );
    assert!(!stage.exists());
    assert_ne!(current["version"], initial["version"]);
    assert_eq!(trace["outline_files_staged"], 0);
    assert_eq!(outline_identity(&store), generated);
    assert_eq!(authored_snapshot(&store), authored);
    let row = current["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == incremental_id(3))
        .unwrap();
    assert_eq!(row["scope"], "Changed authored scope only.");
    assert_eq!(row["content"], "Changed body only.\n");
    let (ok, inspected) = native(&store, "outline_inspect", None);
    assert!(ok);
    assert_eq!(inspected["version"], current["version"]);
    let bpe = ORACLE_BPE.clone();
    assert_dependency_equality(&store, &current, &bpe);
}
