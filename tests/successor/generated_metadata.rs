use crate::{
    Entry, Operation, Policy, Store, apply, candidate_validation as gate, sha256,
    token_metadata::{Measurement, PauseBeforeAccept, Refresher},
    validate_draft,
};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

const ID: &str = "70000000-0000-4000-8000-000000000001";
fn dir(root: &Path) -> PathBuf {
    root.join("nodes").join(ID)
}
fn source(root: &Path) -> PathBuf {
    dir(root).join("node.md")
}
fn sidecar(root: &Path) -> PathBuf {
    dir(root).join("node.meta.json")
}
fn text(name: &str) -> String {
    format!(
        "---\nid: \"{ID}\"\nname: \"{name}\"\nmicro_summary: \"Synthetic summary\"\nsee_also: []\nstatus: \"in_progress\"\n---\nSynthetic Ω body.\n"
    )
}
fn fixture(root: &Path) {
    fs::create_dir_all(dir(root).join("children")).unwrap();
    fs::create_dir_all(root.join("trees")).unwrap();
    fs::write(source(root), text("Node")).unwrap();
    fs::write(dir(root).join("children.order"), "").unwrap();
    std::os::unix::fs::symlink(format!("../nodes/{ID}"), root.join("trees").join(ID)).unwrap();
    fs::write(root.join("protected.bin"), b"protected original").unwrap();
}
fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}
fn noop() -> Operation {
    Operation::Rename {
        node: ID.into(),
        name: "Node".into(),
    }
}
fn request(root: &Path, before: Option<&Path>) -> gate::Request {
    gate::Request {
        store: root.into(),
        before: before.map(Path::to_path_buf),
        explicit: BTreeSet::new(),
        expected_version: None,
        pause_before_accept: None,
    }
}
fn git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn ready(path: &Path) {
    let start = Instant::now();
    while !path.exists() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "barrier not reached"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn canonical_refresh_preserves_exact_inventory_draft_apply_and_candidate_version() {
    let t = tempfile::tempdir().unwrap();
    let b = t.path().join("before");
    let a = t.path().join("after");
    fixture(&b);
    fixture(&a);
    let before = Store::read(&b).unwrap();
    let bytes = fs::read(source(&a)).unwrap();
    let measured = Refresher::default().refresh(&source(&a), None).unwrap();
    assert_eq!(measured.status, "current");
    assert_eq!(measured.measurement.hash, sha256(&bytes));
    assert!(dir(&a).join("node.meta.lock").is_file());
    let after = Store::read(&a).unwrap();
    assert_eq!(before.inventory, after.inventory);
    assert!(
        !after
            .inventory
            .contains_key(&PathBuf::from(format!("nodes/{ID}/node.meta.json")))
    );
    validate_draft(&policy(), &before, &after, &noop()).unwrap();
    let r = gate::validate(&policy(), &request(&a, Some(&b)));
    assert_eq!(r.status, "current", "{}", r.lean().unwrap());
    assert_eq!(r.version, r.before_version);
    // Refresh on the same path after capture must not invalidate apply's recheck.
    let captured = Store::read(&a).unwrap();
    fs::remove_file(sidecar(&a)).unwrap();
    Refresher::default().refresh(&source(&a), None).unwrap();
    apply(&policy(), &captured, &noop()).unwrap();
    assert_eq!(fs::read(source(&a)).unwrap(), bytes);
    println!(
        "canonical refresh: current; exact authored hash={}; draft/apply/noop candidate pass",
        measured.measurement.hash
    );
}

#[test]
fn restore_revert_cache_loss_and_authored_metadata_edits_keep_exact_source_binding() {
    let t = tempfile::tempdir().unwrap();
    fixture(t.path());
    let root = t.path();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.name", "Synthetic Fixture"]);
    git(root, &["config", "user.email", "fixture@example.invalid"]);
    git(root, &["add", "nodes", "trees", "protected.bin"]);
    git(root, &["commit", "-qm", "fixture: initial"]);
    let original = fs::read(source(root)).unwrap();
    let original_store = Store::read(root).unwrap();
    let first = Refresher::default().refresh(&source(root), None).unwrap();
    let original_sidecar = fs::read(sidecar(root)).unwrap();
    let mut req = request(root, None);
    req.expected_version = Some(gate::version(&original_store));
    // A frontmatter-only edit is authored, not disposable token metadata.
    fs::write(source(root), text("Changed")).unwrap();
    assert_eq!(gate::validate(&policy(), &req).status, "rejected");
    let changed = Refresher::default().refresh(&source(root), None).unwrap();
    assert_ne!(changed.measurement.hash, first.measurement.hash);
    git(root, &["restore", "nodes"]);
    assert_eq!(gate::validate(&policy(), &req).status, "current");
    assert_eq!(
        Refresher::default()
            .refresh(&source(root), None)
            .unwrap()
            .measurement,
        first.measurement
    );
    assert_eq!(fs::read(sidecar(root)).unwrap(), original_sidecar);
    fs::write(source(root), text("Changed")).unwrap();
    git(root, &["add", &format!("nodes/{ID}/node.md")]);
    git(root, &["commit", "-qm", "fixture: authored edit"]);
    Refresher::default().refresh(&source(root), None).unwrap();
    git(root, &["revert", "--no-edit", "HEAD"]);
    // Stale disk counts are not consumed by validation. A fresh consumer proves
    // current bytes and repairs the projection, never trusting the disk claim.
    assert_eq!(gate::validate(&policy(), &req).status, "current");
    assert_eq!(
        Refresher::default()
            .refresh(&source(root), None)
            .unwrap()
            .measurement,
        first.measurement
    );
    fs::remove_file(sidecar(root)).unwrap();
    assert_eq!(
        Store::read(root).unwrap().inventory,
        original_store.inventory
    );
    let rebuilt = Refresher::default().refresh(&source(root), None).unwrap();
    assert!(!rebuilt.reused);
    assert_eq!(rebuilt.measurement, first.measurement);
    assert_eq!(fs::read(source(root)).unwrap(), original);
}

#[test]
fn forged_hash_count_and_version_are_ignored_then_explicitly_regenerated() {
    let t = tempfile::tempdir().unwrap();
    fixture(t.path());
    let root = t.path();
    let authored = fs::read(source(root)).unwrap();
    let expected = Refresher::default()
        .refresh(&source(root), None)
        .unwrap()
        .measurement;
    let before = Store::read(root).unwrap();
    for (field, value) in [
        ("hash", serde_json::json!("forged")),
        ("tokens", serde_json::json!(0)),
        ("tokenizer_version", serde_json::json!("foreign/v0")),
        ("encoding", serde_json::json!("foreign")),
    ] {
        let mut forged = serde_json::to_value(&expected).unwrap();
        forged[field] = value;
        fs::write(sidecar(root), serde_json::to_vec(&forged).unwrap()).unwrap();
        let after = Store::read(root).unwrap();
        if field == "tokens" {
            // Canonical schema still cannot certify the honesty of a disk count.
            assert_eq!(after.inventory, before.inventory);
            validate_draft(&policy(), &before, &after, &noop()).unwrap();
        } else {
            // Foreign schema/identity is protected until explicit regeneration.
            assert!(validate_draft(&policy(), &before, &after, &noop()).is_err());
        }
        let proof = Refresher::default().refresh(&source(root), None).unwrap();
        assert_eq!(proof.status, "current");
        assert!(!proof.reused);
        assert_eq!(proof.measurement, expected);
        assert_eq!(
            serde_json::from_slice::<Measurement>(&fs::read(sidecar(root)).unwrap()).unwrap(),
            expected
        );
        assert_eq!(fs::read(source(root)).unwrap(), authored);
    }
    fs::write(sidecar(root), b"malformed and stale").unwrap();
    assert_eq!(
        Refresher::default()
            .refresh(&source(root), None)
            .unwrap()
            .measurement,
        expected
    );
    println!(
        "forged hash/count/tokenizer/encoding and malformed cache: regenerated from complete source {}",
        expected.hash
    );
}

#[test]
fn basename_is_not_a_bypass_for_unknown_locations_or_real_protected_edits() {
    let t = tempfile::tempdir().unwrap();
    fixture(t.path());
    let root = t.path();
    let before = Store::read(root).unwrap();
    for path in [
        root.join("node.meta.json"),
        dir(root).join("nested/node.meta.json"),
        dir(root).join("node.meta.other.json"),
        root.join("node.meta.lock"),
    ] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"protected bytes").unwrap();
        let after = Store::read(root).unwrap();
        assert!(
            after
                .inventory
                .values()
                .any(|e| matches!(e, Entry::File(b) if b == b"protected bytes"))
        );
        assert!(validate_draft(&policy(), &before, &after, &noop()).is_err());
        fs::remove_file(&path).unwrap();
        if path.parent() == Some(dir(root).join("nested").as_path()) {
            fs::remove_dir(path.parent().unwrap()).unwrap();
        }
    }
    fs::write(root.join("protected.bin"), b"real edit").unwrap();
    assert!(validate_draft(&policy(), &before, &Store::read(root).unwrap(), &noop()).is_err());
    fs::write(source(root), text("Changed")).unwrap();
    assert!(validate_draft(&policy(), &before, &Store::read(root).unwrap(), &noop()).is_err());
    assert!(!crate::token_metadata::derived(Path::new(
        "nodes/not-a-uuid/node.meta.json"
    )));
    assert!(!crate::token_metadata::derived(Path::new(
        "../nodes/70000000-0000-4000-8000-000000000001/node.meta.json"
    )));
}

