//! File-first home pointer: ordinary discovery for a NEW supplied native KB home.
use super::{
    CHAT_ARTIFACTS_RULE, IGNORE, PIPELINE, POINTER, POINTER_RULE, advance, carries_pipeline,
    exclude_local_artifacts, pointer_text,
    write_pointer,
};
use crate::{Policy, Store, checked_publication, local};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const ROOT: &str = "10000000-0000-4000-8000-000000000001";
const ENTRY: &str = local::ENTRY;
const USER: &str = "a real unfinished AGENTS.md edit \u{2014} caf\u{e9}\n";

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

fn node(dir: &Path, id: &str, name: &str, children: &[&str]) {
    let at = dir.join("nodes").join(id);
    fs::create_dir_all(at.join("children")).unwrap();
    fs::write(at.join("node.md"), format!(
        "---\nid: \"{id}\"\nname: \"{name}\"\nmicro_summary: \"Keeps the pointer trial reversible.\"\nstatus: \"not_examined\"\nsee_also: []\n---\nAuthored bytes \u{2014} caf\u{e9} \u{4e2d}.\n"
    )).unwrap();
    fs::write(
        at.join("children.order"),
        children
            .iter()
            .map(|c| c.to_string() + "\n")
            .collect::<String>(),
    )
    .unwrap();
    if children.is_empty() {
        fs::write(at.join("children/.keep"), b"").unwrap();
    }
    for child in children {
        std::os::unix::fs::symlink(format!("../../{child}"), at.join("children").join(child))
            .unwrap();
    }
}

/// Generic synthetic source. `full` selects the complete canonical pipeline;
/// otherwise the entrypoint keeps only one non-canonical child.
fn bundle(dir: &Path, full: bool) {
    fs::create_dir_all(dir.join("trees")).unwrap();
    let children: Vec<&str> = if full {
        PIPELINE.to_vec()
    } else {
        vec![PIPELINE[0]]
    };
    node(dir, ROOT, "Synthetic root", &[ENTRY]);
    node(dir, ENTRY, "Synthetic entrypoint", &children);
    for child in children {
        node(dir, child, "Synthetic instruction", &[]);
    }
    std::os::unix::fs::symlink(format!("../nodes/{ROOT}"), dir.join("trees").join(ROOT)).unwrap();
}

/// Every `nodes/...` path the pointer names, in the order it names them.
fn named_paths(home: &Path) -> Vec<String> {
    let text = fs::read_to_string(home.join(POINTER)).unwrap();
    let root = fs::canonicalize(home).unwrap();
    let mut found = Vec::new();
    for word in text.split_whitespace() {
        let clean = word.trim_matches(|c| "`,".contains(c));
        if !clean.starts_with("nodes/") {
            continue;
        }
        assert!(!clean.contains(".."), "the pointer escapes the home");
        let resolved = root.join(clean);
        assert!(resolved.starts_with(&root), "{resolved:?} left the home");
        assert!(
            fs::symlink_metadata(&resolved).is_ok_and(|m| m.is_file()),
            "the pointer names an unreadable path {clean}"
        );
        found.push(clean.to_owned());
    }
    found
}

