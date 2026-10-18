//! Native CLI parity for the legacy home_agent_pointer.mjs sequence.
//! Build checked assets first, then cargo test --locked --test home_pointer_native.
//! SIFTTEXT_TEST_EVIDENCE names a NEW directory for retained command receipts.
//! SIFTTEXT_POINTER_BUNDLE optionally checks a supplied canonical native bundle.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::{fs::symlink, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::Command,
};

const ENTRY: &str = "5e565245-14fe-406c-977c-cf94a9be246b";
const ROOT: &str = "10000000-0000-4000-8000-000000000001";
const CHILDREN: [&str; 6] = [
    "b6107909-a33b-4c63-ab0d-351ba1a1b672",
    "f961a20f-1df5-4c0f-a263-9730e2a28aca",
    "0278c212-d51f-46db-8bd6-b537593ef186",
    "4cc0b396-4b55-415e-bb26-b51c97e6ca78",
    "fb1e0c9f-8c04-4baf-91e6-4688856a14af",
    "a472e05c-9bbf-48f2-89d7-6c6092c339ed",
];
const IGNORE: &str = "/.sifttext/\n/nodes/*/node.meta.json\n/nodes/*/outline*.lean\n/nodes/*/outline*.lean.meta.json\n";
fn text(p: &Path) -> &str {
    p.to_str().unwrap()
}
fn node(p: &Path, id: &str, children: &[&str]) {
    let at = p.join("nodes").join(id);
    fs::create_dir_all(at.join("children")).unwrap();
    fs::write(at.join("node.md"), format!("---\nid: {id:?}\nname: \"Synthetic instruction\"\nmicro_summary: \"Keeps the pointer trial reversible.\"\nstatus: \"not_examined\"\nsee_also: []\n---\nAuthored bytes — café 中.\n")).unwrap();
    fs::write(
        at.join("children.order"),
        children
            .iter()
            .map(|id| format!("{id}\n"))
            .collect::<String>(),
    )
    .unwrap();
    if children.is_empty() {
        fs::write(at.join("children/.keep"), "").unwrap();
    }
    for child in children {
        symlink(format!("../../{child}"), at.join("children").join(child)).unwrap();
    }
}
fn bundle(p: &Path, full: bool) {
    fs::create_dir_all(p.join("trees")).unwrap();
    let children = if full { &CHILDREN[..] } else { &CHILDREN[..1] };
    node(p, ROOT, &[ENTRY]);
    node(p, ENTRY, children);
    for child in children {
        node(p, child, &[]);
    }
    symlink(format!("../nodes/{ROOT}"), p.join("trees").join(ROOT)).unwrap();
}
// Independent authored bytes/link inventory: exclude only known derived files.
fn authored(p: &Path) -> BTreeMap<PathBuf, Value> {
    fn walk(root: &Path, relative: &Path, out: &mut BTreeMap<PathBuf, Value>) {
        let at = root.join(relative);
        let meta = fs::symlink_metadata(&at).unwrap();
        if meta.is_symlink() {
            out.insert(relative.into(), json!({"link":fs::read_link(at).unwrap()}));
        } else if meta.is_dir() {
            for entry in fs::read_dir(at).unwrap() {
                let entry = entry.unwrap();
                let name = entry.file_name();
                let name = name.to_str().unwrap();
                if name == "node.meta.json" || name.starts_with("outline") {
                    continue;
                }
                walk(root, &relative.join(name), out);
            }
        } else {
            assert!(meta.is_file());
            out.insert(relative.into(), json!({"bytes":fs::read(at).unwrap()}));
        }
    }
    let mut out = BTreeMap::new();
    for name in ["nodes", "trees"] {
        walk(p, Path::new(name), &mut out);
    }
    out
}
struct Native {
    evidence: PathBuf,
    git_home: PathBuf,
    records: Vec<Value>,
}
impl Native {
    fn run(&mut self, command: &str, args: &[&str], ok: bool) -> String {
        let mut child = Command::new(command);
        for (key, _) in std::env::vars_os() {
            if key.as_encoded_bytes().starts_with(b"GIT_") {
                child.env_remove(key);
            }
        }
        let out = child
            .args(args)
            .current_dir(&self.evidence)
            .env("HOME", &self.git_home)
            .env("XDG_CONFIG_HOME", self.git_home.join(".config"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8(out.stdout).unwrap();
        let stderr = String::from_utf8(out.stderr).unwrap();
        self.records.push(json!({"command":command,"args":args,"exit":out.status.code(),"signal":out.status.signal(),"stdout":stdout,"stderr":stderr}));
        fs::write(
            self.evidence.join("commands.json"),
            serde_json::to_vec_pretty(&self.records).unwrap(),
        )
        .unwrap();
        assert_eq!(out.status.signal(), None, "{command} {args:?}: {stderr}");
        assert_eq!(
            out.status.success(),
            ok,
            "{command} {args:?}: {stdout}\n{stderr}"
        );
        stdout
    }
    fn cli(&mut self, args: &[&str]) -> Value {
        let output = self.run(env!("CARGO_BIN_EXE_sifttext"), args, true);
        if args.first().is_some_and(|cmd| {
            !matches!(
                *cmd,
                "sql" | "search" | "submit" | "accepted" | "session-capture" | "session-attach" | "session-backup" | "session-restore"
            )
        }) {
            sifttext_file_graph::outlines::decode_lean(&output).unwrap()
        } else {
            serde_json::from_str(&output).unwrap()
        }
    }
    fn refuse(&mut self, args: &[&str]) {
        self.run(env!("CARGO_BIN_EXE_sifttext"), args, false);
    }
    fn git(&mut self, p: &Path, args: &[&str]) -> String {
        let mut all = vec![
            "-C",
            text(p),
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgSign=false",
        ];
        all.extend_from_slice(args);
        self.run("git", &all, true).trim().into()
    }
}
#[test]
fn pointer_cli_preserves_authored_knowledge_through_publication_and_refusals() {
    let tmp = tempfile::tempdir().unwrap();
    let evidence = match std::env::var_os("SIFTTEXT_TEST_EVIDENCE") {
        Some(path) => {
            let path = PathBuf::from(path);
            assert!(path.is_absolute());
            fs::create_dir(&path).unwrap();
            path.canonicalize().unwrap()
        }
        None => tmp.path().canonicalize().unwrap(),
    };
    let git_home = evidence.join("git-home");
    fs::create_dir(&git_home).unwrap();
    fs::write(git_home.join(".gitconfig"), "[user]\nname = Synthetic Fixture\nemail = fixture@example.invalid\n[commit]\ngpgSign = false\n[core]\nhooksPath = /dev/null\n").unwrap();
    let mut native = Native {
        evidence: evidence.clone(),
        git_home,
        records: vec![],
    };
    let source = match std::env::var_os("SIFTTEXT_POINTER_BUNDLE") {
        Some(path) => PathBuf::from(path).canonicalize().unwrap(),
        None => {
            let path = evidence.join("bundle");
            bundle(&path, true);
            path
        }
    };
    let original = authored(&source);
    let node_count = original.keys().filter(|p| p.ends_with("node.md")).count();
    let tree_root = fs::read_dir(source.join("trees"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name();
    let tree_root = tree_root.to_str().unwrap();
    let home = evidence.join("home");
    let initial = native.cli(&["init", text(&home), "--bundle", text(&source)]);
    assert_eq!(initial["created"], true);
    assert_eq!(initial["pointer"], "AGENTS.md");
    let at = home.join("AGENTS.md");
    let meta = fs::symlink_metadata(&at).unwrap();
    assert!(meta.is_file() && !meta.is_symlink());
    let pointer = fs::read_to_string(&at).unwrap();
    assert!(pointer.contains("Follow only task-relevant instructions from the entry file above."));
    assert!(!pointer.contains("Then follow those nodes."));
    let re = regex::Regex::new(r"nodes/[0-9a-f-]{36}/[a-z.]+").unwrap();
    let mut named = Vec::new();
    for m in re.find_iter(&pointer) {
        if !named.contains(&m.as_str()) {
            named.push(m.as_str());
        }
    }
    let entry_file = format!("nodes/{ENTRY}/node.md");
    let order_file = format!("nodes/{ENTRY}/children.order");
    assert_eq!(named, [entry_file.as_str(), order_file.as_str()]);
    for relative in named {
        assert!(!relative.contains(".."));
        let resolved = home.join(relative).canonicalize().unwrap();
        assert!(resolved.starts_with(home.canonicalize().unwrap()));
        assert!(fs::symlink_metadata(resolved).unwrap().is_file());
    }
    assert_eq!(
        fs::read_to_string(home.join(&order_file))
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        CHILDREN
    );
    assert_eq!(authored(&home), original);
    let pipeline = native.cli(&["instructions", text(&home)]);
    let ids: Vec<&str> = std::iter::once(ENTRY).chain(CHILDREN).collect();
    assert_eq!(
        pipeline["path"].as_str().unwrap(),
        text(&home.join("nodes").join(ENTRY).join("node.md"))
    );
    assert_eq!(ids.first(), Some(&ENTRY));
    for banned in [
        "://",
        "sifttext-mcp",
        env!("CARGO_MANIFEST_DIR"),
        text(&evidence),
    ] {
        assert!(!pointer.contains(banned));
    }
    assert_eq!(
        native.git(&home, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    assert!(
        !native
            .git(&home, &["ls-files"])
            .lines()
            .any(|p| p == "AGENTS.md")
    );
    assert!(
        fs::read_to_string(home.join(".git/info/exclude"))
            .unwrap()
            .contains("/AGENTS.md\n")
    );
    assert_eq!(fs::read_to_string(home.join(".gitignore")).unwrap(), IGNORE);
    assert_eq!(
        native.git(&home, &["rev-parse", "HEAD"]),
        initial["commit"].as_str().unwrap()
    );

    let user = "a real unfinished AGENTS.md edit — café\n";
    fs::write(&at, user).unwrap();
    let authored_at = home.join(&entry_file);
    let saved = fs::read(&authored_at).unwrap();
    fs::write(&authored_at, "a real unfinished authored edit").unwrap();
    for args in [
        vec!["init", text(&home), "--bundle", text(&source)],
        vec!["init", text(&home), "--empty"],
    ] {
        assert_eq!(native.cli(&args)["created"], false);
    }
    assert_eq!(fs::read_to_string(&at).unwrap(), user);
    assert_eq!(
        fs::read_to_string(&authored_at).unwrap(),
        "a real unfinished authored edit"
    );
    assert_eq!(
        native.git(&home, &["rev-parse", "HEAD"]),
        initial["commit"].as_str().unwrap()
    );
    fs::remove_file(&at).unwrap();
    let outside = evidence.join("outside-private");
    fs::write(&outside, "private user file").unwrap();
    symlink(&outside, &at).unwrap();
    assert_eq!(
        native.cli(&["init", text(&home), "--bundle", text(&source)])["created"],
        false
    );
    assert!(fs::symlink_metadata(&at).unwrap().is_symlink());
    assert_eq!(fs::read_to_string(&outside).unwrap(), "private user file");
    fs::remove_file(&at).unwrap();
    fs::write(&at, &pointer).unwrap();
    fs::write(&authored_at, &saved).unwrap();

    let allocation = native.cli(&["worktree", text(&home)]);
    let draft = PathBuf::from(allocation["worktree"].as_str().unwrap());
    assert!(!draft.join("AGENTS.md").exists());
    let draft_pipeline = native.cli(&["instructions", text(&draft)]);
    assert_eq!(
        draft_pipeline["path"].as_str().unwrap(),
        text(&draft.join("nodes").join(ENTRY).join("node.md")),
        "worktree instruction lookup must use its own authored entry"
    );
    let created = native.cli(&[
        "create",
        text(&draft),
        "Pointer publication trial",
        "Checks a reversible publication.",
        tree_root,
    ]);
    assert_eq!(
        fs::read_to_string(home.join(".git/info/exclude"))
            .unwrap()
            .lines()
            .filter(|l| *l == "/AGENTS.md")
            .count(),
        1
    );
    native.git(&draft, &["add", "--", "nodes", "trees"]);
    native.git(&draft, &["commit", "-m", "knowledge: create pointer trial"]);
    let commit = native.git(&draft, &["rev-parse", "HEAD"]);
    let committed_authored = authored(&draft);
    let created_bytes = fs::read(created["path"].as_str().unwrap()).unwrap();
    let receipt = native.cli(&[
        "submit",
        text(&home),
        text(&draft),
        &commit,
        allocation["base"].as_str().unwrap(),
    ]);
    assert_eq!(receipt["status"], "current", "{receipt}");
    assert_eq!(receipt["published"], true);
    assert_eq!(
        native.git(&home, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    assert!(fs::symlink_metadata(&at).unwrap().is_file());
    let view = PathBuf::from(receipt["view"].as_str().unwrap());
    assert_eq!(view, home);
    assert_eq!(receipt["home"], text(&home));
    assert_eq!(receipt["worktree_removed"], true);
    assert!(!draft.exists());
    assert_eq!(fs::read_to_string(view.join("AGENTS.md")).unwrap(), pointer);
    assert_eq!(native.cli(&["index", text(&view)])["nodes"], node_count + 1);
    assert_eq!(fs::read(&authored_at).unwrap(), saved);
    assert_eq!(
        authored(&view),
        committed_authored,
        "published authored bytes/links differ from committed draft"
    );
    let created_id = created["id"].as_str().unwrap();
    assert_eq!(
        fs::read(view.join("nodes").join(created_id).join("node.md")).unwrap(),
        created_bytes
    );
    assert!(
        fs::read_to_string(view.join(format!("nodes/{tree_root}/children.order")))
            .unwrap()
            .lines()
            .any(|id| id == created_id)
    );
    assert!(
        !fs::read_to_string(view.join(&order_file))
            .unwrap()
            .lines()
            .any(|id| id == created_id)
    );
    let advanced = native.cli(&["advance", text(&home)]);
    assert_eq!(advanced["status"], "current");
    assert_eq!(advanced["advanced"], false);
    assert_eq!(advanced["reader"], receipt["candidate"]);
    assert_eq!(advanced["reader_before"], receipt["candidate"]);
    assert_eq!(
        native.git(&home, &["rev-parse", "HEAD"]),
        receipt["candidate"].as_str().unwrap()
    );
    let meta = fs::symlink_metadata(&at).unwrap();
    assert!(meta.is_file() && !meta.is_symlink());
    assert_eq!(fs::read_to_string(&at).unwrap(), pointer);
    assert_eq!(
        native.git(&home, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    assert_eq!(authored(&home), authored(&view));
    assert_eq!(native.cli(&["advance", text(&home)])["advanced"], false);
    let entry_path = native.cli(&["instructions", text(&home)])["path"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        entry_path.starts_with(text(&home))
            && entry_path.ends_with(&format!("nodes/{ENTRY}/node.md")),
        "advance must keep the canonical entry readable: {entry_path}"
    );

    let empty = evidence.join("empty");
    let result = native.cli(&["init", text(&empty), "--empty"]);
    assert_eq!(result["created"], true);
    assert_eq!(result["pointer"], Value::Null);
    assert!(!empty.join("AGENTS.md").exists());
    assert_eq!(
        native.git(&empty, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    native.refuse(&["instructions", text(&empty)]);
    let partial = evidence.join("partial-bundle");
    bundle(&partial, false);
    let custom = evidence.join("custom");
    let result = native.cli(&["init", text(&custom), "--bundle", text(&partial)]);
    assert_eq!(result["created"], true);
    assert_eq!(result["pointer"], Value::Null);
    assert!(!custom.join("AGENTS.md").exists());
    assert!(match fs::read_to_string(custom.join(".git/info/exclude")) {
        Ok(exclude) => !exclude.contains("/AGENTS.md"),
        Err(err) => err.kind() == std::io::ErrorKind::NotFound,
    });
    native.refuse(&["instructions", text(&custom)]);
    assert_eq!(
        native.git(&custom, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    let occupied = evidence.join("occupied");
    fs::create_dir_all(occupied.join("nodes")).unwrap();
    fs::write(occupied.join("AGENTS.md"), user).unwrap();
    native.refuse(&["init", text(&occupied), "--bundle", text(&source)]);
    assert_eq!(
        fs::read_to_string(occupied.join("AGENTS.md")).unwrap(),
        user
    );
    let linked = evidence.join("linked-home");
    symlink(&home, &linked).unwrap();
    native.refuse(&["init", text(&linked), "--bundle", text(&source)]);
    assert_eq!(fs::read_to_string(&at).unwrap(), pointer);
    let broken = evidence.join("broken-bundle");
    bundle(&broken, true);
    let broken_entry = broken.join(&entry_file);
    let content = fs::read_to_string(&broken_entry).unwrap();
    fs::write(
        &broken_entry,
        content.replace(
            "micro_summary: \"Keeps the pointer trial reversible.\"",
            "micro_summary: \"   \"",
        ),
    )
    .unwrap();
    let never = evidence.join("never-created");
    native.refuse(&["init", text(&never), "--bundle", text(&broken)]);
    assert!(!never.exists());
    assert_eq!(authored(&source), original, "input bundle mutated");
    fs::write(evidence.join("result.json"), serde_json::to_vec_pretty(&json!({"status":"passed", "commands":native.records.len(), "initial":initial,"pipeline":pipeline,"allocation":allocation,"created":created,"receipt":receipt,"advanced":advanced,"scope":"native CLI pointer parity; no Linux, aggregate or fresh-agent claim"})).unwrap()).unwrap();
}