#[test]
fn symlinks_hardlinks_directories_lock_payload_and_source_aliases_reject() {
    use std::os::unix::fs::symlink;
    let t = tempfile::tempdir().unwrap();
    fixture(t.path());
    let root = t.path();
    let foreign = t.path().join("foreign");
    fs::write(&foreign, b"do not touch").unwrap();
    for file in [sidecar(root), dir(root).join("node.meta.lock")] {
        // A failed sidecar refresh may already have made the safe persistent lock.
        let lock = dir(root).join("node.meta.lock");
        if lock.exists() {
            fs::remove_file(&lock).unwrap();
        }
        symlink(&foreign, &file).unwrap();
        assert!(Store::read(root).is_err());
        assert_eq!(
            gate::validate(&policy(), &request(root, None)).status,
            "rejected"
        );
        assert!(Refresher::default().refresh(&source(root), None).is_err());
        fs::remove_file(&file).unwrap();
        fs::hard_link(&foreign, &file).unwrap();
        assert!(Store::read(root).is_err());
        assert!(Refresher::default().refresh(&source(root), None).is_err());
        fs::remove_file(&file).unwrap();
        fs::create_dir(&file).unwrap();
        assert!(Store::read(root).is_err());
        assert!(Refresher::default().refresh(&source(root), None).is_err());
        fs::remove_dir(&file).unwrap();
    }
    fs::write(dir(root).join("node.meta.lock"), b"protected payload").unwrap();
    assert!(Store::read(root).is_err());
    assert!(Refresher::default().refresh(&source(root), None).is_err());
    fs::remove_file(dir(root).join("node.meta.lock")).unwrap();
    fs::hard_link(source(root), sidecar(root)).unwrap();
    assert!(Store::read(root).is_err());
    assert!(Refresher::default().refresh(&source(root), None).is_err());
    fs::remove_file(sidecar(root)).unwrap();
    assert_eq!(fs::read(&foreign).unwrap(), b"do not touch");
    assert_eq!(fs::read(source(root)).unwrap(), text("Node").as_bytes());
}

