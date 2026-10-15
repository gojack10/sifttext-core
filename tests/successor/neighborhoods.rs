use crate::{
    Authored, Policy, Reference, Store,
    neighborhoods::{PauseBeforeAccept, neighborhood},
};
use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

fn id(n: usize) -> String {
    format!("20000000-0000-4000-8000-{n:012}")
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("nodes")).unwrap();
    fs::create_dir(dir.path().join("trees")).unwrap();
    for n in 1..=5 {
        let p = dir.path().join("nodes").join(id(n));
        fs::create_dir_all(p.join("children")).unwrap();
        let node = Authored {
            id: id(n),
            name: format!("Node {n}"),
            micro_summary: format!("Summary {n} Ω"),
            content: "Prose references are not edges.\n".into(),
            see_also: match n {
                2 => vec![Reference {
                    target: id(5),
                    reason: "Forward\nreason Ω".into(),
                }],
                4 => vec![Reference {
                    target: id(2),
                    reason: "Cross-tree reason".into(),
                }],
                _ => vec![],
            },
        };
        let mut text = "---\n".to_string();
        for (k, v) in [
            ("id", serde_json::json!(node.id)),
            ("name", serde_json::json!(node.name)),
            ("micro_summary", serde_json::json!(node.micro_summary)),
            ("see_also", serde_json::json!(node.see_also)),
        ] {
            text.push_str(&format!("{k}: {v}\n"));
        }
        text.push_str(&format!("---\n{}", node.content));
        fs::write(p.join("node.md"), text).unwrap();
        let children = match n {
            1 => vec![3, 2],
            4 => vec![5],
            _ => vec![],
        };
        fs::write(
            p.join("children.order"),
            children.iter().map(|&n| id(n) + "\n").collect::<String>(),
        )
        .unwrap();
        for child in children {
            std::os::unix::fs::symlink(
                format!("../../{}", id(child)),
                p.join("children").join(id(child)),
            )
            .unwrap();
        }
    }
    for n in [1, 4] {
        std::os::unix::fs::symlink(
            format!("../nodes/{}", id(n)),
            dir.path().join("trees").join(id(n)),
        )
        .unwrap();
    }
    dir
}

fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}
fn readable() -> BTreeSet<String> {
    (1..=5).map(id).collect()
}

#[test]
fn checked_projection_preserves_bytes_order_and_directional_reason() {
    let dir = fixture();
    let before = Store::read(dir.path()).unwrap();
    let policy = policy();
    let root = neighborhood(&policy, dir.path(), &id(1), &readable(), None).unwrap();
    assert!(root.neighborhood.parent.is_none());
    assert_eq!(
        root.neighborhood
            .children
            .iter()
            .map(|n| n.id.clone())
            .collect::<Vec<_>>(),
        vec![id(3), id(2)]
    );
    let leaf = neighborhood(&policy, dir.path(), &id(2), &readable(), None).unwrap();
    assert_eq!(leaf.neighborhood.parent.unwrap().id, id(1));
    assert!(leaf.neighborhood.children.is_empty());
    assert_eq!(leaf.neighborhood.outgoing[0].node.id, id(5));
    assert_eq!(leaf.neighborhood.outgoing[0].reason, "Forward\nreason Ω");
    assert_eq!(leaf.neighborhood.incoming[0].node.id, id(4));
    assert_eq!(leaf.neighborhood.incoming[0].reason, "Cross-tree reason");
    assert_eq!(leaf.version, root.version);
    assert_eq!(Store::read(dir.path()).unwrap().inventory, before.inventory);
    let restored = neighborhood(&policy, dir.path(), &id(2), &readable(), None).unwrap();
    assert_eq!(restored.version, root.version);
    for identity in restored.neighborhood.outgoing {
        assert_eq!(
            identity.node.path,
            fs::canonicalize(dir.path())
                .unwrap()
                .join(format!("nodes/{}/node.md", identity.node.id))
        );
    }
}

