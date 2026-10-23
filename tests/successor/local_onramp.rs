use crate::{Policy, Store, checked_publication, local};
use std::{collections::BTreeSet, fs, path::Path, process::Command};

const ROOT: &str = "10000000-0000-4000-8000-000000000001";

fn git(path: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}
fn seed(path: &Path) {
    fs::create_dir_all(path.join(format!("nodes/{ROOT}/children"))).unwrap();
    fs::create_dir(path.join("trees")).unwrap();
    fs::write(path.join(format!("nodes/{ROOT}/children/.keep")), "").unwrap();
    fs::write(path.join(format!("nodes/{ROOT}/children.order")), "").unwrap();
    fs::write(path.join(format!("nodes/{ROOT}/node.md")), format!(
        "---\nid: \"{ROOT}\"\nname: \"Synthetic root\"\nmicro_summary: \"Preserves a reversible synthetic trial.\"\nstatus: \"not_examined\"\nsee_also: []\n---\nAuthored bytes — café.\n"
    )).unwrap();
    std::os::unix::fs::symlink(format!("../nodes/{ROOT}"), path.join("trees").join(ROOT)).unwrap();
}
fn publish(policy: &Policy, home: &Path, draft: &Path, base: &str) -> checked_publication::Receipt {
    git(draft, &["add", "--", "nodes", "trees"]);
    git(
        draft,
        &["commit", "-m", "knowledge: create synthetic trial"],
    );
    let commit = git(draft, &["rev-parse", "HEAD"]);
    checked_publication::submit(
        policy,
        &checked_publication::Request {
            repo: home.into(),
            accepted_ref: "refs/heads/main".into(),
            draft: Some(draft.into()),
            commit: Some(commit),
            base: Some(base.into()),
            explicit: BTreeSet::new(),
            pause_before_publish: None,
            pause_after_publish: None,
        },
    )
}

#[test]
fn supplied_init_preserves_edits_and_uses_current_components() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("bundle");
    seed(&bundle);
    let policy = policy();
    let home = dir.path().join("home");
    assert_eq!(
        local::init(&policy, &home, Some(&bundle)).unwrap()["created"],
        true
    );
    assert_eq!(Store::read(&home).unwrap().nodes.len(), 1);
    let exclude = fs::read_to_string(home.join(".git/info/exclude")).unwrap();
    assert!(exclude.lines().any(|line| line == "/nodes/*/artifacts/chats/"));
    assert!(exclude.lines().any(|line| line == "/nodes/*/artifacts/"));
    assert_eq!(
        crate::candidate_validation::version(&Store::read(&home).unwrap()),
        crate::candidate_validation::version(&Store::read(&bundle).unwrap()),
    );
    assert_eq!(
        local::read(&policy, &home, ROOT).unwrap(),
        fs::read_to_string(home.join(format!("nodes/{ROOT}/node.md"))).unwrap()
    );
    let authored = home.join(format!("nodes/{ROOT}/node.md"));
    let original = fs::read(&authored).unwrap();
    fs::write(&authored, "a real unfinished user edit").unwrap();
    assert_eq!(
        local::init(&policy, &home, Some(Path::new("missing-bundle"))).unwrap()["created"],
        false
    );
    assert_eq!(fs::read(&authored).unwrap(), b"a real unfinished user edit");
    assert!(local::refresh(&policy, &home).is_err());
    fs::write(&authored, original).unwrap();
    let allocation = local::worktree(&policy, &home).unwrap();
    let draft = Path::new(allocation["worktree"].as_str().unwrap());
    local::create(
        &policy,
        draft,
        "Trial",
        "Owns a reversible test.",
        Some(ROOT),
    )
    .unwrap();
    let sql = local::sql(&policy, draft, "SELECT count(*) FROM nodes").unwrap();
    assert_eq!(sql["result"]["rows"][0][0], 2);
    for mutation in [
        "DELETE FROM nodes",
        "PRAGMA user_version=1",
        "SELECT 1; DELETE FROM nodes",
        "ATTACH 'other' AS other",
    ] {
        assert!(local::sql(&policy, draft, mutation).is_err(), "{mutation}");
    }
    let receipt = publish(&policy, &home, draft, allocation["base"].as_str().unwrap());
    assert_eq!(receipt.status, "current", "{receipt:?}");
    assert!(receipt.published);
    let candidate = receipt.candidate.as_ref().unwrap();
    assert_eq!(git(&home, &["rev-parse", "refs/heads/main"]), *candidate);
    let submitted = receipt.draft_commit.as_ref().unwrap();
    assert_eq!(
        git(&home, &["rev-parse", &format!("{candidate}^{{tree}}")]),
        git(&home, &["rev-parse", &format!("{submitted}^{{tree}}")])
    );
    assert_eq!(Store::read(&receipt.view.unwrap()).unwrap().nodes.len(), 2);
    // Independent accepted publication leaves the original reader and authored bytes untouched.
    assert_eq!(Store::read(&home).unwrap().nodes.len(), 1);
}

