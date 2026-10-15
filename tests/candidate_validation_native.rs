//! CLI/Git parity for candidate_validation.mjs; build checked assets first.
//! SIFTTEXT_TEST_EVIDENCE keeps requests and process outputs in a fresh directory.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
fn id(n: usize) -> String {
    format!("50000000-0000-4000-8000-{n:012}")
}
fn file(store: &Path, n: usize) -> PathBuf {
    store.join("nodes").join(id(n)).join("node.md")
}
fn fields() -> Value {
    json!({"name":"Node","micro_summary":"Summary","status":"in_progress","see_also":[]})
}
fn node(store: &Path, n: usize, mut fields: Value, body: &str) {
    fields["id"] = json!(id(n));
    let mut text = String::from("---\n");
    for (key, value) in fields.as_object().unwrap() {
        text.push_str(&format!("{key}: {value}\n"));
    }
    fs::write(file(store, n), format!("{text}---\n{body}")).unwrap();
}
fn fixture(store: &Path) {
    fs::create_dir_all(store.join("trees")).unwrap();
    for n in 1..=2 {
        let p = store.join("nodes").join(id(n));
        fs::create_dir_all(p.join("children")).unwrap();
        fs::write(p.join("children/.keep"), "").unwrap();
        fs::write(p.join("children.order"), "").unwrap();
        node(store, n, fields(), "Body");
        symlink(
            format!("../nodes/{}", id(n)),
            store.join("trees").join(id(n)),
        )
        .unwrap();
    }
}
fn command(bin: &Path) -> Command {
    let mut c = Command::new(bin);
    for (key, _) in std::env::vars_os() {
        if key.as_encoded_bytes().starts_with(b"GIT_") {
            c.env_remove(key);
        }
    }
    c.stdout(Stdio::piped()).stderr(Stdio::piped());
    c
}
// Independent pinned LEAN expectations for this scalar-only report. Not a general decoder.
fn report(output: &Output, evidence: &Path) -> Value {
    let text = std::str::from_utf8(&output.stdout).unwrap();
    assert!(
        !text.contains(evidence.to_str().unwrap()),
        "native path leaked"
    );
    let mut scalars = BTreeMap::new();
    let mut rows = Vec::new();
    let mut lines = text.lines();
    for line in lines.by_ref() {
        if let Some(header) = line.strip_prefix("rows[") {
            let (count, columns) = header.split_once("]:").unwrap();
            assert_eq!(columns, "check\tid\tlaw\trule\tsource\ttarget\ttokens");
            scalars.insert("row_count", json!(count.parse::<usize>().unwrap()));
            break;
        }
        let (key, value) = line.split_once(':').unwrap();
        assert!(scalars.insert(key, json!(value)).is_none());
    }
    for line in lines {
        let cells: Vec<_> = line.strip_prefix("  ").unwrap().split('\t').collect();
        assert_eq!(cells.len(), 7);
        let uuid = |v: &str| {
            assert!(v.len() == 36 && v.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-'));
        };
        uuid(cells[3]);
        if cells[1] != "_" {
            uuid(cells[1]);
        }
        rows.push(json!({"check":cells[0],"id":cells[1],"rule":cells[3],
            "tokens":if cells[6] == "_" { None } else { Some(cells[6].parse::<usize>().unwrap()) }}));
    }
    assert_eq!(scalars.get("row_count"), Some(&json!(rows.len())));
    let mut value = serde_json::to_value(scalars).unwrap();
    value["rows"] = json!(rows);
    value
}
struct Native {
    bin: PathBuf,
    evidence: PathBuf,
    calls: usize,
}
impl Native {
    fn receipt(&mut self, request: &Value, output: &Output, seconds: f64) {
        fs::write(
            self.evidence.join(format!("call-{}.json", self.calls)),
            serde_json::to_vec_pretty(&json!({"request":request,"exit":output.status.code(),
                "seconds":seconds,"stdout":String::from_utf8_lossy(&output.stdout),
                "stderr":String::from_utf8_lossy(&output.stderr)}))
            .unwrap(),
        )
        .unwrap();
        self.calls += 1;
    }
    fn input(&self, q: &Value) -> PathBuf {
        let input = self.evidence.join(format!("request-{}.json", self.calls));
        fs::write(&input, serde_json::to_vec(q).unwrap()).unwrap();
        input
    }
    fn invoke(&mut self, q: Value, pass: bool) -> Value {
        let start = Instant::now();
        let output = command(&self.bin)
            .args(["validate-candidate", self.input(&q).to_str().unwrap()])
            .output()
            .unwrap();
        self.receipt(&q, &output, start.elapsed().as_secs_f64());
        assert_eq!(output.status.success(), pass, "{output:?}");
        let r = report(&output, &self.evidence);
        assert_eq!(r["status"] == "current", pass);
        r
    }
    fn git(&mut self, p: &Path, args: &[&str]) -> String {
        let start = Instant::now();
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
        self.receipt(
            &json!({"git":args,"store":p}),
            &output,
            start.elapsed().as_secs_f64(),
        );
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap().trim().into()
    }
}
#[test]
fn native_candidate_cli_ceiling_races_and_checked_git_publication() {
    let t = tempfile::tempdir().unwrap();
    let evidence = if let Some(p) = std::env::var_os("SIFTTEXT_TEST_EVIDENCE") {
        let p = PathBuf::from(p);
        assert!(p.is_absolute());
        fs::create_dir(&p).unwrap();
        p
    } else {
        t.path().into()
    };
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(|p| PathBuf::from(p).join("sifttext-file-graph"))
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_sifttext-file-graph").into());
    let mut native = Native {
        bin,
        evidence: evidence.clone(),
        calls: 0,
    };
    let store = evidence.join("knowledge");
    fixture(&store);
    native.git(&store, &["init", "-q", "-b", "main"]);
    native.git(&store, &["add", "nodes", "trees"]);
    native.git(&store, &["commit", "-qm", "fixture: baseline"]);
    let baseline = native.git(&store, &["rev-parse", "HEAD"]);
    let first = native.invoke(json!({"store":store}), true);
    native.invoke(
        json!({"store":store,"expected_version":first["version"]}),
        true,
    );
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
    node(&draft, 2, fields(), "Ordinary editor change");
    let valid = native.invoke(json!({"store":draft,"before":store}), true);
    // The independent pinned oracle fixture used by the old runner: N ordinary
    // repetitions of " a" are exactly N tokens. Do not derive this from output.
    fs::write(
        evidence.join("independent-token-expectations.json"),
        "[5000,5001]",
    )
    .unwrap();
    node(&draft, 1, fields(), &" a".repeat(5000));
    assert!(
        native.invoke(json!({"store":draft}), true)["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == id(1) && r["tokens"] == 5000)
    );
    node(&draft, 1, fields(), &" a".repeat(5001));
    assert!(
        native.invoke(json!({"store":draft}), false)["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == id(1)
                && r["tokens"] == 5001
                && r["check"] == "crystallization_over_limit")
    );
    node(&draft, 2, fields(), "A separate edit");
    native.invoke(json!({"store":draft}), false);
    let q = json!({"store":draft,"before":store});
    let input = native.input(&q);
    let start = Instant::now();
    let attempt = command(Path::new("/bin/sh"))
        .args([
            "-c",
            "\"$1\" validate-candidate \"$2\" && git -C \"$3\" merge --ff-only draft",
            "sh",
            native.bin.to_str().unwrap(),
            input.to_str().unwrap(),
            store.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    native.receipt(&q, &attempt, start.elapsed().as_secs_f64());
    assert!(!attempt.status.success());
    assert_eq!(report(&attempt, &evidence)["status"], "rejected");
    assert_eq!(native.git(&store, &["rev-parse", "HEAD"]), baseline);
    for patch in [
        json!({"name":" "}),
        json!({"micro_summary":"\t"}),
        json!({"status":"invalid"}),
        json!({"status":"stuck","blocker":" "}),
        json!({"status":"deferred"}),
        json!({"status":"discarded","discard_reason":null}),
        json!({"status":"resolved","body":" \n\t"}),
        json!({"ruled_out":""}),
        json!({"see_also":[{"target":id(1),"reason":"self"}]}),
        json!({"see_also":[{"target":id(2),"reason":"a"},{"target":id(2),"reason":"b"}]}),
        json!({"see_also":[{"target":id(2),"reason":" "}]}),
    ] {
        let mut f = fields();
        for (key, value) in patch.as_object().unwrap() {
            if key != "body" {
                f[key] = value.clone();
            }
        }
        node(&draft, 1, f, patch["body"].as_str().unwrap_or("Body"));
        let bytes = fs::read(file(&draft, 1)).unwrap();
        native.invoke(json!({"store":draft}), false);
        assert_eq!(fs::read(file(&draft, 1)).unwrap(), bytes);
    }
    let mut f = fields();
    f["scope"] = json!(" a".repeat(6000));
    f["warnings"] = json!(" a".repeat(6000));
    node(&draft, 1, f, "Body");
    native.invoke(json!({"store":draft}), true);
    fs::write(
        draft.join("nodes").join(id(1)).join("node.meta.json"),
        " a".repeat(6000),
    )
    .unwrap();
    native.invoke(json!({"store":draft}), true);
    node(&draft, 1, fields(), "Body");
    native.invoke(
        json!({"store":draft,"expected_version":valid["version"]}),
        false,
    );
    let order = draft.join("nodes").join(id(1)).join("children.order");
    fs::write(&order, format!("{}\n", id(2))).unwrap();
    native.invoke(json!({"store":draft}), false);
    fs::write(&order, "").unwrap();
    for (n, target) in [&draft, &store].into_iter().enumerate() {
        let ready = evidence.join(format!("ready-{n}"));
        let release = evidence.join(format!("release-{n}"));
        let q = json!({"store":draft,"before":store,"pause_before_accept":{"ready":ready,"release":release}});
        let start = Instant::now();
        let mut child = command(&native.bin)
            .args(["validate-candidate", native.input(&q).to_str().unwrap()])
            .spawn()
            .unwrap();
        while !ready.exists() {
            if start.elapsed() > Duration::from_secs(15) {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                native.receipt(&q, &output, start.elapsed().as_secs_f64());
                panic!("acceptance barrier unavailable: {output:?}");
            }
            assert!(
                child.try_wait().unwrap().is_none(),
                "CLI exited before barrier"
            );
            thread::sleep(Duration::from_millis(5));
        }
        if n == 0 {
            node(target, 2, fields(), "Race new body");
        } else {
            let p = file(target, 2);
            let replacement = p.with_extension("new");
            fs::write(&replacement, fs::read(&p).unwrap()).unwrap();
            fs::rename(&replacement, &p).unwrap();
        }
        fs::write(&release, "go").unwrap();
        let output = child.wait_with_output().unwrap();
        native.receipt(&q, &output, start.elapsed().as_secs_f64());
        assert!(!output.status.success());
        assert_eq!(report(&output, &evidence)["status"], "stale");
    }
    node(&draft, 1, fields(), "Body");
    node(&draft, 2, fields(), "Ordinary accepted edit");
    native.git(&draft, &["add", &format!("nodes/{}/node.md", id(2))]);
    native.git(&draft, &["commit", "-qm", "fixture: bounded edit"]);
    let accepted = native.git(&draft, &["rev-parse", "HEAD"]);
    let receipt = native.invoke(json!({"store":draft,"before":store}), true);
    native.invoke(
        json!({"store":draft,"before":store,"expected_version":receipt["version"]}),
        true,
    );
    assert_eq!(native.git(&draft, &["rev-parse", "HEAD"]), accepted);
    native.git(&store, &["merge", "--ff-only", "draft"]);
    assert_eq!(native.git(&store, &["rev-parse", "HEAD"]), accepted);
    native.invoke(json!({"store":store}), true);
    fs::write(evidence.join("passed.json"), serde_json::to_vec_pretty(&json!({
        "calls":native.calls,"baseline":baseline,"accepted":accepted,"independent_body_tokens":[5000,5001],
        "limits":["cooperative single-writer ff-only procedure, not non-bypassability","256-node frozen envelope","no semantic completion or hosted/import evidence"]})).unwrap()).unwrap();
}
