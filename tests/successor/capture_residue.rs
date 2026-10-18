//! Synthetic recovery and adversarial physical-capture checks.
use crate::{Policy, coherent_capture::Capture, index};
use std::{fs, path::Path, process::Command};

const ROOT: &str = "71000000-0000-4000-8000-000000000001";
const CREATED: &str = "71000000-0000-4000-8000-000000000002";
fn node(store: &Path, id: &str) {
    let dir = store.join("nodes").join(id);
    fs::create_dir_all(dir.join("children")).unwrap();
    fs::write(dir.join("children/.keep"), "").unwrap();
    fs::write(dir.join("children.order"), "").unwrap();
    fs::write(dir.join("node.md"), format!("---\nid: \"{id}\"\nname: \"Synthetic node\"\nmicro_summary: \"Independent recovery specimen\"\nstatus: \"in_progress\"\nsee_also: []\n---\nExact authored body Ω.\n")).unwrap();
    std::os::unix::fs::symlink(format!("../nodes/{id}"), store.join("trees").join(id)).unwrap();
}
fn git(store: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(store)
        .args([
            "-c",
            "user.name=Synthetic Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}
fn fixture() -> (tempfile::TempDir, index::View) {
    let temp = tempfile::tempdir().unwrap();
    let store = temp.path();
    fs::create_dir(store.join("trees")).unwrap();
    node(store, ROOT);
    fs::write(
        store.join(".gitignore"),
        ".sifttext/\n*.lean\n*.meta.json\n*.meta.lock\n",
    )
    .unwrap();
    git(store, &["init", "-q", "-b", "main"]);
    git(store, &["add", "."]);
    git(store, &["commit", "-qm", "fixture: baseline"]);
    let baseline = index::refresh(&policy(), store, None, index::Mode::Full, None).unwrap();
    node(store, CREATED);
    git(store, &["add", "nodes", "trees"]);
    git(store, &["commit", "-qm", "fixture: creation"]);
    crate::token_metadata::Refresher::default()
        .refresh(&store.join("nodes").join(CREATED).join("node.md"), None)
        .unwrap();
    index::refresh(&policy(), store, None, index::Mode::Incremental, None).unwrap();
    git(store, &["revert", "--no-edit", "HEAD"]);
    (temp, baseline)
}
fn residue(store: &Path, name: &str) -> std::path::PathBuf {
    store.join("nodes").join(CREATED).join(name)
}
fn replace(p: &Path) {
    let bytes = fs::read(p).unwrap();
    let replacement = p.with_extension("replacement");
    fs::write(&replacement, bytes).unwrap();
    fs::rename(replacement, p).unwrap();
}

#[test]
fn ordinary_git_revert_recovers_and_cleanup_removes_residue() {
    let (temp, baseline) = fixture();
    let store = temp.path();
    let tree = git(store, &["rev-parse", "HEAD^{tree}"]);
    let bytes = fs::read(store.join("nodes").join(ROOT).join("node.md")).unwrap();
    let captured = Capture::read(store, None).unwrap();
    assert_eq!(
        crate::candidate_validation::version(&captured.store),
        baseline.version.clone().unwrap()
    );
    assert_eq!(captured.store.nodes.len(), 1);
    let view = index::refresh(&policy(), store, None, index::Mode::Incremental, None).unwrap();
    assert_eq!(view.status, "current");
    assert_eq!(view.version, baseline.version);
    assert_eq!(git(store, &["rev-parse", "HEAD^{tree}"]), tree);
    assert!(git(store, &["status", "--porcelain"]).is_empty());
    assert_eq!(
        fs::read(store.join("nodes").join(ROOT).join("node.md")).unwrap(),
        bytes
    );
    // Strict cleanup is the selected behavior: refreshing a reverted-away node
    // removes its generated residue rather than leaving it behind.
    assert!(!residue(store, "outline.full.lean").exists());
    assert!(!residue(store, "outline.lean").exists());
    let settled = Capture::read(store, None).unwrap();
    index::refresh(&policy(), store, None, index::Mode::Incremental, None).unwrap();
    settled.unchanged().unwrap();
    // Historical proof comes from Git source, not the editable or missing cache.
    fs::remove_file(store.join(".sifttext/index.sqlite")).unwrap();
    assert_eq!(
        index::refresh(&policy(), store, None, index::Mode::Full, None)
            .unwrap()
            .status,
        "current"
    );
}

#[test]
fn every_orphan_resource_is_fail_closed_and_preserved() {
    let (temp, _) = fixture();
    let store = temp.path();
    let p = residue(store, "unknown.txt");
    fs::write(&p, "protected bytes").unwrap();
    assert!(Capture::read(store, None).is_err());
    assert_eq!(fs::read(&p).unwrap(), b"protected bytes");
    fs::remove_file(p).unwrap();
    let p = residue(store, "outline.lean");
    let original = fs::read(&p).unwrap();
    fs::write(&p, "foreign content").unwrap();
    assert!(Capture::read(store, None).is_err());
    fs::write(&p, &original).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&p)
        .unwrap()
        .set_len((crate::LIMIT + 1) as u64)
        .unwrap();
    assert!(
        Capture::read(store, None).is_err(),
        "oversized unknown bytes cannot match a bounded historical receipt"
    );
    fs::write(&p, &original).unwrap();
    fs::remove_file(&p).unwrap();
    fs::create_dir(&p).unwrap();
    fs::write(p.join("unknown.txt"), "nested protected bytes").unwrap();
    assert!(Capture::read(store, None).is_err());
    fs::remove_dir_all(&p).unwrap();
    fs::write(&p, &original).unwrap();
    let target = store.join("protected");
    fs::write(&target, &original).unwrap();
    fs::remove_file(&p).unwrap();
    std::os::unix::fs::symlink(&target, &p).unwrap();
    assert!(Capture::read(store, None).is_err());
    fs::remove_file(&p).unwrap();
    fs::hard_link(&target, &p).unwrap();
    assert!(Capture::read(store, None).is_err());
    fs::remove_file(&p).unwrap();
    fs::write(&p, original).unwrap();
    fs::write(residue(store, "children.order"), "").unwrap();
    assert!(
        Capture::read(store, None).is_err(),
        "partial authored nodes cannot be residue"
    );
}

#[test]
fn forged_token_metadata_and_lock_are_not_exempted() {
    let (temp, _) = fixture();
    let store = temp.path();
    let p = residue(store, "node.meta.json");
    let bytes = fs::read(&p).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["tokens"] = serde_json::json!(0);
    fs::write(&p, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(Capture::read(store, None).is_err());
    fs::write(&p, bytes).unwrap();
    let p = residue(store, "node.meta.lock");
    fs::write(&p, "protected lock payload").unwrap();
    assert!(Capture::read(store, None).is_err());
}

#[test]
fn identity_barriers_cover_source_residue_and_membership_aba() {
    let (temp, _) = fixture();
    let store = temp.path();
    for p in [
        residue(store, "outline.lean"),
        residue(store, "node.meta.json"),
        store.join("nodes").join(ROOT).join("node.md"),
    ] {
        let capture = Capture::read(store, None).unwrap();
        replace(&p);
        assert!(
            capture.unchanged().is_err(),
            "same-byte replacement at {}",
            p.display()
        );
    }
    let capture = Capture::read(store, None).unwrap();
    fs::write(residue(store, "unknown.txt"), "ABA").unwrap();
    fs::remove_file(residue(store, "unknown.txt")).unwrap();
    assert!(capture.unchanged().is_err());
    let p = residue(store, "outline.full.lean");
    crate::coherent_capture::on_inventory(move || replace(&p));
    assert!(
        Capture::read(store, None).is_err(),
        "replacement between inventory and acceptance"
    );
}

#[test]
fn sparse_base_and_overlay_keep_independent_residue_identities() {
    let (base, baseline) = fixture();
    let overlay = tempfile::tempdir().unwrap();
    fs::create_dir(overlay.path().join("nodes")).unwrap();
    fs::create_dir(overlay.path().join("trees")).unwrap();
    std::os::unix::fs::symlink(
        format!("../nodes/{ROOT}"),
        overlay.path().join("trees").join(ROOT),
    )
    .unwrap();
    let capture = Capture::read(overlay.path(), Some(base.path())).unwrap();
    assert_eq!(
        crate::candidate_validation::version(&capture.store),
        baseline.version.unwrap()
    );
    replace(&residue(base.path(), "outline.lean"));
    assert!(capture.unchanged().is_err());
    let (overlay, _) = fixture();
    let clean_base = tempfile::tempdir().unwrap();
    fs::create_dir(clean_base.path().join("trees")).unwrap();
    node(clean_base.path(), ROOT);
    let capture = Capture::read(overlay.path(), Some(clean_base.path())).unwrap();
    replace(&residue(overlay.path(), "node.meta.json"));
    assert!(capture.unchanged().is_err());
}

#[test]
fn missing_or_wrong_historical_source_never_mints_provenance() {
    let (temp, _) = fixture();
    let store = temp.path();
    let capture = Capture::read(store, None).unwrap();
    // A later empty commit removes the owning source from the immediate parent.
    git(
        store,
        &[
            "commit",
            "--allow-empty",
            "-qm",
            "fixture: no historical owner",
        ],
    );
    assert!(
        capture.unchanged().is_err(),
        "historical Git selection is retained through acceptance"
    );
    // A bounded first-parent search still finds the nearest deletion commit, so a
    // later empty commit neither removes nor falsifies the historical owner. The
    // retired immediate-parent-only rule required refusal here; the current capture
    // must still resolve only the surviving node and no provenance for the reverted one.
    let rebound = Capture::read(store, None).unwrap();
    assert_eq!(rebound.store.nodes.len(), 1);
    let (temp, _) = fixture();
    let store = temp.path();
    fs::rename(store.join(".git"), store.join("git-preserved")).unwrap();
    assert!(Capture::read(store, None).is_err());
    let (temp, _) = fixture();
    let store = temp.path();
    git(store, &["reset", "--soft", "HEAD^1"]);
    assert!(
        Capture::read(store, None).is_err(),
        "current committed owner was only physically deleted"
    );
}