#[test]
fn local_artifacts_are_opaque_but_not_an_escape_for_authored_paths_or_orphan_owners() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    let before = Store::read(dir.path()).unwrap();
    let node = dir.path().join("nodes").join(ROOT);
    let artifacts = node.join("artifacts");
    fs::create_dir(&artifacts).unwrap();
    fs::write(artifacts.join("budget.csv"), b"item,quantity\nwidget,10\n").unwrap();
    let captured = crate::coherent_capture::Capture::read(dir.path(), None).unwrap();
    assert_eq!(captured.store.inventory, before.inventory);
    fs::write(artifacts.join("budget.csv"), b"item,quantity\nwidget,20\n").unwrap();
    captured.unchanged_all().unwrap(); // Artifact content is not knowledge freshness.
    let external = tempfile::tempdir().unwrap();
    fs::write(external.path().join("user.txt"), "external bytes").unwrap();
    let parked = dir.path().join("parked");
    fs::rename(&artifacts, &parked).unwrap();
    std::os::unix::fs::symlink(external.path(), &artifacts).unwrap();
    assert!(Store::read(dir.path()).is_err(), "artifact root must be a real container");
    assert_eq!(fs::read(external.path().join("user.txt")).unwrap(), b"external bytes");
    fs::remove_file(&artifacts).unwrap();
    fs::rename(&parked, &artifacts).unwrap();
    let unrelated = node.join("external");
    std::os::unix::fs::symlink(external.path(), &unrelated).unwrap();
    assert!(Store::read(dir.path()).is_err(), "unrelated symlink escaped validation");
    fs::remove_file(unrelated).unwrap();
    let authored = node.join("node.md");
    fs::remove_file(&authored).unwrap();
    assert!(Store::read(dir.path()).is_err(), "artifacts do not authorize a source-less owner");
    assert_eq!(fs::read(artifacts.join("budget.csv")).unwrap(), b"item,quantity\nwidget,20\n");
}

#[test]
fn init_rejects_a_starter_changed_during_capture_before_creating_home() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("bundle");
    seed(&bundle);
    let source = bundle.join(format!("nodes/{ROOT}/node.md"));
    crate::coherent_capture::on_inventory(move || {
        let mut bytes = fs::read(&source).unwrap();
        bytes.extend_from_slice(b"Changed during capture.\n");
        fs::write(source, bytes).unwrap();
    });
    let home = dir.path().join("home");
    assert!(local::init(&policy(), &home, Some(&bundle)).is_err());
    assert!(!home.exists());
}

#[test]
fn empty_init_then_first_checked_publication() {
    let dir = tempfile::tempdir().unwrap();
    let policy = policy();
    let home = dir.path().join("empty-home");
    local::init(&policy, &home, None).unwrap();
    assert!(Store::read(&home).unwrap().nodes.is_empty());
    let allocation = local::worktree(&policy, &home).unwrap();
    let draft = Path::new(allocation["worktree"].as_str().unwrap());
    local::create(
        &policy,
        draft,
        "First tree",
        "Starts an explicitly empty home.",
        None,
    )
    .unwrap();
    let receipt = publish(&policy, &home, draft, allocation["base"].as_str().unwrap());
    assert_eq!(receipt.status, "current", "{receipt:?}");
}