#[test]
fn supplied_home_receives_a_resolvable_file_first_pointer() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("bundle");
    bundle(&source, true);
    let policy = policy();
    let home = dir.path().join("home");
    let created = local::init(&policy, &home, Some(&source)).unwrap();
    assert_eq!(created["created"], true);
    assert_eq!(created["pointer"], POINTER);
    let at = fs::canonicalize(&home).unwrap();

    let pointer = at.join(POINTER);
    let meta = fs::symlink_metadata(&pointer).unwrap();
    assert!(
        meta.is_file() && !meta.is_symlink(),
        "the pointer is not an ordinary file"
    );
    let text = fs::read_to_string(&pointer).unwrap();
    assert_eq!(text, pointer_text());
    // Discovery is local only: no absolute path and no networked tool reference.
    assert!(
        !text.contains(&at.to_string_lossy().to_string()),
        "an absolute path leaked"
    );
    assert!(
        !text.contains("://"),
        "a networked reference entered the pointer"
    );

    // The entry file and its ordered-child source resolve inside this home.
    assert_eq!(
        named_paths(&at),
        vec![
            format!("nodes/{ENTRY}/node.md"),
            format!("nodes/{ENTRY}/children.order")
        ]
    );
    let store = Store::read(&at).unwrap();
    assert_eq!(store.children[ENTRY], PIPELINE.to_vec());
    for child in &PIPELINE {
        let file = at.join(format!("nodes/{child}/node.md"));
        assert!(fs::symlink_metadata(&file).unwrap().is_file(), "{file:?}");
        assert_eq!(
            fs::read(&file).unwrap(),
            fs::read(source.join(format!("nodes/{child}/node.md"))).unwrap(),
            "supplied child bytes changed"
        );
    }
    // The pointer resolves the real entry rather than inventing a substitute.
    assert_eq!(
        fs::read(at.join(format!("nodes/{ENTRY}/node.md"))).unwrap(),
        fs::read(source.join(format!("nodes/{ENTRY}/node.md"))).unwrap()
    );

    // Untracked but excluded: the reader stays clean and the pointer never enters
    // the authored Git history the checked publisher validates.
    assert_eq!(
        git(&at, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    assert!(
        !git(&at, &["ls-files"]).lines().any(|f| f == POINTER),
        "the pointer was committed"
    );
    let exclude = fs::read_to_string(at.join(".git/info/exclude")).unwrap();
    assert!(exclude.contains(&format!("/{POINTER}\n")), "{exclude}");
    assert_eq!(
        fs::read(at.join(".gitignore")).unwrap(),
        IGNORE.as_bytes(),
        "the recognized home marker changed"
    );
}

#[test]
fn pointer_does_not_block_worktree_create_or_publication() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("bundle");
    bundle(&source, true);
    let policy = policy();
    let home = dir.path().join("home");
    local::init(&policy, &home, Some(&source)).unwrap();
    let home = fs::canonicalize(&home).unwrap();

    let allocation = local::worktree(&policy, &home).unwrap();
    let draft = PathBuf::from(allocation["worktree"].as_str().unwrap());
    assert!(
        !draft.join(POINTER).exists(),
        "a derived private worktree gained the home pointer"
    );
    local::create(
        &policy,
        &draft,
        "Pointer trial",
        "Checks a reversible create.",
        Some(ENTRY),
    )
    .unwrap();
    git(&draft, &["add", "--", "nodes", "trees"]);
    git(&draft, &["commit", "-m", "knowledge: create pointer trial"]);
    let receipt = checked_publication::submit(
        &policy,
        &checked_publication::Request {
            repo: home.clone(),
            accepted_ref: "refs/heads/main".into(),
            draft: Some(draft.clone()),
            commit: Some(git(&draft, &["rev-parse", "HEAD"])),
            base: Some(allocation["base"].as_str().unwrap().to_owned()),
            explicit: BTreeSet::new(),
            pause_before_publish: None,
            pause_after_publish: None,
        },
    );
    assert_eq!(receipt.status, "current", "{receipt:?}");
    assert!(receipt.published);
    // The pointer survived checked publication and the reader stayed clean.
    assert_eq!(
        git(&home, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    assert!(fs::symlink_metadata(home.join(POINTER)).unwrap().is_file());
    let view = receipt.view.unwrap();
    assert!(
        !view.join(POINTER).exists(),
        "the accepted view carries authored content only"
    );
    assert_eq!(
        Store::read(&view).unwrap().nodes.len(),
        Store::read(&home).unwrap().nodes.len() + 1
    );
}

#[test]
fn accepted_advance_moves_a_pointer_home_without_touching_the_pointer() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("bundle");
    bundle(&source, true);
    let policy = policy();
    let home = dir.path().join("home");
    let before = local::init(&policy, &home, Some(&source)).unwrap();
    let home = fs::canonicalize(&home).unwrap();
    let pointer = home.join(POINTER);
    let generated = fs::read(&pointer).unwrap();
    assert_eq!(
        before["commit"].as_str(),
        Some(git(&home, &["rev-parse", "HEAD"]).as_str())
    );

    // Publication leaves the detached reader behind, exactly as the accepted reader
    // advance feature expects; the excluded pointer must not make it dirty.
    let allocation = local::worktree(&policy, &home).unwrap();
    let draft = PathBuf::from(allocation["worktree"].as_str().unwrap());
    local::create(
        &policy,
        &draft,
        "Advance a pointer home",
        "Checks that reader advance preserves the generated pointer.",
        None,
    )
    .unwrap();
    git(&draft, &["add", "--", "nodes", "trees"]);
    git(
        &draft,
        &["commit", "-m", "knowledge: advance pointer trial"],
    );
    let receipt = checked_publication::submit(
        &policy,
        &checked_publication::Request {
            repo: home.clone(),
            accepted_ref: "refs/heads/main".into(),
            draft: Some(draft.clone()),
            commit: Some(git(&draft, &["rev-parse", "HEAD"])),
            base: Some(allocation["base"].as_str().unwrap().to_owned()),
            explicit: BTreeSet::new(),
            pause_before_publish: None,
            pause_after_publish: None,
        },
    );
    assert_eq!(receipt.status, "current", "{receipt:?}");
    let candidate = receipt.candidate.clone().unwrap();
    assert_eq!(
        git(&home, &["rev-parse", "HEAD"]),
        before["commit"].as_str().unwrap()
    );

    let moved = advance(&policy, &home).unwrap();
    assert_eq!(moved["advanced"], true, "{moved}");
    assert_eq!(moved["reader"].as_str(), Some(candidate.as_str()));
    assert_eq!(moved["reader"], moved["accepted"]);
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), candidate);
    // The generated pointer is byte-identical, still untracked and still excluded, so
    // a rerun stays clean and the pipeline still resolves by ordinary file reads.
    assert_eq!(fs::read(&pointer).unwrap(), generated);
    assert!(fs::symlink_metadata(&pointer).unwrap().is_file());
    assert_eq!(
        git(&home, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    assert!(!git(&home, &["ls-files"]).lines().any(|f| f == POINTER));
    assert_eq!(
        local::instructions(&policy, &home).unwrap()["files"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
    assert_eq!(
        fs::read(home.join(format!("nodes/{ENTRY}/node.md"))).unwrap(),
        fs::read(source.join(format!("nodes/{ENTRY}/node.md"))).unwrap(),
        "advance changed the supplied entry bytes"
    );
    assert_eq!(advance(&policy, &home).unwrap()["advanced"], false);
}

#[test]
fn empty_and_partial_bundles_get_no_broken_pointer() {
    let dir = tempfile::tempdir().unwrap();
    let policy = policy();

    let empty = dir.path().join("empty");
    let created = local::init(&policy, &empty, None).unwrap();
    assert_eq!(created["pointer"], serde_json::Value::Null);
    assert!(!empty.join(POINTER).exists(), "--empty wrote a pointer");
    assert_eq!(
        git(&empty, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    assert!(match fs::read_to_string(empty.join(".git/info/exclude")) {
        Ok(exclude) => !exclude.contains(POINTER),
        Err(err) => err.kind() == std::io::ErrorKind::NotFound,
    });

    // A custom bundle carrying the entry but not its six ordered children is still
    // valid knowledge and still receives no pointer.
    let partial = dir.path().join("partial-bundle");
    bundle(&partial, false);
    let custom = dir.path().join("custom-home");
    let created = local::init(&policy, &custom, Some(&partial)).unwrap();
    assert_eq!(created["created"], true);
    assert_eq!(created["pointer"], serde_json::Value::Null);
    let custom = fs::canonicalize(&custom).unwrap();
    assert!(
        !custom.join(POINTER).exists(),
        "a partial pipeline wrote a broken pointer"
    );
    assert_eq!(
        git(&custom, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    assert!(
        local::instructions(&policy, &custom).is_err(),
        "instructions must still refuse a partial pipeline"
    );
    assert!(local::worktree(&policy, &custom).is_ok());
    assert!(!carries_pipeline(&Store::read(&partial).unwrap()));

    let full = dir.path().join("full-bundle");
    bundle(&full, true);
    assert!(carries_pipeline(&Store::read(&full).unwrap()));
}

#[test]
fn existing_home_user_pointer_and_authored_edit_are_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("bundle");
    bundle(&source, true);
    let policy = policy();
    let home = dir.path().join("home");
    local::init(&policy, &home, Some(&source)).unwrap();
    let home = fs::canonicalize(&home).unwrap();

    let pointer = home.join(POINTER);
    let marker = home.join(".gitignore");
    let authored = home.join(format!("nodes/{ENTRY}/node.md"));
    let generated = fs::read(&pointer).unwrap();
    let original = fs::read(&authored).unwrap();
    let ignore = fs::read(&marker).unwrap();
    let head = git(&home, &["rev-parse", "HEAD"]);
    let exclude = fs::read(home.join(".git/info/exclude")).unwrap();
    fs::write(&pointer, USER).unwrap();
    fs::write(&authored, "a real unfinished authored edit").unwrap();

    for again in [
        local::init(&policy, &home, Some(&source)).unwrap(),
        local::init(&policy, &home, Some(Path::new("missing-bundle"))).unwrap(),
        local::init(&policy, &home, None).unwrap(),
    ] {
        assert_eq!(again["created"], false);
    }
    assert_eq!(
        fs::read(&pointer).unwrap(),
        USER.as_bytes(),
        "a rerun rewrote the user pointer"
    );
    assert_eq!(
        fs::read(&authored).unwrap(),
        b"a real unfinished authored edit"
    );
    assert_eq!(
        fs::read(&marker).unwrap(),
        ignore,
        "the recognized home marker changed"
    );
    assert_eq!(fs::read(home.join(".git/info/exclude")).unwrap(), exclude);
    assert_eq!(
        git(&home, &["rev-parse", "HEAD"]),
        head,
        "a rerun moved the reader"
    );

    // Even after the authored file is restored, recognition is still not repair.
    fs::write(&authored, original).unwrap();
    assert_eq!(
        local::init(&policy, &home, Some(&source)).unwrap()["created"],
        false
    );
    assert_eq!(fs::read(&pointer).unwrap(), USER.as_bytes());
    assert_ne!(fs::read(&pointer).unwrap(), generated);
}

#[test]
fn conflicting_or_unsafe_targets_are_preserved_and_init_fails() {
    let dir = tempfile::tempdir().unwrap();
    let policy = policy();
    let source = dir.path().join("bundle");
    bundle(&source, true);

    // A destination directory that is not a recognized home is refused and the
    // user's own AGENTS.md survives untouched.
    let occupied = dir.path().join("occupied");
    fs::create_dir_all(occupied.join("nodes")).unwrap();
    fs::write(occupied.join(POINTER), USER).unwrap();
    assert!(local::init(&policy, &occupied, Some(&source)).is_err());
    assert_eq!(fs::read(occupied.join(POINTER)).unwrap(), USER.as_bytes());

    // A planted symlink at the pointer target is never followed or replaced.
    let outside = dir.path().join("outside-target");
    fs::write(&outside, "private user file").unwrap();
    let planted = dir.path().join("planted");
    fs::create_dir_all(planted.join(format!("nodes/{ENTRY}"))).unwrap();
    fs::write(
        planted.join(format!("nodes/{ENTRY}/node.md")),
        fs::read(source.join(format!("nodes/{ENTRY}/node.md"))).unwrap(),
    )
    .unwrap();
    std::os::unix::fs::symlink(&outside, planted.join(POINTER)).unwrap();
    assert!(
        write_pointer(&planted).is_err(),
        "a symlinked target was accepted"
    );
    assert_eq!(fs::read_to_string(&outside).unwrap(), "private user file");
    assert!(
        fs::symlink_metadata(planted.join(POINTER))
            .unwrap()
            .is_symlink()
    );
    assert!(local::init(&policy, &planted, Some(&source)).is_err());
    assert_eq!(fs::read_to_string(&outside).unwrap(), "private user file");

    // A missing entry file cannot produce a pointer.
    let homeless = dir.path().join("homeless");
    fs::create_dir_all(homeless.join("nodes")).unwrap();
    assert!(
        write_pointer(&homeless).is_err(),
        "a pointer was written for an absent entry"
    );
    assert!(!homeless.join(POINTER).exists());

    // A failing starter check creates nothing at all and claims no success.
    let broken = dir.path().join("broken-bundle");
    bundle(&broken, true);
    let target = broken.join(format!("nodes/{ENTRY}/node.md"));
    let text = fs::read_to_string(&target).unwrap();
    fs::write(
        &target,
        text.replace(
            "micro_summary: \"Keeps the pointer trial reversible.\"",
            "micro_summary: \"   \"",
        ),
    )
    .unwrap();
    let unmade = dir.path().join("never-created");
    assert!(local::init(&policy, &unmade, Some(&broken)).is_err());
    assert!(!unmade.exists(), "a rejected bundle still created a home");

    // Exclusion is local, idempotent and preserves an existing user rule.
    let repo = dir.path().join("repo");
    fs::create_dir_all(repo.join(".git/info")).unwrap();
    fs::write(repo.join(".git/info/exclude"), b"keep-me").unwrap();
    exclude_local_artifacts(&repo, true).unwrap();
    exclude_local_artifacts(&repo, true).unwrap();
    let rules = fs::read_to_string(repo.join(".git/info/exclude")).unwrap();
    assert_eq!(rules.lines().next(), Some("keep-me"));
    assert_eq!(rules.lines().filter(|line| *line == POINTER_RULE).count(), 1);
    assert_eq!(
        rules
            .lines()
            .filter(|line| *line == CHAT_ARTIFACTS_RULE)
            .count(),
        1
    );
    fs::write(repo.join(".git/config"), b"not an exclude file").unwrap();
    let linked = repo.join(".git/info/exclude");
    fs::remove_file(&linked).unwrap();
    std::os::unix::fs::symlink(repo.join(".git/config"), &linked).unwrap();
    assert!(
        exclude_local_artifacts(&repo, true).is_err(),
        "a symlinked exclude was accepted"
    );
    assert_eq!(
        fs::read_to_string(repo.join(".git/config")).unwrap(),
        "not an exclude file"
    );
}
