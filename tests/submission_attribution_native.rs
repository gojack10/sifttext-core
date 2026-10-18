//! Selected attribution through public single-submit, including exact fast-forward reuse.
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}
/// CLI receipts are mid-migration: LEAN by default, JSON where the schema is
/// nested. Decode the dialect actually emitted so a receipt moving format does
/// not silently break these content assertions.
fn receipt(output: &str) -> Value {
    let body = output.trim_start();
    if body.starts_with('{') || body.starts_with('[') {
        serde_json::from_str(body).unwrap()
    } else {
        // This suite's `run` trims the trailing newline; canonical CLI LEAN is
        // newline-terminated, so restore it before the round-trip decode.
        let lean = format!("{}\n", body.trim_end());
        sifttext_file_graph::outlines::decode_lean(&lean).unwrap()
    }
}
struct Native {
    root: PathBuf,
    calls: Vec<Value>,
}
impl Native {
    fn run(&mut self, bin: &str, args: &[&str]) -> String {
        let mut command = Command::new(bin);
        for (key, _) in std::env::vars_os() {
            if key.as_encoded_bytes().starts_with(b"GIT_") {
                command.env_remove(key);
            }
        }
        let out = command
            .args(args)
            .current_dir(&self.root)
            .env("HOME", self.root.join("git-home"))
            .env("XDG_CONFIG_HOME", self.root.join("git-home/.config"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.root.join("gitconfig"))
            .stdin(Stdio::null())
            .output()
            .unwrap();
        self.calls.push(json!({"bin":bin,"args":args,"exit":out.status.code(),
            "stdout":String::from_utf8_lossy(&out.stdout),"stderr":String::from_utf8_lossy(&out.stderr)}));
        fs::write(
            self.root.join("commands.json"),
            serde_json::to_vec_pretty(&self.calls).unwrap(),
        )
        .unwrap();
        assert!(out.status.success(), "{bin} {args:?}: {out:?}");
        String::from_utf8(out.stdout)
            .unwrap()
            .trim_end_matches('\n')
            .into()
    }
    fn cli(&mut self, args: &[&str]) -> Value {
        let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
            .map(|p| PathBuf::from(p).join("sifttext"))
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_sifttext").into());
        receipt(&self.run(text(&bin), args))
    }
    fn git(&mut self, repo: &Path, args: &[&str]) -> String {
        let mut all = vec![
            "-C",
            text(repo),
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgSign=false",
        ];
        all.extend_from_slice(args);
        self.run("git", &all)
    }
    fn authored_commit(&mut self, draft: &Path, name: &str, email: &str, credit: &str) -> String {
        self.git(draft, &["add", "--", "nodes", "trees"]);
        self.git(
            draft,
            &[
                "-c",
                &format!("user.name={name}"),
                "-c",
                &format!("user.email={email}"),
                "commit",
                "-qm",
                &format!("knowledge: selected contribution\n\n{credit}"),
            ],
        );
        self.git(draft, &["rev-parse", "HEAD"])
    }
    fn submit(&mut self, home: &Path, allocation: &Value, commit: &str) -> Value {
        let draft = Path::new(allocation["worktree"].as_str().unwrap());
        let receipt = self.cli(&[
            "submit",
            text(home),
            text(draft),
            commit,
            allocation["base"].as_str().unwrap(),
        ]);
        assert_eq!(receipt["status"], "current", "{receipt:#}");
        let accepted = receipt["candidate"].as_str().unwrap();
        assert_eq!(self.git(home, &["rev-parse", "HEAD"]), accepted);
        assert_eq!(self.git(home, &["rev-parse", "main"]), accepted);
        assert!(!draft.exists(), "successful draft retained");
        receipt
    }
}

#[test]
fn selected_commit_attribution_survives_fast_forward_and_diverged_submission() {
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("SIFTTEXT_ATTRIBUTION_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("evidence"));
    assert!(root.is_absolute());
    fs::create_dir(&root).unwrap();
    std::thread::scope(|scope| {
        for (case, (name, email, credit)) in [
            (
                "Selected Human — 中",
                "human@example.invalid",
                "Co-authored-by: SiftText Agent <agent@sifttext.com>",
            ),
            ("SiftText Agent", "agent@sifttext.com", ""),
            (
                "Selected Human",
                "human@example.invalid",
                "Co-authored-by: SiftText Swarm Agent deadbeef <agent@sifttext.com>",
            ),
            ("SiftText Swarm Agent deadbeef", "agent@sifttext.com", ""),
        ]
        .into_iter()
        .enumerate()
        {
            let root = root.join(format!("case-{case}"));
            scope.spawn(move || {
                fs::create_dir(&root).unwrap();
                fs::create_dir(root.join("git-home")).unwrap();
                let config =
                    b"[user]\n name = Synthetic Initial User\n email = initial@example.invalid\n";
                fs::write(root.join("gitconfig"), config).unwrap();
                let mut native = Native {
                    root: root.clone(),
                    calls: Vec::new(),
                };
                let home = root.join(format!("knowledge-{case}"));
                native.cli(&["init", text(&home), "--empty"]);
                let first = native.cli(&["worktree", text(&home)]);
                let second = native.cli(&["worktree", text(&home)]);
                let a = Path::new(first["worktree"].as_str().unwrap());
                let b = Path::new(second["worktree"].as_str().unwrap());
                native.cli(&[
                    "create",
                    text(a),
                    "First root",
                    "Owns the first disjoint attribution specimen.",
                ]);
                native.cli(&[
                    "create",
                    text(b),
                    "Second root",
                    "Owns the second disjoint attribution specimen.",
                ]);
                let first_commit = native.authored_commit(a, name, email, credit);
                let second_commit = native.authored_commit(b, name, email, credit);
                let format = "--format=%an%x00%ae%x00%(trailers:key=Co-authored-by)";
                let first_identity = native.git(&home, &["log", "-1", format, &first_commit]);
                let second_identity = native.git(&home, &["log", "-1", format, &second_commit]);
                let first_receipt = native.submit(&home, &first, &first_commit);
                assert_eq!(
                    first_receipt["candidate"], first_commit,
                    "fast-forward must reuse source commit"
                );
                assert!(b.exists(), "other worker draft removed");
                assert_eq!(native.git(b, &["rev-parse", "HEAD"]), second_commit);
                let second_receipt = native.submit(&home, &second, &second_commit);
                let merged = second_receipt["candidate"].as_str().unwrap();
                assert_ne!(
                    merged, second_commit,
                    "divergence requires an integration commit"
                );
                let parents = native.git(&home, &["log", "-1", "--format=%P", merged]);
                let mut parents: Vec<_> = parents.split_whitespace().collect();
                parents.sort();
                let mut expected = vec![first_commit.as_str(), second_commit.as_str()];
                expected.sort();
                assert_eq!(
                    parents, expected,
                    "integration preserves both submitted histories"
                );
                assert_eq!(
                    native.git(&home, &["log", "-1", format, merged]),
                    second_identity
                );
                assert_eq!(
                    native.git(&home, &["log", "-1", "--format=%cn%x00%ce", merged]),
                    format!("{name}\0{email}")
                );
                assert_eq!(
                    native.git(&home, &["log", "-1", format, &first_commit]),
                    first_identity
                );
                assert_eq!(
                    native.git(&home, &["log", "-1", format, &second_commit]),
                    second_identity
                );
                assert_eq!(
                    fs::read(root.join("gitconfig")).unwrap(),
                    config,
                    "global config changed"
                );
            });
        }
    });
}
