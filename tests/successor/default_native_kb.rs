use super::*;
use crate::{Store, candidate_validation};
use std::collections::BTreeSet;
use std::{path::PathBuf, process::Command};

fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn git(home: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(home)
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

#[test]
fn derived_skills_retain_the_pinned_upstream_license() {
    use sha2::{Digest, Sha256};

    let derived: Vec<_> = FILES
        .iter()
        .filter(|(_, text)| text.contains("https://github.com/mattpocock/skills"))
        .collect();
    assert_eq!(derived.len(), 5);
    let seed = tempfile::tempdir().unwrap();
    materialize(seed.path()).unwrap();
    let store = Store::read(seed.path()).unwrap();
    let tokenizer = tiktoken_rs::o200k_base_singleton();
    for (path, text) in derived {
        let id = path.split('/').nth(1).unwrap();
        let tokens = tokenizer.lock().encode_ordinary(&store.nodes[id].content).len();
        assert!(tokens <= candidate_validation::CEILING, "{id}: {tokens} tokens");
        let notice = text.split_once("```text\nMIT License\n").unwrap().1;
        let notice = format!("MIT License\n{}", notice.split_once("```\n").unwrap().0);
        // Verbatim LICENSE at the revision recorded in each node's provenance.
        assert_eq!(
            format!("{:x}", Sha256::digest(notice.as_bytes())),
            "0e7ac423bf2c6e223b7c5b156f8cf72da49d748e56a1641402c31f22ad07dbb5",
            "upstream notice changed or missing in {path}"
        );
    }
}

#[test]
fn readable_source_is_the_compiled_seed_and_file_first_bootstrap() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    verify_assets(root).unwrap();
    let pointer = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert!(
        fs::symlink_metadata(root.join("AGENTS.md"))
            .unwrap()
            .is_file()
    );
    assert!(pointer.contains(&format!("nodes/{}/node.md", local::ENTRY)));
    let order =
        fs::read_to_string(root.join(format!("nodes/{}/children.order", local::ENTRY))).unwrap();
    assert_eq!(order.lines().count(), 6);
    for id in std::iter::once(local::ENTRY).chain(order.lines()) {
        assert!(
            !fs::read(root.join(format!("nodes/{id}/node.md")))
                .unwrap()
                .is_empty()
        );
    }
    // Exact ordinary bytes and symlink targets, not a second hand-authored KB.
    let temp = tempfile::tempdir().unwrap();
    materialize(temp.path()).unwrap();
    for (name, text) in FILES {
        assert_eq!(fs::read(root.join(name)).unwrap(), text.as_bytes());
        assert_eq!(
            fs::read(root.join(name)).unwrap(),
            fs::read(temp.path().join(name)).unwrap()
        );
    }
    for (name, target) in LINKS {
        assert_eq!(fs::read_link(root.join(name)).unwrap(), Path::new(target));
    }
    for name in ["build.rs", "AGENTS.md", "install-local.sh"] {
        fs::copy(root.join(name), temp.path().join(name)).unwrap();
    }
    verify_assets(temp.path()).unwrap();
    let link = temp.path().join(LINKS[0].0);
    fs::remove_file(&link).unwrap();
    symlink("../../00000000-0000-4000-8000-000000000000", &link).unwrap();
    assert!(verify_assets(temp.path()).is_err());
    fs::remove_file(&link).unwrap();
    symlink(LINKS[0].1, &link).unwrap();
    fs::write(
        temp.path().join(format!("nodes/{}/node.md", local::ENTRY)),
        "changed",
    )
    .unwrap();
    assert!(verify_assets(temp.path()).is_err());
}