#[test]
fn safe_refresh_at_candidate_barrier_passes_but_protected_and_authored_races_do_not() {
    for action in ["refresh", "protected", "authored", "unsafe"] {
        let t = tempfile::tempdir().unwrap();
        let b = t.path().join("before");
        let a = t.path().join("after");
        fixture(&b);
        fixture(&a);
        let bytes = fs::read(source(&a)).unwrap();
        let r = t.path().join("ready");
        let release = t.path().join("release");
        let mut q = request(&a, Some(&b));
        q.pause_before_accept = Some(gate::PauseBeforeAccept {
            ready: r.clone(),
            release: release.clone(),
        });
        let h = thread::spawn(move || gate::validate(&policy(), &q));
        ready(&r);
        match action {
            "refresh" => {
                Refresher::default().refresh(&source(&a), None).unwrap();
                Refresher::default().refresh(&source(&b), None).unwrap();
            }
            "protected" => fs::write(a.join("protected.bin"), "raced protected edit").unwrap(),
            "authored" => fs::write(source(&a), text("Changed")).unwrap(),
            "unsafe" => std::os::unix::fs::symlink(a.join("protected.bin"), sidecar(&a)).unwrap(),
            _ => unreachable!(),
        }
        fs::write(&release, b"release").unwrap();
        let report = h.join().unwrap();
        assert_eq!(
            report.status,
            if action == "refresh" {
                "current"
            } else {
                "stale"
            },
            "{action}"
        );
        if action != "authored" {
            assert_eq!(fs::read(source(&a)).unwrap(), bytes);
        }
        println!("candidate barrier {action}: {}", report.status);
    }
}

#[test]
fn metadata_refresh_source_replacement_race_stays_stale_without_publication() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("store");
    fixture(&root);
    let captured = Store::read(&root).unwrap();
    let bytes = fs::read(source(&root)).unwrap();
    let r = t.path().join("ready");
    let release = t.path().join("release");
    let path = source(&root);
    let pause = PauseBeforeAccept {
        ready: r.clone(),
        release: release.clone(),
    };
    let h = thread::spawn(move || Refresher::default().refresh(&path, Some(&pause)).unwrap());
    ready(&r);
    let replacement = dir(&root).join("replacement");
    fs::write(&replacement, &bytes).unwrap();
    fs::rename(&replacement, source(&root)).unwrap();
    fs::write(release, b"release").unwrap();
    assert_eq!(h.join().unwrap().status, "stale");
    assert!(!sidecar(&root).exists());
    assert_eq!(Store::read(&root).unwrap().inventory, captured.inventory);
    assert_eq!(fs::read(source(&root)).unwrap(), bytes);
    let current = Refresher::default().refresh(&source(&root), None).unwrap();
    assert_eq!(current.status, "current");
    assert_eq!(current.measurement.hash, sha256(&bytes));
}
