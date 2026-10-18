//! Rust replacement for tests/successor/file_first_kb.mjs. Build checked assets once
//! with scripts/build.sh, then run `cargo test --locked --test file_first_kb`.
//! Set SIFTTEXT_TEST_EVIDENCE to a NEW directory to retain command receipts.
//! No nested builds, hosted knowledge, or shared-checkout mutations.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::{fs::symlink, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::Command,
};

const ENTRY: &str = "5e565245-14fe-406c-977c-cf94a9be246b";
const ROOT: &str = "38a9fc79-8a84-494d-a962-2ec5134bc856";

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn inventory(root: &Path) -> BTreeMap<PathBuf, Value> {
    fn walk(root: &Path, relative: &Path, out: &mut BTreeMap<PathBuf, Value>) {
        let path = root.join(relative);
        let meta = fs::symlink_metadata(&path).unwrap();
        let value = if meta.is_symlink() {
            json!({"link": fs::read_link(&path).unwrap()})
        } else if meta.is_dir() {
            for entry in fs::read_dir(&path).unwrap() {
                walk(root, &relative.join(entry.unwrap().file_name()), out);
            }
            json!({"dir": true})
        } else {
            assert!(meta.is_file(), "unsupported inventory entry: {path:?}");
            json!({"bytes": fs::read(&path).unwrap()})
        };
        out.insert(relative.into(), value);
    }
    let mut result = BTreeMap::new();
    for name in ["nodes", "trees"] {
        walk(root, Path::new(name), &mut result);
    }
    result
}

struct Commands {
    cwd: PathBuf,
    git_home: PathBuf,
    evidence: PathBuf,
    records: Vec<Value>,
}