#[test]
fn embedded_inventory_is_complete_and_copied_by_native_init() {
    let parent = tempfile::tempdir().unwrap();
    let seed = parent.path().join("seed");
    fs::create_dir(&seed).unwrap();
    materialize(&seed).unwrap();
    let store = Store::read(&seed).unwrap();
    assert_eq!(store.nodes.len(), 80);
    assert_eq!(store.roots, ["38a9fc79-8a84-494d-a962-2ec5134bc856"]);
    assert!(store.nodes.values().any(|n| n.name == "Skill Library"));
    let p = policy();
    local::check(&p, &seed, None, BTreeSet::new()).unwrap();
    let home = parent.path().join("home");
    assert_eq!(init(&p, &home).unwrap()["starter_nodes"], 80);
    let copied = Store::read(&home).unwrap();
    for (path, entry) in &store.inventory {
        assert_eq!(
            copied.inventory.get(path),
            Some(entry),
            "{}",
            path.display()
        );
    }
    assert_eq!(
        candidate_validation::version(&copied),
        candidate_validation::version(&store)
    );
    assert_eq!(
        local::instructions(&p, &home).unwrap()["files"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
    let cli = local::read(&p, &home, "a472e05c-9bbf-48f2-89d7-6c6092c339ed").unwrap();
    assert!(cli.contains("\"$BIN\" init \"$home\""));
    assert!(cli.contains("source-owned KB"));
    assert!(!cli.contains("PKG="));
    assert!(!cli.contains("/Users/"));
}

#[test]
fn common_questions_describe_local_capabilities_not_hosted_services() {
    let (_, text) = FILES
        .iter()
        .find(|(path, _)| *path == "nodes/a0805f9a-dc3f-417e-81f9-1521c90e1e71/node.md")
        .unwrap();
    for obsolete in [
        "ask SIFT",
        "private to your account",
        "reads and writes through MCP",
        "SIFT still works",
    ] {
        assert!(!text.contains(obsolete), "obsolete FAQ claim: {obsolete}");
    }
    for boundary in [
        "does not upload",
        "not an access-control sandbox",
        "requires no model connection",
        "5000 `o200k_base`",
        "do not establish truth",
    ] {
        assert!(
            text.contains(boundary),
            "missing local boundary: {boundary}"
        );
    }
}

#[test]
fn knowledge_root_introduces_an_independent_local_copy() {
    let (_, text) = FILES
        .iter()
        .find(|(path, _)| *path == "nodes/38a9fc79-8a84-494d-a962-2ec5134bc856/node.md")
        .unwrap();
    for obsolete in [
        "on signup",
        "created your account",
        "ask SIFT",
        "SIFT reads this tree",
    ] {
        assert!(!text.contains(obsolete), "obsolete root claim: {obsolete}");
    }
    for boundary in [
        "independent copy",
        "does not include a SIFT assistant",
        "ordered child reads",
        "not your editable knowledge home",
        "not silently reseeded",
    ] {
        assert!(
            text.contains(boundary),
            "missing local boundary: {boundary}"
        );
    }
}

/// The embedded CLI Operations body must describe what flagless `init` really does.
#[test]
fn default_init_generates_the_file_first_pointer_the_repaired_body_describes() {
    let dir = tempfile::tempdir().unwrap();
    let p = policy();
    let home = dir.path().join("home");
    let created = init(&p, &home).unwrap();
    assert_eq!(created["starter_nodes"], 80);
    assert_eq!(created["pointer"], "AGENTS.md");
    let pointer = home.join("AGENTS.md");
    let pointer_bytes = fs::read(&pointer).unwrap();
    assert!(fs::symlink_metadata(&pointer).unwrap().is_file());
    let text = String::from_utf8(pointer_bytes.clone()).unwrap();
    assert!(text.contains(&format!("nodes/{}/node.md", local::ENTRY)));
    // The pointer names the canonical entry and the entry's own order supplies the six.
    let files = local::instructions(&p, &home).unwrap()["files"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(files.len(), 7);
    for file in &files {
        let path = Path::new(file["path"].as_str().unwrap());
        assert!(path.is_file(), "{}", path.display());
    }
    let from_pointer = std::iter::once(local::ENTRY.to_owned())
        .chain(
            fs::read_to_string(home.join(format!("nodes/{}/children.order", local::ENTRY)))
                .unwrap()
                .lines()
                .map(str::to_owned),
        )
        .map(|id| fs::canonicalize(home.join(format!("nodes/{id}/node.md"))).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        from_pointer,
        files
            .iter()
            .map(|f| fs::canonicalize(f["path"].as_str().unwrap()).unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        local::read(&p, &home, local::ENTRY).unwrap(),
        fs::read_to_string(&from_pointer[0]).unwrap()
    );
    // Untracked local discovery: the home stays clean and reruns never reseed it.
    assert_eq!(
        git(&home, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    assert_eq!(git(&home, &["ls-files", "--", "AGENTS.md"]), "");
    assert_eq!(init(&p, &home).unwrap()["created"], false);
    assert_eq!(fs::read(&pointer).unwrap(), pointer_bytes);
    // A derived worktree carries authored content only, so `instructions` is the lookup.
    let allocation = local::worktree(&p, &home).unwrap();
    let draft = PathBuf::from(allocation["worktree"].as_str().unwrap());
    assert!(
        !draft.join("AGENTS.md").exists(),
        "the untracked home pointer travelled into a private worktree"
    );
    assert_eq!(
        local::instructions(&p, &draft).unwrap()["files"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
    // `--empty` gets no broken pointer.
    let empty = dir.path().join("empty");
    let voided = local::init(&p, &empty, None).unwrap();
    assert_eq!(voided["starter_nodes"], 0);
    assert!(voided["pointer"].is_null());
    assert!(!empty.join("AGENTS.md").exists());
    // The repaired body states the delivered behavior and its honest fallback.
    let body = local::read(&p, &home, "a472e05c-9bbf-48f2-89d7-6c6092c339ed").unwrap();
    assert!(!body.contains("generates no home"));
    assert!(!body.contains("no top-level instruction file"));
    assert!(body.contains("\"pointer\":\"AGENTS.md\""));
    assert!(body.contains("\"pointer\":null"));
    assert!(body.contains(".git/info/exclude"));
    assert!(body.contains("remains the supported lookup"));
}

#[test]
fn independent_defaults_never_reseed_dirty_or_empty_homes() {
    let dir = tempfile::tempdir().unwrap();
    let p = policy();
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    init(&p, &first).unwrap();
    init(&p, &second).unwrap();
    let file = first.join(format!("nodes/{}/node.md", local::ENTRY));
    fs::write(&file, "unfinished authored edit").unwrap();
    fs::write(first.join("untracked"), "keep me").unwrap();
    assert_eq!(init(&p, &first).unwrap()["created"], false);
    assert_eq!(
        fs::read_to_string(file).unwrap(),
        "unfinished authored edit"
    );
    assert_eq!(
        fs::read_to_string(first.join("untracked")).unwrap(),
        "keep me"
    );
    assert_eq!(Store::read(&second).unwrap().nodes.len(), 80);
    let empty = dir.path().join("empty");
    local::init(&p, &empty, None).unwrap();
    assert_eq!(init(&p, &empty).unwrap()["created"], false);
    assert!(Store::read(&empty).unwrap().nodes.is_empty());
}

#[test]
fn default_rejects_unsafe_existing_and_symlinked_targets_without_repair() {
    let dir = tempfile::tempdir().unwrap();
    let p = policy();
    let existing = dir.path().join("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("note"), "keep").unwrap();
    assert!(init(&p, &existing).is_err());
    assert_eq!(fs::read_to_string(existing.join("note")).unwrap(), "keep");
    let linked = dir.path().join("linked");
    symlink(&existing, &linked).unwrap();
    assert!(init(&p, &linked).is_err());
    assert!(init(&p, &linked.join("new")).is_err());
    assert!(!existing.join("new").exists());
    assert!(init(&p, &dir.path().join("missing/new")).is_err());
    assert!(init(&p, &dir.path().join("existing/../escape")).is_err());
    assert!(!dir.path().join("escape").exists());
}
