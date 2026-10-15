use crate::{
    Entry, Store,
    coherent_capture::{self, Capture, Relationships},
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
fn id(n: usize) -> String {
    format!("61000000-0000-4000-8000-{n:012}")
}
fn fixture(count: usize, deep: bool) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir(d.path().join("trees")).unwrap();
    for n in 1..=count {
        let p = d.path().join("nodes").join(id(n));
        fs::create_dir_all(p.join("children")).unwrap();
        fs::write(p.join("children/.keep"), "").unwrap();
        fs::write(p.join("node.md"),format!("---\nid: {:?}\nname: {:?}\nmicro_summary: \"Source summary Ω\"\nstatus: \"in_progress\"\nsee_also: []\ncustom: [1,\"opaque\"]\n---\nBody\r\n",id(n),format!("Node {n}"))).unwrap();
        let cs = if deep {
            if n < count { vec![n + 1] } else { vec![] }
        } else if n == 1 {
            (2..=count).rev().collect()
        } else {
            vec![]
        };
        fs::write(
            p.join("children.order"),
            cs.iter().map(|n| id(*n) + "\n").collect::<String>(),
        )
        .unwrap();
        for c in cs {
            std::os::unix::fs::symlink(format!("../../{}", id(c)), p.join("children").join(id(c)))
                .unwrap();
        }
    }
    std::os::unix::fs::symlink(
        format!("../nodes/{}", id(1)),
        d.path().join("trees").join(id(1)),
    )
    .unwrap();
    d
}

#[test]
fn single_inventory_parse_and_one_pass_relationship_work() {
    for deep in [false, true] {
        let d = fixture(128, deep);
        coherent_capture::take_work();
        let s = Store::read(d.path()).unwrap();
        let work = coherent_capture::take_work();
        assert_eq!(work.inventories, 1);
        assert_eq!(work.parses, 128);
        assert_eq!(work.relationship_entries, s.inventory.len());
        // Execute the former consumer binding algorithm, counting its real work.
        let mut legacy_visits = 0;
        for (id, n) in &s.nodes {
            let Entry::File(b) = &s.inventory[&PathBuf::from(format!("nodes/{id}/node.md"))] else {
                panic!()
            };
            assert_eq!(
                crate::parse_markdown(std::str::from_utf8(b).unwrap()).unwrap(),
                *n
            );
            let dir = PathBuf::from(format!("nodes/{id}/children"));
            let members: Vec<_> = s
                .inventory
                .iter()
                .filter(|(p, e)| {
                    legacy_visits += 1;
                    p.parent() == Some(dir.as_path()) && matches!(e, Entry::Link(_))
                })
                .collect();
            assert_eq!(members.len(), s.children[id].len());
        }
        let legacy = coherent_capture::take_work();
        assert_eq!(legacy.parses, 128); // these were extra reparses after Store::read
        assert_eq!(legacy_visits, 128 * s.inventory.len());
        let rel = Relationships::new(&s);
        assert_eq!(rel.parents.len(), 127);
        assert_eq!(
            rel.parents[id(128).as_str()],
            if deep { id(127) } else { id(1) }
        );
        println!(
            "coherent work shape={} nodes=128 inventory_entries={} parses={} binding_visits={} legacy_extra_parses={} legacy_binding_visits={}",
            if deep { "deep" } else { "wide" },
            s.inventory.len(),
            work.parses,
            work.relationship_entries,
            legacy.parses,
            legacy_visits
        );
    }
}

#[test]
fn real_consumers_reuse_metadata_without_extra_markdown_parses() {
    let policy = crate::Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let before = fixture(8, false);
    let after = fixture(8, false);
    let readable = (1..=8).map(id).collect();
    for operation in ["index", "candidate", "neighborhood", "status"] {
        coherent_capture::take_work();
        match operation {
            "index" => assert_eq!(
                crate::index::refresh(
                    &policy,
                    before.path(),
                    None,
                    crate::index::Mode::Incremental,
                    None
                )
                .unwrap()
                .status,
                "current"
            ),
            "candidate" => {
                let request = crate::candidate_validation::Request {
                    store: before.path().to_owned(),
                    before: None,
                    explicit: Default::default(),
                    expected_version: None,
                    pause_before_accept: None,
                };
                assert_eq!(
                    crate::candidate_validation::validate(&policy, &request).status,
                    "current"
                );
            }
            "neighborhood" => {
                crate::neighborhoods::neighborhood(&policy, before.path(), &id(2), &readable, None)
                    .unwrap();
            }
            "status" => {
                assert!(
                    crate::status::normalize(
                        &policy,
                        before.path(),
                        after.path(),
                        &Default::default(),
                        None
                    )
                    .unwrap()
                    .normalized_ids
                    .is_empty()
                );
            }
            _ => unreachable!(),
        }
        let work = coherent_capture::take_work();
        assert_eq!(
            work.parses,
            // Status captures before, candidate and the accepted post-write view.
            if operation == "status" { 24 } else { 8 },
            "consumer {operation} must parse only its initial captures"
        );
        println!("consumer={operation} work={work:?}");
    }
}