impl Commands {
    fn run(&mut self, command: &str, args: &[&str], ok: bool) -> String {
        let mut child = Command::new(command);
        for (key, _) in std::env::vars_os() {
            if key.as_encoded_bytes().starts_with(b"GIT_") {
                child.env_remove(key);
            }
        }
        // Core strips GIT_* before init's identity probe, so configure a private
        // HOME instead of relying on author env vars or the user's real config.
        let output = child
            .args(args)
            .current_dir(&self.cwd)
            .env("HOME", &self.git_home)
            .env("XDG_CONFIG_HOME", self.git_home.join(".config"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        self.records.push(json!({
            "cmd": command, "args": args, "status": output.status.code(),
            "signal": output.status.signal(), "stdout": stdout, "stderr": stderr,
        }));
        fs::write(
            self.evidence.join("commands.json"),
            serde_json::to_vec_pretty(&self.records).unwrap(),
        )
        .unwrap();
        assert_eq!(output.status.signal(), None, "command terminated by signal");
        assert_eq!(
            output.status.success(),
            ok,
            "{command} {args:?}: {stdout}\n{stderr}"
        );
        stdout
    }

    fn git(&mut self, home: &Path, args: &[&str]) -> String {
        let mut all = vec!["-C", text(home)];
        all.extend_from_slice(args);
        self.run("git", &all, true).trim().into()
    }
}

#[test]
fn file_first_install_preserves_native_knowledge_and_refuses_unsafe_targets() {
    let temporary = tempfile::tempdir().unwrap();
    let evidence = match std::env::var_os("SIFTTEXT_TEST_EVIDENCE") {
        Some(path) => {
            let path = PathBuf::from(path);
            assert!(path.is_absolute());
            fs::create_dir(&path).unwrap(); // Refuse reuse of another run's evidence.
            fs::canonicalize(path).unwrap()
        }
        None => fs::canonicalize(temporary.path()).unwrap(),
    };
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = evidence.join("source");
    fs::create_dir(&source).unwrap();
    let git_home = evidence.join("git-home");
    fs::create_dir(&git_home).unwrap();
    fs::write(
        git_home.join(".gitconfig"),
        "[user]\nname = Synthetic Fixture\nemail = fixture@example.invalid\n[commit]\ngpgSign = false\n[core]\nhooksPath = /dev/null\n",
    )
    .unwrap();
    let mut commands = Commands {
        cwd: checkout.into(),
        git_home,
        evidence: evidence.clone(),
        records: vec![],
    };
    // A disposable checked source copy lets the independence control hide nodes
    // without renaming a live checkout's source or racing another test/build.
    let mut copied = vec![
        "-R",
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "install-local.sh",
        "README.md",
        "AGENTS.md",
        ".gitignore",
        "rust-toolchain.toml",
        "package.json",
        "integrations",
        "src",
        "laws",
        "scripts",
        "tests",
        "fixtures",
        "nodes",
        "trees",
        "build",
    ];
    if checkout.join("docs").is_dir() {
        copied.push("docs");
    }
    copied.push(text(&source));
    commands.run("cp", &copied, true);
    fs::create_dir_all(source.join("target/debug")).unwrap();
    // runtime() canonicalizes the executable: a copied build binary keeps its
    // original compile-time checkout, so resolve this fixture's CLI at its
    // packaged asset root, not at the copied target/debug directory.
    fs::copy(env!("CARGO_BIN_EXE_sifttext"), source.join("sifttext")).unwrap();
    symlink("../../sifttext", source.join("target/debug/sifttext")).unwrap();
    assert_eq!(
        fs::canonicalize(source.join("target/debug/sifttext"))
            .unwrap()
            .parent()
            .unwrap(),
        source
    );
    commands.cwd = source.clone();

    let authored = inventory(&source);
    let source_nodes = authored
        .keys()
        .filter(|path| {
            path.starts_with("nodes") && path.file_name().is_some_and(|name| name == "node.md")
        })
        .count();
    let pointer = source.join("AGENTS.md");
    let meta = fs::symlink_metadata(&pointer).unwrap();
    assert!(meta.is_file() && !meta.is_symlink());
    assert!(
        fs::read_to_string(pointer)
            .unwrap()
            .contains(&format!("nodes/{ENTRY}/node.md"))
    );
    let children: Vec<String> =
        fs::read_to_string(source.join(format!("nodes/{ENTRY}/children.order")))
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
    assert_eq!(children.len(), 6);
    let ordered: Vec<&str> = std::iter::once(ENTRY)
        .chain(children.iter().map(String::as_str))
        .collect();
    for id in &ordered {
        assert!(
            !fs::read(source.join(format!("nodes/{id}/node.md")))
                .unwrap()
                .is_empty()
        );
    }
    assert!(
        fs::read_to_string(source.join("README.md"))
            .unwrap()
            .contains("[AGENTS.md](AGENTS.md)")
    );

    let prefix = evidence.join("application");
    let home = commands.git_home.join("sifttext");
    // Native init must refuse an uninitialized existing home without leaving
    // an application prefix that prevents retry, or touching user knowledge.
    let refused_home = evidence.join("uninitialized-knowledge");
    fs::create_dir(&refused_home).unwrap();
    fs::write(refused_home.join("private-note"), "retain user knowledge").unwrap();
    fs::write(refused_home.join(".gitignore"), "user-owned ignore rules").unwrap();
    commands.run(
        "sh",
        &["install-local.sh", text(&prefix), text(&refused_home)],
        false,
    );
    assert!(
        commands.records.last().unwrap()["stderr"]
            .as_str()
            .unwrap()
            .contains("rejected: not an initialized local knowledge home"),
        "installer did not reach native init refusal"
    );
    assert!(
        !prefix.exists(),
        "native init refusal left a partial application"
    );
    assert_eq!(fs::read_dir(&refused_home).unwrap().count(), 2);
    assert_eq!(
        fs::read_to_string(refused_home.join(".gitignore")).unwrap(),
        "user-owned ignore rules"
    );
    assert_eq!(
        fs::read_to_string(refused_home.join("private-note")).unwrap(),
        "retain user knowledge"
    );
    // Reuse exactly the same application destination after that refusal.
    let stdout = commands.run(
        "sh",
        &["install-local.sh", text(&prefix), text(&home)],
        true,
    );
    assert!(stdout.contains(&format!("Read \"{}/AGENTS.md\"", home.display())));
    assert!(stdout.contains(&format!("use \"{}/sifttext\"", prefix.display())));
    let operations =
        fs::read_to_string(home.join("nodes/a472e05c-9bbf-48f2-89d7-6c6092c339ed/node.md"))
            .unwrap();
    assert!(
        operations.contains("For an installed application")
            && operations.contains("without rebuilding or cloning software")
    );
    assert_eq!(inventory(&prefix), authored);
    assert_eq!(inventory(&home), authored);
    assert!(!home.join("src").exists() && !home.join("Cargo.toml").exists());
    assert_eq!(
        commands.git(&home, &["rev-parse", "--show-toplevel"]),
        text(&home)
    );
    assert_eq!(
        commands.git(&home, &["log", "-1", "--format=%an <%ae>|%cn <%ce>"]),
        "Synthetic Fixture <fixture@example.invalid>|Synthetic Fixture <fixture@example.invalid>"
    );
    assert_eq!(commands.git(&home, &["remote"]), "");
    assert_eq!(commands.git(&home, &["status", "--porcelain"]), "");
    assert_eq!(commands.git(&home, &["ls-files", "--", "AGENTS.md"]), "");

    let bin = prefix.join("sifttext");
    let independent = evidence.join("independent");
    let hidden = evidence.join("hidden-source-nodes");
    fs::rename(source.join("nodes"), &hidden).unwrap();
    let result = sifttext_file_graph::outlines::decode_lean(&commands.run(
        text(&bin),
        &["init", text(&independent)],
        true,
    ))
    .unwrap();
    assert_eq!(result["starter_nodes"], source_nodes);
    assert_eq!(inventory(&independent), authored);
    fs::rename(&hidden, source.join("nodes")).unwrap();
    let instructions = sifttext_file_graph::outlines::decode_lean(&commands.run(
        text(&bin),
        &["instructions", text(&home)],
        true,
    ))
    .unwrap();
    assert_eq!(
        instructions["path"].as_str().unwrap(),
        text(&home.join(format!("nodes/{ENTRY}/node.md")))
    );
    assert_eq!(
        ordered.first(),
        Some(&ENTRY),
        "the single returned entry precedes its conditional children"
    );
    assert_eq!(
        sifttext_file_graph::outlines::decode_lean(&commands.run(
            text(&bin),
            &["instructions"],
            true
        ))
        .unwrap(),
        instructions,
        "omitting STORE should select $HOME/sifttext"
    );

    let file = home.join(format!("nodes/{ENTRY}/node.md"));
    let home_pointer = home.join("AGENTS.md");
    fs::write(&file, "unfinished user edit").unwrap();
    fs::write(&home_pointer, "user pointer edit").unwrap();
    fs::write(home.join("private-note"), "retain").unwrap();
    let second = evidence.join("second-application");
    let preserved = commands.run(
        "sh",
        &["install-local.sh", text(&second), text(&home)],
        true,
    );
    assert!(preserved.contains(&format!("Read \"{}/AGENTS.md\"", home.display())));
    assert_eq!(fs::read_to_string(&file).unwrap(), "unfinished user edit");
    assert_eq!(
        fs::read_to_string(&home_pointer).unwrap(),
        "user pointer edit"
    );
    assert_eq!(
        fs::read_to_string(home.join("private-note")).unwrap(),
        "retain"
    );
    commands.run(
        "sh",
        &["install-local.sh", text(&prefix), text(&home)],
        false,
    );
    assert_eq!(inventory(&prefix), authored);
    let nested = evidence.join("nested");
    commands.run(
        "sh",
        &["install-local.sh", text(&nested), text(&nested)],
        false,
    );
    assert!(!nested.exists());
    let home_app = home.join("app");
    commands.run(
        "sh",
        &["install-local.sh", text(&home_app), text(&home)],
        false,
    );
    assert!(!home_app.exists());
    let alias = evidence.join("alias");
    symlink(&prefix, &alias).unwrap();
    commands.run(
        "sh",
        &["install-local.sh", text(&alias), text(&home)],
        false,
    );
    assert_eq!(fs::read_link(&alias).unwrap(), prefix);

    let asset = prefix.join(format!("nodes/{ENTRY}/node.md"));
    let saved = fs::read(&asset).unwrap();
    fs::write(&asset, "changed official asset").unwrap();
    let not_created = evidence.join("not-created");
    commands.run(text(&bin), &["init", text(&not_created)], false);
    assert!(!not_created.exists());
    fs::write(&asset, saved).unwrap();
    let link = prefix.join("trees").join(ROOT);
    fs::remove_file(&link).unwrap();
    symlink(format!("../nodes/{ENTRY}"), &link).unwrap();
    commands.run(text(&bin), &["instructions", text(&home)], false);
    fs::remove_file(&link).unwrap();
    symlink(format!("../nodes/{ROOT}"), &link).unwrap();
    commands.run(
        text(&bin),
        &["init", text(&evidence.join("empty")), "--empty"],
        true,
    );
    let alternate = evidence.join("alternate");
    commands.run(
        text(&bin),
        &["init", text(&alternate), "--bundle", text(&independent)],
        true,
    );
    assert_eq!(inventory(&alternate), authored);
    assert_eq!(inventory(&source), authored);
    fs::write(evidence.join("result.json"), serde_json::to_vec_pretty(&json!({
        "status": "passed", "source": source, "prefix": prefix, "home": home,
        "commands": commands.records.len(),
        "scope": "Readable source/native compile/local install parity and preserving init; no public release, complete-KB, timing or fresh-agent proof."
    })).unwrap()).unwrap();
}