#[test]
fn moved_and_edited_relationships_are_current_and_permission_filtered() {
    use crate::neighborhoods::{Identity, Neighborhood, Related};

    let dir = fixture();
    let policy = policy();
    let initial = neighborhood(&policy, dir.path(), &id(5), &readable(), None).unwrap();
    for operation in [
        crate::Operation::Move {
            node: id(5),
            parent: id(2),
        },
        crate::Operation::Rename {
            node: id(5),
            name: "Renamed target Ω".into(),
        },
    ] {
        crate::apply(&policy, &Store::read(dir.path()).unwrap(), &operation).unwrap();
    }
    for (n, old, new) in [
        (5, "Summary 5 Ω", "Edited target summary"),
        (2, "Forward\\nreason Ω", "Changed source reason."),
    ] {
        let path = dir.path().join(format!("nodes/{}/node.md", id(n)));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(old));
        fs::write(path, text.replace(old, new)).unwrap();
    }
    let authored = Store::read(dir.path()).unwrap().inventory;
    let identity = |n, name: &str, summary: &str| Identity {
        id: id(n),
        name: name.into(),
        micro_summary: summary.into(),
        path: fs::canonicalize(dir.path())
            .unwrap()
            .join(format!("nodes/{}/node.md", id(n))),
    };
    let parent = || identity(2, "Node 2", "Summary 2 Ω");
    let target = || identity(5, "Renamed target Ω", "Edited target summary");
    let updated = neighborhood(&policy, dir.path(), &id(5), &readable(), None).unwrap();
    assert_ne!(updated.version, initial.version);
    assert_eq!(
        updated.neighborhood,
        Neighborhood {
            parent: Some(parent()),
            children: vec![],
            outgoing: vec![],
            incoming: vec![Related {
                node: parent(),
                reason: "Changed source reason.".into()
            }],
        }
    );
    let source = neighborhood(&policy, dir.path(), &id(2), &readable(), None).unwrap();
    assert_eq!(source.version, updated.version);
    assert_eq!(
        source.neighborhood,
        Neighborhood {
            parent: Some(identity(1, "Node 1", "Summary 1 Ω")),
            children: vec![target()],
            outgoing: vec![Related {
                node: target(),
                reason: "Changed source reason.".into()
            }],
            incoming: vec![Related {
                node: identity(4, "Node 4", "Summary 4 Ω"),
                reason: "Cross-tree reason".into()
            }],
        }
    );
    let without_target = readable().into_iter().filter(|n| n != &id(5)).collect();
    let restricted = neighborhood(&policy, dir.path(), &id(2), &without_target, None).unwrap();
    assert_eq!(restricted.version, updated.version);
    assert_eq!(
        restricted.neighborhood,
        Neighborhood {
            children: vec![],
            outgoing: vec![],
            ..source.neighborhood
        }
    );
    let without_parent = BTreeSet::from([id(5)]);
    let restricted = neighborhood(&policy, dir.path(), &id(5), &without_parent, None).unwrap();
    assert_eq!(
        restricted.neighborhood,
        Neighborhood {
            parent: None,
            children: vec![],
            outgoing: vec![],
            incoming: vec![],
        }
    );
    assert_eq!(Store::read(dir.path()).unwrap().inventory, authored);
}

#[test]
fn bounded_permissions_and_invalid_graph_fail_closed() {
    let dir = fixture();
    let policy = policy();
    let only = BTreeSet::from([id(2)]);
    let value = neighborhood(&policy, dir.path(), &id(2), &only, None).unwrap();
    assert!(value.neighborhood.parent.is_none());
    assert!(value.neighborhood.incoming.is_empty());
    assert!(value.neighborhood.outgoing.is_empty());
    assert!(neighborhood(&policy, dir.path(), &id(5), &only, None).is_err());
    let before = Store::read(dir.path()).unwrap().inventory;
    assert!(
        neighborhood(
            &policy,
            dir.path(),
            &id(2),
            &readable(),
            Some(&PauseBeforeAccept {
                ready: dir.path().join(format!("nodes/{}/node.md", id(2))),
                release: dir.path().join("release"),
            })
        )
        .is_err()
    );
    assert_eq!(Store::read(dir.path()).unwrap().inventory, before);
    // Well-formed metadata with an absent endpoint must pass through graph policy
    // and be rejected, rather than silently skipped by the projection.
    let p = dir.path().join(format!("nodes/{}/node.md", id(2)));
    let original = fs::read_to_string(&p).unwrap();
    fs::write(&p, original.replace(&id(5), &id(6))).unwrap();
    assert!(Store::read(dir.path()).is_ok());
    assert!(neighborhood(&policy, dir.path(), &id(2), &readable(), None).is_err());
    fs::write(&p, original.replace("Summary 2 Ω", "   ")).unwrap();
    assert!(neighborhood(&policy, dir.path(), &id(2), &readable(), None).is_err());
    assert!(Store::read(dir.path()).is_err());
}

#[test]
fn unreleased_barrier_expires_without_returning_a_current_neighborhood() {
    let dir = fixture();
    let original = Store::read(dir.path()).unwrap().inventory;
    let expected = neighborhood(&policy(), dir.path(), &id(2), &readable(), None).unwrap();
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
        let result = neighborhood(&policy(), dir.path(), &id(2), &readable(), Some(&pause));
        let _ = done.send(());
        assert_eq!(
            result.unwrap_err().to_string(),
            "inspection barrier expired"
        );
    });
    assert!(pause.ready.exists());
    assert!(!pause.release.exists());
    assert_eq!(Store::read(dir.path()).unwrap().inventory, original);
    let current = neighborhood(&policy(), dir.path(), &id(2), &readable(), None).unwrap();
    assert_eq!(current.neighborhood, expected.neighborhood);
    assert_eq!(current.version, expected.version);
}

#[test]
fn source_change_at_acceptance_barrier_is_rejected_without_overwrite() {
    let dir = fixture();
    let barrier = tempfile::tempdir().unwrap();
    let ready = barrier.path().join("ready");
    let release = barrier.path().join("release");
    let source = dir.path().join(format!("nodes/{}/node.md", id(5)));
    let edited = fs::read_to_string(&source)
        .unwrap()
        .replace("Node 5", "Edited target");
    let expected = edited.clone();
    let ready_worker = ready.clone();
    let release_worker = release.clone();
    let handle = thread::spawn(move || {
        let start = Instant::now();
        while !ready_worker.exists() {
            assert!(start.elapsed() < Duration::from_secs(10));
            thread::sleep(Duration::from_millis(5));
        }
        fs::write(source, edited).unwrap();
        fs::write(release_worker, b"release").unwrap();
    });
    assert!(
        neighborhood(
            &policy(),
            dir.path(),
            &id(2),
            &readable(),
            Some(&PauseBeforeAccept { ready, release })
        )
        .is_err()
    );
    handle.join().unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join(format!("nodes/{}/node.md", id(5)))).unwrap(),
        expected
    );
}
