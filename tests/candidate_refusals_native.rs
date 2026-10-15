//! Real observer/CLI refusal controls migrated from candidate_refusals.mjs.
//! Build checked assets first. Optional SIFTTEXT_TEST_EVIDENCE retains receipts.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
const ID: &str = "82000000-0000-4000-8000-000000000001";
const SECRET: &str = "PRIVATE_CANDIDATE_SENTINEL";
fn file(p: &Path) -> PathBuf {
    p.join("nodes").join(ID).join("node.md")
}
fn text(status: &str, body: &str) -> String {
    format!(
        "---\nid: \"{ID}\"\nname: \"Node\"\nmicro_summary: \"Summary\"\nstatus: \"{status}\"\nsee_also: []\n---\n{body}"
    )
}
fn hashes(p: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(p: &Path, base: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(p).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() == ".git" {
                continue;
            }
            let path = entry.path();
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                walk(&path, base, out);
            } else {
                let bytes = if meta.file_type().is_symlink() {
                    fs::read_link(&path)
                        .unwrap()
                        .as_os_str()
                        .as_encoded_bytes()
                        .to_vec()
                } else {
                    fs::read(&path).unwrap()
                };
                out.insert(
                    path.strip_prefix(base).unwrap().into(),
                    Sha256::digest(bytes).to_vec(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(p, p, &mut out);
    out
}
fn command(bin: &Path) -> Command {
    let mut c = Command::new(bin);
    for (key, _) in std::env::vars_os() {
        if key.as_encoded_bytes().starts_with(b"GIT_") {
            c.env_remove(key);
        }
    }
    c.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c
}
struct Native {
    bin: PathBuf,
    evidence: PathBuf,
    sequence: usize,
}
impl Native {
    fn receipt(&mut self, request: &Value, output: &Output) {
        let r = json!({"request":request,"exit":output.status.code(),
            "stdout":String::from_utf8_lossy(&output.stdout),"stderr":String::from_utf8_lossy(&output.stderr)});
        fs::write(
            self.evidence.join(format!("call-{}.json", self.sequence)),
            serde_json::to_vec_pretty(&r).unwrap(),
        )
        .unwrap();
        self.sequence += 1;
    }
    fn start(&self, q: &Value) -> Child {
        let mut child = command(&self.bin.join("core-observe")).spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(q).unwrap())
            .unwrap();
        child
    }
    fn inspect(
        &mut self,
        q: &Value,
        output: Output,
        status: &str,
        failure: Option<(&str, &str)>,
    ) -> Value {
        self.receipt(q, &output);
        assert_eq!(
            output.status.code(),
            Some(if status == "current" { 0 } else { 1 }),
            "{output:?}"
        );
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["status"], status);
        if let Some((stage, kind)) = failure {
            assert_eq!(report["failure"], json!({"stage":stage,"kind":kind}));
            assert_eq!(
                serde_json::from_slice::<Value>(&output.stderr).unwrap(),
                json!({"status":status,"stage":stage,"kind":kind})
            );
            for sentinel in [
                SECRET,
                "PRIVATE_PATH_SENTINEL",
                self.evidence.to_str().unwrap(),
            ] {
                assert!(!String::from_utf8_lossy(&output.stdout).contains(sentinel));
                assert!(!String::from_utf8_lossy(&output.stderr).contains(sentinel));
            }
        } else {
            assert!(report.get("failure").is_none());
            assert!(output.stderr.is_empty());
        }
        report
    }
    fn validate(
        &mut self,
        store: &Path,
        mut q: Value,
        status: &str,
        failure: Option<(&str, &str)>,
    ) -> Value {
        q["op"] = json!("validate_candidate");
        let before = hashes(store);
        let output = self.start(&q).wait_with_output().unwrap();
        let report = self.inspect(&q, output, status, failure);
        assert_eq!(
            hashes(store),
            before,
            "validation changed authored or derived bytes"
        );
        report
    }
    fn git(&mut self, p: &Path, args: &[&str]) -> String {
        let output = command(Path::new("git"))
            .args([
                "-C",
                p.to_str().unwrap(),
                "-c",
                "user.name=Synthetic Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgSign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .output()
            .unwrap();
        self.receipt(&json!({"git":args,"store":p}), &output);
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap().trim().into()
    }
}
#[test]
fn native_refusals_preserve_bytes_privacy_exit_and_git_nonpublication() {
    let temp = tempfile::tempdir().unwrap();
    let evidence = if let Some(p) = std::env::var_os("SIFTTEXT_TEST_EVIDENCE") {
        let p = PathBuf::from(p);
        assert!(p.is_absolute());
        fs::create_dir(&p).unwrap();
        p
    } else {
        temp.path().into()
    };
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_BIN_EXE_core-observe"))
                .parent()
                .unwrap()
                .into()
        });
    let mut native = Native {
        bin,
        evidence: evidence.clone(),
        sequence: 0,
    };
    let store = evidence.join("PRIVATE_PATH_SENTINEL");
    fs::create_dir_all(store.join("trees")).unwrap();
    fs::create_dir_all(store.join("nodes").join(ID).join("children")).unwrap();
    fs::write(store.join("nodes").join(ID).join("children/.keep"), "").unwrap();
    fs::write(store.join("nodes").join(ID).join("children.order"), "").unwrap();
    let original = text("in_progress", SECRET);
    fs::write(file(&store), &original).unwrap();
    symlink(format!("../nodes/{ID}"), store.join("trees").join(ID)).unwrap();
    fs::write(
        store.join(".gitignore"),
        ".sifttext/\n*.lean\n*.meta.json\n",
    )
    .unwrap();
    let first = native.validate(&store, json!({"store":store}), "current", None);
    let mut keys = first
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "before_version",
            "graph",
            "nodes",
            "rows",
            "status",
            "version"
        ]
    );
    native.git(&store, &["init", "-q", "-b", "main"]);
    native.git(&store, &["add", ".gitignore", "nodes", "trees"]);
    native.git(&store, &["commit", "-qm", "fixture: baseline"]);
    let baseline = native.git(&store, &["rev-parse", "HEAD"]);
    let q = json!({"op":"index","store":store,"mode":"full"});
    let output = native.start(&q).wait_with_output().unwrap();
    native.receipt(&q, &output);
    assert!(output.status.success(), "{output:?}");
    assert!(store.join(".sifttext/index.sqlite").is_file());
    native.validate(
        &store,
        json!({"store":store,"expected_version":first["version"]}),
        "current",
        None,
    );
    native.validate(
        &store,
        json!({"store":evidence.join("MISSING_PRIVATE_PATH_SENTINEL")}),
        "rejected",
        Some(("candidate_capture", "not_found")),
    );
    fs::write(file(&store), SECRET).unwrap();
    native.validate(
        &store,
        json!({"store":store}),
        "rejected",
        Some(("candidate_capture", "invalid_capture")),
    );
    fs::write(file(&store), &original).unwrap();
    fs::hard_link(file(&store), evidence.join("hard-link")).unwrap();
    native.validate(
        &store,
        json!({"store":store}),
        "rejected",
        Some(("candidate_capture", "unknown")),
    );
    fs::remove_file(evidence.join("hard-link")).unwrap();
    assert_eq!(
        native.validate(&store, json!({"store":store}), "current", None)["version"],
        first["version"]
    );
    native.validate(
        &store,
        json!({"store":store,"before":evidence.join("missing-before")}),
        "rejected",
        Some(("before_capture", "not_found")),
    );
    for (status, body) in [
        ("bogus", SECRET.into()),
        ("stuck", SECRET.into()),
        ("resolved", " ".into()),
        ("in_progress", " a".repeat(5001)),
    ] {
        fs::write(file(&store), text(status, &body)).unwrap();
        native.validate(
            &store,
            json!({"store":store}),
            "rejected",
            Some(("candidate", "invalid_candidate")),
        );
    }
    fs::write(file(&store), text("in_progress", &" a".repeat(5000))).unwrap();
    assert!(
        native.validate(&store, json!({"store":store}), "current", None)["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["tokens"] == 5000)
    );
    fs::write(file(&store), &original).unwrap();
    fs::remove_file(store.join("trees").join(ID)).unwrap();
    native.validate(
        &store,
        json!({"store":store}),
        "rejected",
        Some(("candidate", "invalid_candidate")),
    );
    symlink(format!("../nodes/{ID}"), store.join("trees").join(ID)).unwrap();
    native.validate(
        &store,
        json!({"store":store,"expected_version":"wrong"}),
        "rejected",
        Some(("candidate", "invalid_candidate")),
    );
    let ready = evidence.join("ready");
    let release = evidence.join("release");
    let q = json!({"op":"validate_candidate","store":store,"pause_before_accept":{"ready":ready,"release":release}});
    let before = hashes(&store);
    let mut child = native.start(&q);
    let start = Instant::now();
    while !ready.exists() {
        if start.elapsed() > Duration::from_secs(20) {
            child.kill().unwrap();
            let out = child.wait_with_output().unwrap();
            native.receipt(&q, &out);
            panic!("acceptance barrier unavailable: {out:?}");
        }
        assert!(
            child.try_wait().unwrap().is_none(),
            "observer exited before barrier"
        );
        thread::sleep(Duration::from_millis(5));
    }
    fs::write(
        file(&store),
        text("in_progress", &format!("changed {SECRET}")),
    )
    .unwrap();
    let mutated = hashes(&store);
    assert_ne!(before, mutated);
    fs::write(release, "go").unwrap();
    native.inspect(
        &q,
        child.wait_with_output().unwrap(),
        "stale",
        Some(("source_recheck", "source_changed")),
    );
    assert_eq!(hashes(&store), mutated);
    fs::write(file(&store), &original).unwrap();
    assert_eq!(
        native.validate(&store, json!({"store":store}), "current", None)["version"],
        first["version"]
    );
    let input = evidence.join("cli-request.json");
    fs::write(
        &input,
        serde_json::to_vec(&json!({"store":evidence.join("missing")})).unwrap(),
    )
    .unwrap();
    let output = command(&native.bin.join("sifttext-file-graph"))
        .args(["validate-candidate", input.to_str().unwrap()])
        .output()
        .unwrap();
    native.receipt(&json!({"cli":"validate-candidate"}), &output);
    assert!(!output.status.success());
    let lean = String::from_utf8(output.stdout).unwrap();
    // Independent exact scalar expectations, not the implementation's own renderer.
    assert!(lean.lines().any(|l| l == "failure_kind:not_found"));
    assert!(lean.lines().any(|l| l == "failure_stage:candidate_capture"));
    assert!(!lean.contains(evidence.to_str().unwrap()));
    let draft = evidence.join("draft");
    native.git(
        &store,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "draft",
            draft.to_str().unwrap(),
        ],
    );
    fs::write(file(&draft), text("resolved", " ")).unwrap();
    native.git(&draft, &["add", "nodes"]);
    native.git(&draft, &["commit", "-qm", "fixture: invalid draft"]);
    let request = json!({"op":"validate_candidate","store":draft,"before":store});
    let attempt = command(Path::new("/bin/sh"))
        .args([
            "-c",
            "printf '%s' \"$1\" | \"$2\" && git -C \"$3\" merge --ff-only draft",
            "sh",
            &request.to_string(),
            native.bin.join("core-observe").to_str().unwrap(),
            store.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    native.receipt(&request, &attempt);
    assert!(!attempt.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&attempt.stdout).unwrap()["failure"]["kind"],
        "invalid_candidate"
    );
    assert_eq!(native.git(&store, &["rev-parse", "HEAD"]), baseline);
    fs::write(file(&draft), text("in_progress", "valid later recovery")).unwrap();
    native.validate(
        &store,
        json!({"store":draft,"before":store}),
        "current",
        None,
    );
    assert_eq!(native.git(&store, &["rev-parse", "HEAD"]), baseline);
}
