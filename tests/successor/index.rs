use crate::{
    Policy, Store,
    index::{self, Mode, PauseBeforeAccept},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};
fn id(n: usize) -> String {
    format!("30000000-0000-4000-8000-{n:012}")
}
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("trees")).unwrap();
    for n in 1..=3 {
        let p = dir.path().join("nodes").join(id(n));
        fs::create_dir_all(p.join("children")).unwrap();
        let fields = json!({"id":id(n),"name":format!("Node {n}"),"micro_summary":format!("Summary {n}"),
            "status":"in_progress","scope":"Exact scope Ω","custom":[1,"opaque"],
            "see_also":if n==2 {json!([{"target":id(3),"reason":"Exact\nreason Ω"}])} else {json!([])}});
        let text = "---\n".to_owned()
            + &fields
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| format!("{k}: {v}\n"))
                .collect::<String>()
            + "---\nExact body\r\n";
        fs::write(p.join("node.md"), text).unwrap();
        let children = if n == 1 { vec![3, 2] } else { vec![] };
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
    std::os::unix::fs::symlink(
        format!("../nodes/{}", id(1)),
        dir.path().join("trees").join(id(1)),
    )
    .unwrap();
    dir
}
fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}
fn refresh(p: &Path, mode: Mode) -> index::View {
    index::refresh(&policy(), p, None, mode, None).unwrap()
}
fn field(p: &Path, n: usize, key: &str, v: Value) {
    let p = p.join("nodes").join(id(n)).join("node.md");
    let s = fs::read_to_string(&p).unwrap();
    let mut out = String::new();
    for line in s.split_inclusive('\n') {
        if line.starts_with(&(key.to_owned() + ":")) {
            out.push_str(&format!("{key}: {v}\n"));
        } else {
            out.push_str(line);
        }
    }
    fs::write(p, out).unwrap();
}
#[test]
fn sqlite_changed_node_restore_revert_and_rebuild_preserve_authored_bytes() {
    let d = fixture();
    let p = d.path();
    let original = Store::read(p).unwrap();
    let full = refresh(p, Mode::Full);
    assert_eq!(full.status, "current");
    // Independent complete expectations, not records projected by the index or Store.
    let mut expected = vec![
        json!({"id":id(1),"name":"Node 1","micro_summary":"Summary 1",
            "status":"in_progress","scope":"Exact scope Ω","custom":[1,"opaque"],
            "see_also":[],"content":"Exact body\r\n","children":[id(3),id(2)]}),
        json!({"id":id(2),"name":"Node 2","micro_summary":"Summary 2",
            "status":"in_progress","scope":"Exact scope Ω","custom":[1,"opaque"],
            "see_also":[{"target":id(3),"reason":"Exact\nreason Ω"}],
            "content":"Exact body\r\n","children":[]}),
        json!({"id":id(3),"name":"Node 3","micro_summary":"Summary 3",
            "status":"in_progress","scope":"Exact scope Ω","custom":[1,"opaque"],
            "see_also":[],"content":"Exact body\r\n","children":[]}),
    ];
    assert_eq!(full.nodes, expected);
    assert_eq!(
        full.links,
        vec![index::Link {
            source: id(2),
            target: id(3),
            reason: "Exact\nreason Ω".into(),
        }]
    );
    let db = rusqlite::Connection::open_with_flags(
        p.join(".sifttext/index.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        db.query_row("select name from nodes where id=?1", [id(2)], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "Node 2"
    );
    let sql_rows: Vec<(String, String)> = db
        .prepare("select id,name from nodes order by id")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        sql_rows,
        vec![
            (id(1), "Node 1".into()),
            (id(2), "Node 2".into()),
            (id(3), "Node 3".into())
        ]
    );
    drop(db);
    let db = rusqlite::Connection::open(p.join(".sifttext/index.sqlite")).unwrap();
    db.execute_batch(
        "DELETE FROM links; UPDATE nodes SET name='forged';
        INSERT INTO links VALUES ('orphan','missing','forged',0);",
    )
    .unwrap();
    drop(db);
    let recovered = refresh(p, Mode::Incremental);
    assert_eq!(recovered.nodes, full.nodes);
    assert_eq!(recovered.links, full.links);
    let db = rusqlite::Connection::open(p.join(".sifttext/index.sqlite")).unwrap();
    assert_eq!(
        db.query_row("select name from nodes where id=?1", [id(2)], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "Node 2"
    );
    drop(db);
    let after = Store::read(p).unwrap();
    for (path, entry) in &original.inventory {
        assert_eq!(after.inventory.get(path), Some(entry));
    }
    field(p, 2, "name", json!("Uncommitted Ω"));
    field(p, 2, "see_also", json!([]));
    let edit = refresh(p, Mode::Incremental);
    assert_ne!(edit.version, full.version);
    assert!(edit.links.is_empty());
    expected[1]["name"] = json!("Uncommitted Ω");
    expected[1]["see_also"] = json!([]);
    assert_eq!(edit.nodes, expected);
    let again = refresh(p, Mode::Full);
    assert_eq!(again.version, edit.version);
    assert_eq!(again.nodes, expected);
    assert_eq!(again.links, edit.links);
    // A real move and sibling reorder must change complete authored-order rows.
    let root = p.join("nodes").join(id(1));
    let parent = p.join("nodes").join(id(2));
    fs::write(root.join("children.order"), id(2) + "\n" + &id(3) + "\n").unwrap();
    expected[0]["children"] = json!([id(2), id(3)]);
    let reordered = refresh(p, Mode::Incremental);
    assert_eq!(reordered.nodes, expected);
    assert_eq!(refresh(p, Mode::Full).nodes, expected);
    fs::rename(
        root.join("children").join(id(3)),
        parent.join("children").join(id(3)),
    )
    .unwrap();
    fs::write(root.join("children.order"), id(2) + "\n").unwrap();
    fs::write(parent.join("children.order"), id(3) + "\n").unwrap();
    expected[0]["children"] = json!([id(2)]);
    expected[1]["children"] = json!([id(3)]);
    let moved = refresh(p, Mode::Incremental);
    assert_eq!(moved.nodes, expected);
    let moved_full = refresh(p, Mode::Full);
    assert_eq!(moved_full.version, moved.version);
    assert_eq!(moved_full.nodes, expected);
    assert_eq!(moved_full.links, moved.links);
    fs::rename(
        parent.join("children").join(id(3)),
        root.join("children").join(id(3)),
    )
    .unwrap();
    fs::write(root.join("children.order"), id(3) + "\n" + &id(2) + "\n").unwrap();
    fs::write(parent.join("children.order"), "").unwrap();
    assert_eq!(refresh(p, Mode::Incremental).nodes, edit.nodes);
    let run = |args: &[&str]| {
        assert!(
            Command::new("git")
                .current_dir(p)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    };
    run(&["init", "-q"]);
    run(&["add", "nodes", "trees"]);
    run(&[
        "-c",
        "user.name=Oracle",
        "-c",
        "user.email=oracle@example.invalid",
        "commit",
        "-qm",
        "index specimen",
    ]);
    field(p, 2, "name", json!("Commit edit"));
    run(&["add", "nodes"]);
    run(&[
        "-c",
        "user.name=Oracle",
        "-c",
        "user.email=oracle@example.invalid",
        "commit",
        "-qm",
        "edit",
    ]);
    assert_eq!(
        refresh(p, Mode::Incremental).nodes[1]["name"],
        "Commit edit"
    );
    assert!(
        Command::new("git")
            .current_dir(p)
            .env("GIT_COMMITTER_NAME", "Oracle")
            .env("GIT_COMMITTER_EMAIL", "oracle@example.invalid")
            .args(["revert", "--no-edit", "HEAD"])
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(refresh(p, Mode::Incremental).version, edit.version);
    field(p, 2, "name", json!("Restorable edit"));
    assert_eq!(
        refresh(p, Mode::Incremental).nodes[1]["name"],
        "Restorable edit"
    );
    run(&["restore", "nodes"]);
    assert_eq!(refresh(p, Mode::Incremental).nodes, edit.nodes);
    fs::remove_file(p.join(".sifttext/index.sqlite")).unwrap();
    assert_eq!(refresh(p, Mode::Incremental).nodes, edit.nodes);
}
#[test]
fn sparse_overlay_edits_hydration_isolation_and_unsafe_links() {
    let base = fixture();
    let sparse = fixture();
    let aliases = tempfile::tempdir().unwrap();
    let alias = aliases.path().join("store");
    std::os::unix::fs::symlink(base.path(), &alias).unwrap();
    assert_eq!(refresh(&alias, Mode::Full).status, "error");
    assert_eq!(
        index::refresh(&policy(), sparse.path(), Some(&alias), Mode::Full, None)
            .unwrap()
            .status,
        "error"
    );
    assert!(!base.path().join(".sifttext").exists());
    fs::remove_dir_all(sparse.path().join("nodes").join(id(3))).unwrap();
    let first = index::refresh(
        &policy(),
        sparse.path(),
        Some(base.path()),
        Mode::Full,
        None,
    )
    .unwrap();
    assert_eq!(first.nodes, refresh(base.path(), Mode::Full).nodes);
    assert!(!sparse.path().join("nodes").join(id(3)).exists());
    field(sparse.path(), 2, "name", json!("Sparse edit"));
    let edited = index::refresh(
        &policy(),
        sparse.path(),
        Some(base.path()),
        Mode::Incremental,
        None,
    )
    .unwrap();
    assert_ne!(first.version, edited.version);
    assert_eq!(
        refresh(base.path(), Mode::Incremental).nodes[1]["name"],
        "Node 2"
    );
    let complete = fixture();
    field(complete.path(), 2, "name", json!("Sparse edit"));
    assert_eq!(refresh(complete.path(), Mode::Full).version, edited.version);
    assert!(
        Command::new("cp")
            .args(["-R"])
            .arg(base.path().join("nodes").join(id(3)))
            .arg(sparse.path().join("nodes"))
            .status()
            .unwrap()
            .success()
    );
    let hydrated = index::refresh(
        &policy(),
        sparse.path(),
        Some(base.path()),
        Mode::Incremental,
        None,
    )
    .unwrap();
    assert_eq!(hydrated.version, edited.version);
    assert_eq!(hydrated.nodes, edited.nodes);
    let link = sparse
        .path()
        .join("nodes")
        .join(id(1))
        .join("children")
        .join(id(3));
    fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("/etc/passwd", &link).unwrap();
    assert_eq!(
        index::refresh(
            &policy(),
            sparse.path(),
            Some(base.path()),
            Mode::Full,
            None
        )
        .unwrap()
        .status,
        "error"
    );
}
#[test]
fn invalid_current_drafts_never_return_cached_rows() {
    for (key, v) in [
        ("micro_summary", json!("  ")),
        ("status", json!("unknown")),
        ("status", json!("stuck")),
        ("status", json!("resolved")),
        ("see_also", json!([{"target":id(2),"reason":"self"}])),
        (
            "see_also",
            json!([{"target":id(3),"reason":"one"},{"target":id(3),"reason":"two"}]),
        ),
    ] {
        let d = fixture();
        assert_eq!(refresh(d.path(), Mode::Full).status, "current");
        field(
            d.path(),
            if key == "status" && v == "resolved" {
                1
            } else {
                2
            },
            key,
            v,
        );
        let rejected = refresh(d.path(), Mode::Incremental);
        assert_eq!(rejected.status, "error");
        assert!(rejected.nodes.is_empty());
        assert!(rejected.version.is_none());
    }
    let d = fixture();
    field(d.path(), 2, "status", json!("resolved"));
    let p = d.path().join("nodes").join(id(2)).join("node.md");
    let text = fs::read_to_string(&p)
        .unwrap()
        .replace("Exact body\r\n", " \n");
    fs::write(p, text).unwrap();
    assert_eq!(refresh(d.path(), Mode::Full).status, "error");
}
#[test]
fn unreleased_barrier_expires_rolls_back_and_next_refresh_recovers() {
    let d = fixture();
    let initial = refresh(d.path(), Mode::Full);
    field(d.path(), 2, "name", json!("Pending replacement"));
    let original = Store::read(d.path()).unwrap().inventory;
    let barrier = tempfile::tempdir().unwrap();
    let pause = PauseBeforeAccept {
        ready: barrier.path().join("ready"),
        release: barrier.path().join("release"),
    };
    let (done, waiting) = std::sync::mpsc::channel();
    let release = pause.release.clone();
    thread::scope(|scope| {
        scope.spawn(move || {
            if waiting.recv_timeout(Duration::from_secs(20)).is_err() {
                fs::write(release, "rescue").unwrap();
            }
        });
        let result = index::refresh(&policy(), d.path(), None, Mode::Full, Some(&pause));
        let _ = done.send(());
        assert_eq!(result.unwrap_err().to_string(), "barrier expired");
    });
    assert!(pause.ready.exists());
    assert!(!pause.release.exists());
    let db = rusqlite::Connection::open(d.path().join(".sifttext/index.sqlite")).unwrap();
    let version: String = db
        .query_row("select value from metadata where key='version'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(Some(version), initial.version);
    let rows: Vec<(String, String)> = db
        .prepare("select id,name from nodes order by id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        rows,
        (1..=3)
            .map(|n| (id(n), format!("Node {n}")))
            .collect::<Vec<_>>()
    );
    drop(db);
    assert_eq!(Store::read(d.path()).unwrap().inventory, original);
    assert_eq!(
        refresh(d.path(), Mode::Incremental).nodes[1]["name"],
        "Pending replacement"
    );
}

#[test]
fn acceptance_race_rolls_back_and_next_refresh_recovers() {
    let d = fixture();
    let barriers = tempfile::tempdir().unwrap();
    let initial = refresh(d.path(), Mode::Full);
    let p = d.path().to_path_buf();
    let ready = barriers.path().join("ready");
    let release = barriers.path().join("release");
    let barrier = PauseBeforeAccept {
        ready: ready.clone(),
        release: release.clone(),
    };
    let worker = thread::spawn(move || {
        index::refresh(&policy(), &p, None, Mode::Incremental, Some(&barrier)).unwrap()
    });
    let start = Instant::now();
    while !ready.exists() {
        assert!(start.elapsed() < Duration::from_secs(10));
        thread::sleep(Duration::from_millis(5));
    }
    field(d.path(), 2, "name", json!("Changed at acceptance"));
    fs::write(release, "release").unwrap();
    assert_eq!(worker.join().unwrap().status, "stale");
    let db = rusqlite::Connection::open(d.path().join(".sifttext/index.sqlite")).unwrap();
    assert_eq!(
        db.query_row("select value from metadata where key='version'", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        initial.version.unwrap()
    );
    drop(db);
    assert_eq!(
        refresh(d.path(), Mode::Incremental).nodes[1]["name"],
        "Changed at acceptance"
    );
}