#[test]
fn exact_inventory_is_the_only_parser_input_and_skew_rejects() {
    for kind in ["body", "order", "membership", "root", "same_bytes"] {
        let d = fixture(3, false);
        let p = d.path().to_owned();
        coherent_capture::on_inventory(move || {
            let node = p.join("nodes").join(id(1));
            match kind {
                "body" => {
                    let file = node.join("node.md");
                    let text = fs::read_to_string(&file).unwrap();
                    fs::write(file, text + "New body").unwrap();
                }
                "order" => {
                    fs::write(node.join("children.order"), id(2) + "\n" + &id(3) + "\n").unwrap()
                }
                "membership" => fs::remove_file(node.join("children").join(id(2))).unwrap(),
                "root" => fs::remove_file(p.join("trees").join(id(1))).unwrap(),
                "same_bytes" => {
                    let file = node.join("node.md");
                    fs::write(node.join("new"), fs::read(&file).unwrap()).unwrap();
                    fs::rename(node.join("new"), file).unwrap();
                }
                _ => unreachable!(),
            }
        });
        assert!(Capture::read(d.path(), None).is_err(), "skew {kind}");
    }
}

#[test]
fn freshness_identity_and_distinct_asset_protections() {
    let d = fixture(3, false);
    fs::write(d.path().join("asset"), "old").unwrap();
    let captured = Capture::read(d.path(), None).unwrap();
    fs::write(d.path().join("asset"), "new").unwrap();
    assert!(captured.unchanged().is_ok()); // index logical boundary
    assert!(captured.unchanged_all().is_err()); // candidate/status/neighborhood full inventory
    let file = d.path().join("nodes").join(id(2)).join("node.md");
    fs::write(file.with_extension("new"), fs::read(&file).unwrap()).unwrap();
    fs::rename(file.with_extension("new"), file).unwrap();
    assert!(captured.unchanged().is_err());
}

#[test]
fn sparse_inheritance_is_complete_and_base_identity_is_checked() {
    let base = fixture(3, false);
    let overlay = tempfile::tempdir().unwrap();
    fs::create_dir(overlay.path().join("nodes")).unwrap();
    fs::create_dir(overlay.path().join("trees")).unwrap();
    std::os::unix::fs::symlink(
        format!("../nodes/{}", id(1)),
        overlay.path().join("trees").join(id(1)),
    )
    .unwrap();
    let captured = Capture::read(overlay.path(), Some(base.path())).unwrap();
    assert_eq!(captured.store.nodes.len(), 3);
    assert_eq!(captured.store.children[&id(1)], vec![id(3), id(2)]);
    assert!(!overlay.path().join("nodes").join(id(2)).exists());
    let file = base.path().join("nodes").join(id(2)).join("children.order");
    fs::write(file.with_extension("new"), fs::read(&file).unwrap()).unwrap();
    fs::rename(file.with_extension("new"), file).unwrap();
    assert!(captured.unchanged().is_err());
}

#[test]
fn sparse_override_does_not_hide_base_freshness_changes() {
    for target in ["base", "overlay"] {
        let base = fixture(3, false);
        let overlay = fixture(3, false);
        let file = |root: &Path| root.join("nodes").join(id(2)).join("node.md");
        let override_file = file(overlay.path());
        fs::write(
            &override_file,
            fs::read_to_string(&override_file).unwrap() + "Overlay-only body\n",
        )
        .unwrap();
        let capture = Capture::read(overlay.path(), Some(base.path())).unwrap();
        assert!(
            capture.store.nodes[&id(2)]
                .content
                .contains("Overlay-only body")
        );
        capture.unchanged().unwrap();
        capture.unchanged_all().unwrap();
        let changed = file(if target == "base" {
            base.path()
        } else {
            overlay.path()
        });
        let original = fs::read(&changed).unwrap();
        fs::write(
            &changed,
            [original.as_slice(), b"Changed source\n"].concat(),
        )
        .unwrap();
        assert!(capture.unchanged().is_err(), "{target} authored change");
        assert!(capture.unchanged_all().is_err(), "{target} authored change");
        // Restoring bytes does not undo the intervening physical mutation.
        fs::write(&changed, original).unwrap();
        assert!(capture.unchanged().is_err(), "{target} restored-byte ABA");
        assert!(
            capture.unchanged_all().is_err(),
            "{target} restored-byte ABA"
        );
    }
}

#[test]
fn same_byte_asset_hardlink_is_not_a_safe_freshness_recapture() {
    let d = fixture(3, false);
    let asset = d.path().join("asset");
    fs::write(&asset, "unchanged bytes").unwrap();
    let capture = Capture::read(d.path(), None).unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::hard_link(&asset, outside.path().join("alias")).unwrap();
    // Byte equality alone cannot replace inventory's trust-boundary checks.
    assert!(capture.unchanged().is_err());
    assert!(capture.unchanged_all().is_err());
    assert_eq!(fs::read(asset).unwrap(), b"unchanged bytes");
}

