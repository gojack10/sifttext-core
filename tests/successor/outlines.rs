use crate::{
    Policy, Store,
    index::{self, Mode, PauseBeforeAccept},
    outlines,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};
fn id(n: usize) -> String {
    format!("80000000-0000-4000-8000-{n:012}")
}
fn fixture() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir(d.path().join("trees")).unwrap();
    for n in 1..=5 {
        let p = d.path().join("nodes").join(id(n));
        fs::create_dir_all(p.join("children")).unwrap();
        let children = match n {
            1 => vec![2, 3],
            2 => vec![4],
            _ => vec![],
        };
        let fields = json!({"id":id(n),"name":format!("Name {n} Ω\t\n"),"micro_summary":"Keep exact summary Ω\nline.",
            "status":"in_progress","warnings":"Quotes \" and slash \\ and control \u{0001}","see_also":[]});
        fs::write(
            p.join("node.md"),
            "---\n".to_owned()
                + &fields
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}\n"))
                    .collect::<String>()
                + "---\nBody never copied.\n",
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
fn refresh(path: &Path, base: Option<&Path>) -> index::View {
    index::refresh(&policy(), path, base, Mode::Incremental, None).unwrap()
}
fn inspect(path: &Path, base: Option<&Path>) -> outlines::Inspection {
    outlines::inspect(&policy(), path, base).unwrap()
}
fn field(p: &Path, n: usize, key: &str, v: Value) {
    let path = p.join("nodes").join(id(n)).join("node.md");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(
        path,
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
fn paused(path: &Path, change: impl FnOnce(&Path)) -> index::View {
    let barriers = tempfile::tempdir().unwrap();
    let ready = barriers.path().join("ready");
    let release = barriers.path().join("release");
    let pause = PauseBeforeAccept {
        ready: ready.clone(),
        release: release.clone(),
    };
    let p = path.to_path_buf();
    let t = thread::spawn(move || {
        index::refresh(&policy(), &p, None, Mode::Full, Some(&pause)).unwrap()
    });
    let start = Instant::now();
    while !ready.exists() {
        assert!(start.elapsed() < Duration::from_secs(10));
        thread::sleep(Duration::from_millis(5));
    }
    change(path);
    fs::write(release, "go").unwrap();
    t.join().unwrap()
}
#[test]
fn refreshed_outline_reuses_checked_publication_bytes_and_capture() {
    let d = fixture();
    let p = d.path();
    let policy = policy();
    refresh(p, None);
    crate::coherent_capture::take_work();
    refresh(p, None);
    let baseline = crate::coherent_capture::take_work();
    for n in [1, 2, 5] {
        for full in [false, true] {
            crate::coherent_capture::take_work();
            let text = index::outline(&policy, p, &id(n), full).unwrap();
            let work = crate::coherent_capture::take_work();
            assert_eq!(work.inventories, baseline.inventories);
            assert_eq!(work.parses, baseline.parses);
            let file = p.join("nodes").join(id(n)).join(if full {
                "outline.full.lean"
            } else {
                "outline.lean"
            });
            let inspected = inspect(p, None);
            assert_eq!(inspected.status, "current");
            let checked = inspected
                .files
                .iter()
                .find(|f| f["path"] == file.to_str().unwrap())
                .unwrap();
            assert_eq!(text, checked["text"].as_str().unwrap());
        }
    }
    assert!(index::outline(&policy, p, "80000000", false).is_err());
    assert!(index::outline(&policy, p, &id(99), false).is_err());
    let artifact = p.join("nodes").join(id(2)).join("outline.lean");
    fs::write(&artifact, "forged").unwrap();
    let repaired = index::outline(&policy, p, &id(2), false).unwrap();
    assert_eq!(repaired, fs::read_to_string(artifact).unwrap());
    let target = p.join("nodes").join(id(2)).join("node.md");
    crate::coherent_capture::on_inventory(move || {
        fs::write(target, "changed during capture").unwrap();
    });
    assert!(index::outline(&policy, p, &id(2), false).is_err());
}

#[test]
fn complete_publication_costs_tamper_loss_and_stale_inspection() {
    let d = fixture();
    let p = d.path();
    let authored = Store::read(p).unwrap().inventory;
    assert_eq!(inspect(p, None).status, "pending");
    let v = refresh(p, None);
    let got = inspect(p, None);
    assert_eq!(got.status, "current");
    assert_eq!(got.version, v.version);
    assert_eq!(got.files.len(), 20);
    assert_eq!(
        Store::read(p)
            .unwrap()
            .inventory
            .iter()
            .filter(|(k, _)| authored.contains_key(*k))
            .count(),
        authored.len()
    );
    for (k, b) in &authored {
        assert_eq!(Store::read(p).unwrap().inventory.get(k), Some(b));
    }
    let file = p.join("nodes").join(id(1)).join("outline.full.lean");
    let original = fs::read(&file).unwrap();
    let meta: Value = serde_json::from_slice(
        &fs::read(file.with_file_name("outline.full.lean.meta.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(meta["hash"], crate::sha256(&original));
    fs::write(&file, "forged").unwrap();
    assert_eq!(inspect(p, None).status, "stale");
    refresh(p, None);
    assert_eq!(fs::read(&file).unwrap(), original);
    fs::remove_file(&file).unwrap();
    assert_eq!(inspect(p, None).status, "stale");
    refresh(p, None);
    let db = rusqlite::Connection::open(p.join(".sifttext/index.sqlite")).unwrap();
    db.execute_batch("UPDATE nodes SET name='forged column'")
        .unwrap();
    drop(db);
    assert_eq!(inspect(p, None).status, "stale");
    refresh(p, None);
    let db = rusqlite::Connection::open(p.join(".sifttext/index.sqlite")).unwrap();
    db.execute_batch("UPDATE metadata SET value='{}' WHERE key='outlines'")
        .unwrap();
    drop(db);
    assert_eq!(inspect(p, None).status, "stale");
    refresh(p, None);
    field(p, 4, "name", json!("Changed without refresh"));
    assert_eq!(inspect(p, None).status, "stale");
    let updated = refresh(p, None);
    assert_ne!(updated.version, v.version);
    assert_eq!(inspect(p, None).version, updated.version);
    field(p, 4, "micro_summary", json!(" \t"));
    assert_eq!(inspect(p, None).status, "error");
    assert_eq!(refresh(p, None).status, "error");
}
#[test]
fn sparse_artifacts_never_write_base_and_hydration_preserves_version() {
    let base = fixture();
    let overlay = fixture();
    let before = Store::read(base.path()).unwrap().inventory;
    assert_eq!(refresh(base.path(), Some(base.path())).status, "error");
    assert!(!base.path().join(".sifttext").exists());
    fs::remove_dir_all(overlay.path().join("nodes").join(id(4))).unwrap();
    let sparse = refresh(overlay.path(), Some(base.path()));
    assert_eq!(sparse.status, "current");
    assert_eq!(Store::read(base.path()).unwrap().inventory, before);
    assert!(!base.path().join(".sifttext").exists());
    assert!(!overlay.path().join("nodes").join(id(4)).exists());
    assert!(
        overlay
            .path()
            .join(".sifttext/outlines")
            .join(id(4))
            .join("outline.full.lean")
            .exists()
    );
    assert_eq!(inspect(overlay.path(), Some(base.path())).status, "current");
    assert!(
        std::process::Command::new("cp")
            .arg("-R")
            .arg(base.path().join("nodes").join(id(4)))
            .arg(overlay.path().join("nodes"))
            .status()
            .unwrap()
            .success()
    );
    // Placement changed, so the old manifest is not complete for the hydrated view.
    assert_eq!(inspect(overlay.path(), Some(base.path())).status, "stale");
    let hydrated = refresh(overlay.path(), Some(base.path()));
    assert_eq!(sparse.version, hydrated.version);
    assert_eq!(inspect(overlay.path(), Some(base.path())).status, "current");
    assert_eq!(Store::read(base.path()).unwrap().inventory, before);
}
#[test]
fn source_staged_and_destination_tamper_reject_before_publication() {
    let d = fixture();
    let p = d.path();
    refresh(p, None);
    assert_eq!(
        paused(p, |p| field(p, 4, "name", json!("Newer bytes"))).status,
        "stale"
    );
    assert_eq!(inspect(p, None).status, "stale");
    refresh(p, None);
    assert_eq!(
        paused(p, |p| {
            let stage = p.join(".sifttext/outline-stage");
            fs::write(
                fs::read_dir(stage).unwrap().next().unwrap().unwrap().path(),
                "forged candidate",
            )
            .unwrap();
        })
        .status,
        "stale"
    );
    refresh(p, None);
    assert_eq!(
        paused(p, |p| fs::write(
            p.join("nodes").join(id(1)).join("outline.lean"),
            "forged target"
        )
        .unwrap())
        .status,
        "stale"
    );
    assert_eq!(inspect(p, None).status, "stale");
    refresh(p, None);
    assert_eq!(inspect(p, None).status, "current");
}
#[test]
fn generated_symlinks_and_hardlinks_are_rejected_without_writes_through() {
    for hard in [false, true] {
        let d = fixture();
        refresh(d.path(), None);
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), "untouched").unwrap();
        let file = d.path().join("nodes").join(id(1)).join("outline.lean");
        fs::remove_file(&file).unwrap();
        if hard {
            fs::hard_link(outside.path(), &file).unwrap();
        } else {
            std::os::unix::fs::symlink(outside.path(), &file).unwrap();
        }
        assert_eq!(refresh(d.path(), None).status, "error");
        assert_eq!(inspect(d.path(), None).status, "error");
        assert_eq!(fs::read_to_string(outside.path()).unwrap(), "untouched");
    }
}

fn validated(policy: &Policy, path: &Path) -> crate::candidate_validation::ValidatedCapture {
    let (report, capture) = crate::candidate_validation::validate_with_capture(
        policy,
        &crate::candidate_validation::Request {
            store: path.into(),
            before: None,
            explicit: Default::default(),
            expected_version: None,
            pause_before_accept: None,
        },
        None,
    );
    assert_eq!(report.status, "current", "{report:?}");
    capture.expect("current validation must retain its exact capture")
}

// Count real helper invocations, never substitute an accepting script.
pub(super) fn counted_policy(project: &Path) -> Policy {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = crate::candidate_files(source).unwrap();
    files.extend([
        "build/candidate.sha256".into(),
        "build/graph-policy.sha256".into(),
        "build/graph-policy".into(),
    ]);
    for file in files {
        let target = project.join(&file);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source.join(file), target).unwrap();
    }
    let helper = project.join("build/graph-policy");
    fs::rename(&helper, project.join("build/graph-policy.real")).unwrap();
    fs::write(
        &helper,
        "#!/bin/sh\nset -eu\nd=$(dirname \"$0\")\nprintf 'call\\n' >> \"$d/calls\"\nexec \"$d/graph-policy.real\" \"$@\"\n",
    ).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        project.join("build/graph-policy.sha256"),
        format!(
            "{}  build/graph-policy\n",
            crate::sha256(&fs::read(&helper).unwrap())
        ),
    )
    .unwrap();
    Policy::checked(project).unwrap()
}

#[test]
fn validated_inspection_matches_public_bytes_without_repeating_full_law() {
    let temp = tempfile::tempdir().unwrap();
    let policy = counted_policy(&temp.path().join("policy"));
    let d = fixture();
    assert_eq!(
        index::refresh(&policy, d.path(), None, Mode::Full, None)
            .unwrap()
            .status,
        "current"
    );
    let capture = validated(&policy, d.path());
    assert_verification_matches(&policy, d.path(), &capture);
    let calls = temp.path().join("policy/build/calls");
    fs::write(&calls, "").unwrap();
    let public = outlines::inspect(&policy, d.path(), None).unwrap();
    assert_eq!(public.status, "current");
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 1);
    fs::write(&calls, "").unwrap();
    for _ in 0..2 {
        let reused = outlines::inspect_validated(&policy, d.path(), &capture).unwrap();
        assert_eq!(
            serde_json::to_value(reused).unwrap(),
            serde_json::to_value(&public).unwrap()
        );
    }
    assert_eq!(fs::read_to_string(&calls).unwrap(), "");
    // An identical graph at a different canonical path is not this capability.
    let other = fixture();
    assert_eq!(
        index::refresh(&policy, other.path(), None, Mode::Full, None)
            .unwrap()
            .status,
        "current"
    );
    assert_ne!(
        outlines::inspect_validated(&policy, other.path(), &capture)
            .unwrap()
            .status,
        "current"
    );
    assert_verification_matches(&policy, other.path(), &capture);
    let foreign = counted_policy(&temp.path().join("foreign"));
    assert_verification_matches(&foreign, d.path(), &capture);
    assert_ne!(
        outlines::inspect_validated(&foreign, d.path(), &capture)
            .unwrap()
            .status,
        "current"
    );
    // A same-byte replacement must invalidate physical source identity.
    let node = d.path().join("nodes").join(id(1)).join("node.md");
    let bytes = fs::read(&node).unwrap();
    let replacement = temp.path().join("replacement");
    fs::write(&replacement, &bytes).unwrap();
    fs::rename(replacement, &node).unwrap();
    assert_verification_matches(&policy, d.path(), &capture);
    assert_ne!(
        outlines::inspect_validated(&policy, d.path(), &capture)
            .unwrap()
            .status,
        "current"
    );
    assert_eq!(
        outlines::inspect(&policy, d.path(), None).unwrap().status,
        "current"
    );
    let capture = validated(&policy, d.path());
    fs::write(&node, "changed").unwrap();
    fs::write(&node, bytes).unwrap();
    assert_verification_matches(&policy, d.path(), &capture);
    assert_ne!(
        outlines::inspect_validated(&policy, d.path(), &capture)
            .unwrap()
            .status,
        "current"
    );
}

fn assert_verification_matches(
    policy: &Policy,
    path: &Path,
    capture: &crate::candidate_validation::ValidatedCapture,
) {
    let inspection = outlines::inspect_validated(policy, path, capture).unwrap();
    let verified = outlines::verify_validated(policy, path, capture).unwrap();
    assert_eq!(verified.status, inspection.status);
    assert_eq!(verified.version, inspection.version);
    assert!(verified.files.is_empty());
}

#[test]
fn validated_inspection_keeps_independent_sql_manifest_artifact_and_cost_checks() {
    let policy = policy();
    let d = fixture();
    let p = d.path();
    assert_eq!(
        index::refresh(&policy, p, None, Mode::Full, None)
            .unwrap()
            .status,
        "current"
    );
    let capture = validated(&policy, p);
    let db_path = p.join(".sifttext/index.sqlite");
    for sql in [
        "UPDATE nodes SET name='forged'",
        "UPDATE nodes SET record='{}'",
        "INSERT INTO links(source,position,target,reason) SELECT id,0,id,'forged' FROM nodes LIMIT 1",
        "UPDATE metadata SET value='{}' WHERE key='outlines'",
        "UPDATE metadata SET value='forged' WHERE key='version'",
    ] {
        let db = rusqlite::Connection::open(&db_path).unwrap();
        db.execute_batch("BEGIN IMMEDIATE").unwrap();
        db.execute_batch(sql).unwrap();
        db.execute_batch("COMMIT").unwrap();
        drop(db);
        assert_eq!(
            outlines::inspect_validated(&policy, p, &capture)
                .unwrap()
                .status,
            "stale",
            "{sql}"
        );
        assert_verification_matches(&policy, p, &capture);
        assert_eq!(
            index::refresh(&policy, p, None, Mode::Full, None)
                .unwrap()
                .status,
            "current"
        );
    }
    let artifact = p.join("nodes").join(id(1)).join("outline.full.lean");
    let original = fs::read(&artifact).unwrap();
    fs::write(&artifact, "forged").unwrap();
    assert_eq!(
        outlines::inspect_validated(&policy, p, &capture)
            .unwrap()
            .status,
        "stale"
    );
    assert_verification_matches(&policy, p, &capture);
    fs::write(&artifact, original).unwrap();
    let meta = artifact.with_file_name("outline.full.lean.meta.json");
    let original = fs::read(&meta).unwrap();
    let mut cost: Value = serde_json::from_slice(&original).unwrap();
    cost["tokens"] = json!(0);
    fs::write(&meta, serde_json::to_vec(&cost).unwrap()).unwrap();
    assert_eq!(
        outlines::inspect_validated(&policy, p, &capture)
            .unwrap()
            .status,
        "stale"
    );
    assert_verification_matches(&policy, p, &capture);
    fs::write(meta, original).unwrap();
    assert_verification_matches(&policy, p, &capture);
    assert_eq!(
        outlines::inspect_validated(&policy, p, &capture)
            .unwrap()
            .status,
        "current"
    );
}
