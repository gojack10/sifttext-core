//! One-submit CLI contract: normal-home agreement, custody, cleanup and repair.
//! Build checked assets first. SIFTTEXT_TEST_EVIDENCE retains actual commands.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn text(p: &Path) -> &str {
    p.to_str().unwrap()
}
/// CLI receipts are mid-migration: LEAN by default, JSON where the schema is
/// nested. Decode the dialect actually emitted so a receipt moving format does
/// not silently break these content assertions.
fn receipt(output: &str) -> Value {
    let body = output.trim_start();
    if body.starts_with('{') || body.starts_with('[') {
        serde_json::from_str(body).unwrap()
    } else {
        sifttext_file_graph::outlines::decode_lean(body).unwrap()
    }
}
fn snapshot(p: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, rel: PathBuf, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let path = root.join(&rel);
        let meta = fs::symlink_metadata(&path).unwrap();
        if meta.is_symlink() {
            out.insert(
                rel,
                fs::read_link(path)
                    .unwrap()
                    .as_os_str()
                    .as_encoded_bytes()
                    .to_vec(),
            );
        } else if meta.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                let name = entry.file_name();
                let name = name.to_str().unwrap();
                if name == "node.meta.json" || name.starts_with("outline") {
                    continue;
                }
                walk(root, rel.join(name), out);
            }
        } else {
            out.insert(rel, fs::read(path).unwrap());
        }
    }
    let mut out = BTreeMap::new();
    for name in ["nodes", "trees"] {
        walk(p, name.into(), &mut out);
    }
    out
}
struct Native {
    evidence: PathBuf,
    config: PathBuf,
    records: Vec<Value>,
}
impl Native {
    fn run(&mut self, bin: &str, args: &[&str], ok: bool) -> String {
        let mut cmd = Command::new(bin);
        for (key, _) in std::env::vars_os() {
            if key.as_encoded_bytes().starts_with(b"GIT_") {
                cmd.env_remove(key);
            }
        }
        let out = cmd
            .args(args)
            .current_dir(&self.evidence)
            .env("HOME", &self.config)
            .env("XDG_CONFIG_HOME", self.config.join(".config"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        self.records.push(json!({"bin":bin,"args":args,"exit":out.status.code(),"stdout":String::from_utf8_lossy(&out.stdout),"stderr":String::from_utf8_lossy(&out.stderr)}));
        fs::write(
            self.evidence.join("commands.json"),
            serde_json::to_vec_pretty(&self.records).unwrap(),
        )
        .unwrap();
        assert_eq!(out.status.success(), ok, "{bin} {args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    }
    fn cli(&mut self, args: &[&str]) -> Value {
        receipt(&self.run(env!("CARGO_BIN_EXE_sifttext"), args, true))
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
    fn commit(&mut self, p: &Path) -> String {
        self.git(p, &["add", "--", "nodes", "trees"]);
        self.git(p, &["commit", "-m", "knowledge: synthetic save"]);
        self.git(p, &["rev-parse", "HEAD"])
    }
    fn submit(&mut self, home: &Path, a: &Value, sha: &str, ok: bool) -> Value {
        let args = [
            "submit",
            text(home),
            a["worktree"].as_str().unwrap(),
            sha,
            a["base"].as_str().unwrap(),
        ];
        serde_json::from_str(&self.run(env!("CARGO_BIN_EXE_sifttext"), &args, ok)).unwrap()
    }
    fn finished(
        &mut self,
        home: &Path,
        a: &Value,
        receipt: &Value,
        expected: &BTreeMap<PathBuf, Vec<u8>>,
    ) {
        assert_eq!(receipt["status"], "current");
        assert_eq!(receipt["view"], text(home));
        assert_eq!(receipt["worktree_removed"], true);
        let target = receipt["candidate"].as_str().unwrap();
        assert_eq!(self.git(home, &["rev-parse", "HEAD"]), target);
        assert_eq!(self.git(home, &["rev-parse", "refs/heads/main"]), target);
        assert_eq!(snapshot(home), *expected);
        assert_eq!(self.git(home, &["status", "--porcelain"]), "");
        assert!(!Path::new(a["worktree"].as_str().unwrap()).exists());
        assert_eq!(
            self.git(
                home,
                &[
                    "for-each-ref",
                    "--format=%(refname)",
                    &format!("refs/heads/{}", a["branch"].as_str().unwrap())
                ]
            ),
            ""
        );
        assert!(
            !self
                .git(home, &["worktree", "list", "--porcelain"])
                .lines()
                .any(|l| l == format!("worktree {}", a["worktree"].as_str().unwrap()))
        );
        let mut ids: Vec<_> = expected
            .keys()
            .filter(|p| p.file_name().unwrap() == "node.md")
            .map(|p| {
                p.components()
                    .nth(1)
                    .unwrap()
                    .as_os_str()
                    .to_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        ids.sort();
        let sql = self.cli(&["sql", text(home), "SELECT id FROM nodes ORDER BY id"]);
        assert_eq!(sql["version"], receipt["version"]);
        assert_eq!(
            sql["result"]["rows"],
            json!(ids.iter().map(|id| vec![id]).collect::<Vec<_>>())
        );
        for id in &ids {
            let relative = PathBuf::from(format!("nodes/{id}/node.md"));
            let body = self.run(
                env!("CARGO_BIN_EXE_sifttext"),
                &["read", text(home), id],
                true,
            );
            assert_eq!(body.as_bytes(), expected[&relative]);
            for kind in ["outline.lean", "outline.full.lean"] {
                assert!(home.join("nodes").join(id).join(kind).is_file());
            }
        }
    }
}
#[test]
fn single_save_concurrent_drafts_home_cleanup_and_conflict_repair() {
    let temp = tempfile::tempdir().unwrap();
    let evidence = if let Some(path) = std::env::var_os("SIFTTEXT_TEST_EVIDENCE") {
        let path = PathBuf::from(path);
        assert!(path.is_absolute());
        fs::create_dir(&path).unwrap();
        path.canonicalize().unwrap()
    } else {
        temp.path().canonicalize().unwrap()
    };
    let config = evidence.join("git-home");
    fs::create_dir(&config).unwrap();
    fs::write(config.join(".gitconfig"), "[user]\nname = Synthetic Fixture\nemail = fixture@example.invalid\n[commit]\ngpgSign = false\n[core]\nhooksPath = /dev/null\n").unwrap();
    let mut n = Native {
        evidence: evidence.clone(),
        config,
        records: vec![],
    };
    let home = evidence.join("home");
    n.cli(&["init", text(&home), "--empty"]);
    let first = n.cli(&["worktree", text(&home)]);
    let second = n.cli(&["worktree", text(&home)]);
    let first_path = PathBuf::from(first["worktree"].as_str().unwrap());
    let second_path = PathBuf::from(second["worktree"].as_str().unwrap());
    let root = n.cli(&[
        "create",
        text(&first_path),
        "Saveanchor",
        "Owns the complete home check.",
    ]);
    let child = n.cli(&[
        "create",
        text(&first_path),
        "Savechild",
        "Checks complete ancestry.",
        root["id"].as_str().unwrap(),
    ]);
    let other = n.cli(&[
        "create",
        text(&second_path),
        "Saveseparate",
        "Checks competing draft custody.",
    ]);
    let first_sha = n.commit(&first_path);
    let second_sha = n.commit(&second_path);
    let first_files = snapshot(&first_path);
    let second_files = snapshot(&second_path);
    let second_status = n.git(&second_path, &["status", "--porcelain"]);
    let first_receipt = n.submit(&home, &first, &first_sha, true);
    assert_eq!(
        first_receipt["candidate"], first_sha,
        "fast-forward must not manufacture history"
    );
    n.finished(&home, &first, &first_receipt, &first_files);
    let accepted = n.cli(&["accepted", text(&home)]);
    assert_eq!(accepted["status"], "current");
    let forced = n.run(
        env!("CARGO_BIN_EXE_sifttext"),
        &["accepted", text(&home), "-y"],
        true,
    );
    let forced = receipt(&forced);
    for key in ["status", "candidate", "check", "validation"] {
        assert_eq!(forced[key], accepted[key], "accepted -y field {key}");
    }
    assert!(accepted.get("graph_checks").is_none());
    assert!(accepted["validation"].get("graph").is_none());
    // A lost CLI response can be recovered by retrying the original request;
    // after draft removal it must return the accepted receipt without advancing again.
    let retried = n.submit(&home, &first, &first_sha, true);
    assert_eq!(retried["candidate"], first_receipt["candidate"]);
    assert_eq!(n.git(&home, &["rev-parse", "refs/heads/main"]), first_sha);
    assert!(!first_path.exists());
    assert_eq!(snapshot(&home), first_files);
    assert_eq!(snapshot(&second_path), second_files);
    assert_eq!(n.git(&second_path, &["rev-parse", "HEAD"]), second_sha);
    assert_eq!(
        n.git(&second_path, &["status", "--porcelain"]),
        second_status
    );
    let mut combined = first_files.clone();
    combined.extend(second_files);
    let second_receipt = n.submit(&home, &second, &second_sha, true);
    n.finished(&home, &second, &second_receipt, &combined);
    let merge = second_receipt["candidate"].as_str().unwrap();
    let mut parents: Vec<_> = n
        .git(&home, &["show", "-s", "--format=%P", merge])
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    parents.sort();
    let mut expected_parents = vec![first_sha, second_sha];
    expected_parents.sort();
    assert_eq!(parents, expected_parents);
    for node in [&root, &child, &other] {
        let name: String = serde_json::from_str(
            fs::read_to_string(
                home.join(format!("nodes/{}/node.md", node["id"].as_str().unwrap())),
            )
            .unwrap()
            .lines()
            .find_map(|l| l.strip_prefix("name: "))
            .unwrap(),
        )
        .unwrap();
        let search = n.cli(&["search", text(&home), &name]);
        assert_eq!(search["version"], second_receipt["version"]);
        assert_eq!(search["total_matches"], 1);
        assert_eq!(search["hits"][0]["id"], node["id"]);
    }
    // Complete shallow/full rows for the two-level root, not merely file existence.
    for full in [false, true] {
        let mut args = vec!["outline", text(&home), root["id"].as_str().unwrap()];
        if full {
            args.push("full");
        }
        let outline = n.run(env!("CARGO_BIN_EXE_sifttext"), &args, true);
        let mut lines = outline.lines();
        let header = lines.find(|l| l.starts_with("rows[")).unwrap();
        let (count, fields) = header
            .strip_prefix("rows[")
            .unwrap()
            .split_once("]:")
            .unwrap();
        assert_eq!(count, "2");
        let id_column = fields.split('\t').position(|f| f == "id").unwrap();
        let row_ids: Vec<_> = lines
            .take(2)
            .map(|l| {
                l.strip_prefix("  ")
                    .unwrap()
                    .split('\t')
                    .nth(id_column)
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_eq!(
            row_ids,
            vec![root["id"].as_str().unwrap(), child["id"].as_str().unwrap()]
        );
    }
    let left = n.cli(&["worktree", text(&home)]);
    let right = n.cli(&["worktree", text(&home)]);
    let relative = format!("nodes/{}/node.md", root["id"].as_str().unwrap());
    for (a, line) in [(&left, "Left choice"), (&right, "Right choice")] {
        let draft = Path::new(a["worktree"].as_str().unwrap());
        let mut bytes = fs::read(draft.join(&relative)).unwrap();
        bytes.extend_from_slice(format!("\n{line}\n").as_bytes());
        fs::write(draft.join(&relative), bytes).unwrap();
        n.run(
            env!("CARGO_BIN_EXE_sifttext"),
            &["normalize", text(&home), text(draft)],
            true,
        );
    }
    let left_path = PathBuf::from(left["worktree"].as_str().unwrap());
    let right_path = PathBuf::from(right["worktree"].as_str().unwrap());
    let left_sha = n.commit(&left_path);
    let right_sha = n.commit(&right_path);
    let left_files = snapshot(&left_path);
    let left_receipt = n.submit(&home, &left, &left_sha, true);
    n.finished(&home, &left, &left_receipt, &left_files);
    let refused = n.submit(&home, &right, &right_sha, false);
    assert_eq!(refused["check"], "git_file_conflict");
    assert_eq!(
        n.git(&home, &["rev-parse", "refs/heads/main"]),
        left_receipt["candidate"].as_str().unwrap()
    );
    assert_eq!(snapshot(&home), left_files);
    assert!(right_path.exists());
    assert_eq!(
        n.git(&right_path, &["rev-parse", "MERGE_HEAD"]),
        left_receipt["candidate"].as_str().unwrap()
    );
    assert!(
        !n.git(&right_path, &["ls-files", "-u", "--", &relative])
            .is_empty()
    );
    let mut repaired = fs::read(home.join(&relative)).unwrap();
    repaired.extend_from_slice(b"\nSelected repaired intent.\n");
    fs::write(right_path.join(&relative), repaired).unwrap();
    let repair_sha = n.commit(&right_path);
    let repaired_files = snapshot(&right_path);
    let repair_receipt = n.submit(&home, &right, &repair_sha, true);
    n.finished(&home, &right, &repair_receipt, &repaired_files);
    assert_eq!(repair_receipt["candidate"], repair_sha);
    let next = n.cli(&["worktree", text(&home)]);
    assert_eq!(next["base"], repair_receipt["candidate"]);
    let invalid_path = PathBuf::from(next["worktree"].as_str().unwrap());
    let authored = invalid_path.join(&relative);
    let invalid = fs::read_to_string(&authored)
        .unwrap()
        .lines()
        .map(|line| {
            if line.starts_with("name: ") {
                "name: ' '"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&authored, invalid).unwrap();
    let invalid_sha = n.commit(&invalid_path);
    let before_refusal = snapshot(&home);
    let invalid_receipt = n.submit(&home, &next, &invalid_sha, false);
    assert_eq!(invalid_receipt["status"], "rejected");
    assert_eq!(invalid_receipt["check"], "candidate_validation_rejected");
    assert_eq!(invalid_receipt["validation"]["status"], "rejected");
    assert!(
        !invalid_receipt["validation"]["rows"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(invalid_receipt.get("graph_checks").is_none());
    assert!(invalid_receipt["validation"].get("graph").is_none());
    assert_eq!(snapshot(&home), before_refusal);
    assert!(invalid_path.exists());
    fs::write(evidence.join("result.json"), serde_json::to_vec_pretty(&json!({"status":"passed","scope":"same-base concurrent drafts, fast-forward/merge history, exact normal-home bytes/read/SQL/search/navigation, draft/branch cleanup, conflict repair, and real CLI candidate-validation rejection without graph-routing metadata","last":repair_receipt,"validation_rejection":invalid_receipt,"commands":n.records.len()})).unwrap()).unwrap();
}