#[test]
fn captured_invalid_orders_links_and_markers_are_not_accepted() {
    let d = fixture(3, false);
    let s = Store::read(d.path()).unwrap();
    let dirs: BTreeMap<_, _> = s
        .nodes
        .keys()
        .map(|id| (id.clone(), s.path.join("nodes").join(id)))
        .collect();
    for (p, e) in [
        (
            format!("nodes/{}/children.order", id(1)),
            Entry::File((id(2) + "\n").into_bytes()),
        ),
        (
            format!("nodes/{}/children/{}", id(1), id(2)),
            Entry::Link(PathBuf::from("/unsafe")),
        ),
        ("trees/.keep".to_owned(), Entry::File(vec![1])),
        (
            format!("nodes/{}/node.md", id(2)),
            Entry::File(b"not markdown".to_vec()),
        ),
    ] {
        let mut files = s.inventory.clone();
        files.insert(PathBuf::from(p), e);
        assert!(
            coherent_capture::from_inventory(s.path.clone(), files, dirs.clone(), false).is_err()
        );
    }
    // Accepted cap removal must survive shared capture; this is parsing, not
    // 1698-node Bend/resource qualification (owned by the capacity task).
    let large = fixture(1698, false);
    coherent_capture::take_work();
    let captured = Store::read(large.path()).unwrap();
    assert_eq!(captured.nodes.len(), 1698);
    let work = coherent_capture::take_work();
    assert_eq!(work.inventories, 1);
    assert_eq!(work.parses, 1698);
    assert_eq!(work.relationship_entries, captured.inventory.len());
    println!(
        "1698-node coherent parsing: {work:?}; all identities retained, no Bend qualification"
    );
}

#[test]
fn accepted_unbounded_reference_capture_and_exact_order_diagnostic_share_one_pass() {
    let d = fixture(300, false);
    let file = d.path().join("nodes").join(id(1)).join("node.md");
    let references: Vec<_> = (2..=300)
        .map(|n| serde_json::json!({"target": id(n), "reason": "Synthetic reference"}))
        .collect();
    fs::write(
        &file,
        fs::read_to_string(&file).unwrap().replace(
            "see_also: []",
            &format!("see_also: {}", serde_json::json!(references)),
        ),
    )
    .unwrap();
    coherent_capture::take_work();
    let store = Store::read(d.path()).unwrap();
    assert_eq!(store.nodes[&id(1)].see_also.len(), 299);
    let work = coherent_capture::take_work();
    assert_eq!(work.parses, 300);
    assert_eq!(work.inventories, 1);
    assert_eq!(work.relationship_entries, store.inventory.len());

    let policy = crate::Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    fs::write(
        d.path().join("nodes").join(id(1)).join("children.order"),
        id(2) + "\n" + &id(2) + "\n",
    )
    .unwrap();
    coherent_capture::take_work();
    let report = crate::candidate_validation::validate(
        &policy,
        &crate::candidate_validation::Request {
            store: d.path().into(),
            before: None,
            explicit: Default::default(),
            expected_version: None,
            pause_before_accept: None,
        },
    );
    assert_eq!(report.status, "rejected");
    assert_eq!(report.rows.len(), 1);
    assert_eq!(report.rows[0].source, Some(id(1)));
    assert_eq!(report.rows[0].target, Some(id(2)));
    assert_eq!(
        report.rows[0].law,
        Some("src/lib.rs:Store.read_view/order_membership")
    );
    let work = coherent_capture::take_work();
    assert_eq!(work.inventories, 1);
    assert_eq!(work.parses, 1); // reject at the actual parse/order boundary, no binding reparse
}

#[test]
#[ignore = "explicit paired freshness timing, not an acceptance gate"]
fn freshness_recapture_timing() {
    fn cpu() -> f64 {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
        assert_eq!(unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) }, 0);
        let u = unsafe { usage.assume_init() };
        u.ru_utime.tv_sec as f64 + u.ru_utime.tv_usec as f64 / 1e6
            + u.ru_stime.tv_sec as f64 + u.ru_stime.tv_usec as f64 / 1e6
    }
    for (count, deep) in [(10000, false), (1000, true)] {
        let d = fixture(count, deep);
        let captured = Capture::read(d.path(), None).unwrap();
        coherent_capture::take_work();
        let start = std::time::Instant::now();
        let before = cpu();
        for _ in 0..3 {
            captured.unchanged_all().unwrap();
        }
        let cpu = cpu() - before;
        let wall = start.elapsed().as_secs_f64();
        let work = coherent_capture::take_work();
        println!("freshness nodes={count} deep={deep} repeats=3 wall={wall:.6} cpu={cpu:.6} concurrency={:.4} work={work:?}", cpu / wall);
        assert_eq!(work.inventories, 3);
    }
}
