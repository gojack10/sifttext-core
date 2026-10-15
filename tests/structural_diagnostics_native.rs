//! Native CLI/Git parity for structural_diagnostics.mjs; no product renderer as oracle.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
fn id(n: usize) -> String {
    format!("62000000-0000-4000-8000-{n:012}")
}
fn file(s: &Path, n: usize) -> PathBuf {
    s.join(format!("nodes/{}/node.md", id(n)))
}
fn node(s: &Path, n: usize, patch: Value, body: &str) {
    let mut f = json!({"id":id(n),"name":"PRIVATE name","micro_summary":"Summary","status":"in_progress","see_also":[]});
    for (k, v) in patch.as_object().unwrap() {
        f[k] = v.clone();
    }
    let mut text = String::from("---\n");
    for (k, v) in f.as_object().unwrap() {
        text.push_str(&format!("{k}: {v}\n"));
    }
    fs::write(file(s, n), format!("{text}---\n{body}")).unwrap();
}
fn fixture(e: &Path, label: &str) -> PathBuf {
    let s = e.join(label);
    fs::create_dir_all(s.join("trees")).unwrap();
    for n in 1..=3 {
        let p = s.join(format!("nodes/{}", id(n)));
        fs::create_dir_all(p.join("children")).unwrap();
        fs::write(p.join("children/.keep"), "").unwrap();
        fs::write(p.join("children.order"), "").unwrap();
        node(&s, n, json!({}), "PRIVATE body");
        symlink(format!("../nodes/{}", id(n)), s.join("trees").join(id(n))).unwrap();
    }
    s
}
fn edge(s: &Path, p: usize, c: usize) {
    symlink(
        format!("../../{}", id(c)),
        s.join(format!("nodes/{}/children/{}", id(p), id(c))),
    )
    .unwrap();
    fs::write(
        s.join(format!("nodes/{}/children.order", id(p))),
        format!("{}\n", id(c)),
    )
    .unwrap();
}
fn command(bin: &Path) -> Command {
    let mut c = Command::new(bin);
    for (k, _) in std::env::vars_os() {
        if k.as_encoded_bytes().starts_with(b"GIT_") {
            c.env_remove(k);
        }
    }
    c.stdout(Stdio::piped()).stderr(Stdio::piped());
    c
}
// The report's fixed scalar/tabular dialect needs no general LEAN implementation.
// Parse independently of the product renderer and reject unrecognized rows/cells.
fn scalar(s: &str) -> Value {
    match s {
        "_" => Value::Null,
        "T" => json!(true),
        "F" => json!(false),
        _ => {
            if s.starts_with('"') {
                serde_json::from_str(s).unwrap()
            } else if let Ok(n) = s.parse::<u64>() {
                json!(n)
            } else {
                json!(s)
            }
        }
    }
}
fn decode(text: &str) -> Value {
    let mut out = json!({});
    let mut lines = text.lines();
    let mut rows = Vec::new();
    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix("rows[") {
            let (n, header) = rest.split_once("]:").unwrap();
            let n: usize = n.parse().unwrap();
            let keys: Vec<_> = header.split('\t').collect();
            if n > 0 {
                assert_eq!(
                    keys,
                    ["check", "id", "law", "rule", "source", "target", "tokens"]
                );
            }
            for _ in 0..n {
                let cells: Vec<_> = lines
                    .next()
                    .unwrap()
                    .strip_prefix("  ")
                    .unwrap()
                    .split('\t')
                    .collect();
                assert_eq!(cells.len(), keys.len());
                let mut row = json!({});
                for (k, v) in keys.iter().zip(cells) {
                    row[*k] = scalar(v);
                }
                rows.push(row);
            }
            assert!(lines.next().is_none(), "unexpected data after rows");
            break;
        }
        let (k, v) = line.split_once(':').unwrap();
        assert!(out.get(k).is_none());
        out[k] = scalar(v);
    }
    out["rows"] = json!(rows);
    out
}
fn uuid(v: &Value) -> bool {
    v.is_null()
        || v.as_str().is_some_and(|s| {
            s.len() == 36
                && s.bytes().enumerate().all(|(i, c)| {
                    if [8, 13, 18, 23].contains(&i) {
                        c == b'-'
                    } else {
                        c.is_ascii_digit() || (b'a'..=b'f').contains(&c)
                    }
                })
        })
}
fn issue(r: &Value, source: Option<usize>, target: Option<usize>, law: &str, check: Option<&str>) {
    assert!(
        r["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["source"] == json!(source.map(id))
                && row["target"] == json!(target.map(id))
                && row["law"] == law
                && check.is_none_or(|c| row["check"] == c)),
        "{r}"
    );
}
struct Native {
    bin: PathBuf,
    e: PathBuf,
    calls: usize,
    cases: usize,
}
impl Native {
    fn receipt(&mut self, q: &Value, out: &Output, elapsed: Duration) {
        fs::write(self.e.join(format!("call-{}.json",self.calls)),serde_json::to_vec_pretty(&json!({"request":q,"exit":out.status.code(),"stdout":String::from_utf8_lossy(&out.stdout),"stderr":String::from_utf8_lossy(&out.stderr),"elapsed_ms":elapsed.as_secs_f64()*1000.0})).unwrap()).unwrap();
        self.calls += 1;
    }
    fn run(&mut self, bin: &Path, args: &[&str], q: Value) -> Output {
        let start = Instant::now();
        let mut c =
            if cfg!(target_os = "macos") && bin.file_name().unwrap() == "sifttext-file-graph" {
                let mut c = command(Path::new("/usr/bin/time"));
                c.args(["-l", bin.to_str().unwrap()]);
                c
            } else {
                command(bin)
            };
        let out = c.args(args).output().unwrap();
        self.receipt(&q, &out, start.elapsed());
        out
    }
    fn inspect(&mut self, out: Output, status: &str) -> Value {
        assert_eq!(
            out.status.code(),
            Some(if status == "current" { 0 } else { 1 }),
            "{out:?}"
        );
        let text = String::from_utf8(out.stdout).unwrap();
        let r = decode(&text);
        assert_eq!(r["status"], status);
        assert!(!text.contains("PRIVATE") && !text.contains(self.e.to_str().unwrap()));
        for row in r["rows"].as_array().unwrap() {
            for k in ["id", "source", "target"] {
                assert!(uuid(&row[k]), "{row}");
            }
            assert!(!row["rule"].is_null() && uuid(&row["rule"]));
        }
        self.cases += 1;
        r
    }
    fn validate(&mut self, q: Value, status: &str) -> Value {
        let p = self.e.join(format!("request-{}.json", self.calls));
        fs::write(&p, q.to_string()).unwrap();
        let bin = self.bin.join("sifttext-file-graph");
        let out = self.run(&bin, &["validate-candidate", p.to_str().unwrap()], q);
        self.inspect(out, status)
    }
    fn git(&mut self, s: &Path, args: &[&str]) -> String {
        let mut a = vec![
            "-C",
            s.to_str().unwrap(),
            "-c",
            "user.name=Synthetic Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ];
        a.extend_from_slice(args);
        let out = self.run(Path::new("git"), &a, json!({"git":args,"store":s}));
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).unwrap().trim().into()
    }
}
#[test]
fn structural_cli_git_conflicts_identities_privacy_and_race() {
    let temp = tempfile::tempdir().unwrap();
    let e = std::env::var_os("SIFTTEXT_TEST_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("evidence"));
    assert!(e.is_absolute());
    fs::create_dir(&e).unwrap();
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_BIN_EXE_core-observe"))
                .parent()
                .unwrap()
                .into()
        });
    let mut n = Native {
        bin,
        e: e.clone(),
        calls: 0,
        cases: 0,
    };
    let store = fixture(&e, "main");
    n.git(&store, &["init", "-q", "-b", "main"]);
    n.git(&store, &["add", "."]);
    n.git(&store, &["commit", "-qm", "fixture: baseline"]);
    let baseline = n.git(&store, &["rev-parse", "HEAD"]);
    let draft = e.join("draft");
    n.git(
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
    for i in [1, 3] {
        node(
            &draft,
            i,
            json!({"see_also":[{"target":id(2),"reason":"PRIVATE explicit reason"}]}),
            "PRIVATE body",
        );
    }
    n.git(&draft, &["add", "."]);
    n.git(&draft, &["commit", "-qm", "fixture: references"]);
    let draft_sha = n.git(&draft, &["rev-parse", "HEAD"]);
    let draft_tree = n.git(&draft, &["rev-parse", "HEAD^{tree}"]);
    n.validate(json!({"store":draft}), "current");
    n.git(
        &store,
        &[
            "rm",
            "-qr",
            &format!("nodes/{}", id(2)),
            &format!("trees/{}", id(2)),
        ],
    );
    n.git(&store, &["commit", "-qm", "fixture: delete target"]);
    let main_sha = n.git(&store, &["rev-parse", "HEAD"]);
    let main_tree = n.git(&store, &["rev-parse", "HEAD^{tree}"]);
    let combined = e.join("combined");
    n.git(
        &store,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "combined",
            combined.to_str().unwrap(),
        ],
    );
    n.git(&combined, &["merge", "--no-commit", "--no-ff", "draft"]);
    assert_eq!(n.git(&combined, &["ls-files", "-u"]), "");
    let bin = n.bin.join("sifttext-file-graph");
    let out = n.run(
        &bin,
        &["validate", combined.to_str().unwrap()],
        json!({"validate":combined}),
    );
    assert!(!out.status.success());
    let r = n.validate(json!({"store":combined}), "rejected");
    for i in [1, 3] {
        issue(
            &r,
            Some(i),
            Some(2),
            "laws/Spec.bend:refs_valid",
            Some("dangling_reference"),
        );
    }
    assert_eq!(r["rows"].as_array().unwrap().len(), 2);
    let q = e.join("publication.json");
    fs::write(&q, json!({"store":combined}).to_string()).unwrap();
    let out = n.run(
        Path::new("/bin/sh"),
        &[
            "-c",
            "\"$1\" validate-candidate \"$2\" && git -C \"$3\" commit -qm publication",
            "sh",
            bin.to_str().unwrap(),
            q.to_str().unwrap(),
            combined.to_str().unwrap(),
        ],
        json!({"conditional_publication":combined}),
    );
    assert!(!out.status.success());
    for (s, rev, expected) in [
        (&store, "main", &main_sha),
        (&draft, "draft", &draft_sha),
        (&store, "main^{tree}", &main_tree),
        (&draft, "draft^{tree}", &draft_tree),
        (&combined, "HEAD", &main_sha),
    ] {
        assert_eq!(n.git(s, &["rev-parse", rev]), *expected);
    }
    for s in [&store, &draft] {
        assert_eq!(n.git(s, &["status", "--porcelain"]), "");
    }
    let q = json!({"op":"validate_candidate","store":combined});
    let start = Instant::now();
    let mut child = command(&n.bin.join("core-observe"))
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(q.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    n.receipt(&q, &out, start.elapsed());
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["rows"],
        r["rows"]
    );
    let s = fixture(&e, "root-child");
    edge(&s, 1, 2);
    issue(
        &n.validate(json!({"store":s}), "rejected"),
        Some(1),
        Some(2),
        "laws/Spec.bend:nodes_valid",
        Some("placement_count"),
    );
    fs::remove_file(s.join("trees").join(id(2))).unwrap();
    n.validate(json!({"store":s}), "current");
    edge(&s, 3, 2);
    let r = n.validate(json!({"store":s}), "rejected");
    for p in [1, 3] {
        issue(&r, Some(p), Some(2), "laws/Spec.bend:nodes_valid", None);
    }
    let s = fixture(&e, "cycle");
    for i in [1, 2] {
        fs::remove_file(s.join("trees").join(id(i))).unwrap();
    }
    edge(&s, 1, 2);
    edge(&s, 2, 1);
    let r = n.validate(json!({"store":s}), "rejected");
    for (p, c) in [(1, 2), (2, 1)] {
        issue(&r, Some(p), Some(c), "laws/Spec.bend:rooted", None);
    }
    let s = fixture(&e, "orphan");
    fs::remove_file(s.join("trees").join(id(2))).unwrap();
    issue(
        &n.validate(json!({"store":s}), "rejected"),
        Some(2),
        None,
        "laws/Spec.bend:nodes_valid",
        None,
    );
    for (label, order, target) in [
        ("missing", String::new(), 2),
        ("duplicate", format!("{}\n{}\n", id(2), id(2)), 2),
        ("nonchild", format!("{}\n", id(3)), 3),
    ] {
        let s = fixture(&e, &format!("order-{label}"));
        edge(&s, 1, 2);
        fs::write(s.join(format!("nodes/{}/children.order", id(1))), order).unwrap();
        issue(
            &n.validate(json!({"store":s}), "rejected"),
            Some(1),
            Some(target),
            "src/lib.rs:Store.read_view/order_membership",
            None,
        );
    }
    let s = fixture(&e, "unknown-child");
    edge(&s, 1, 4);
    issue(
        &n.validate(json!({"store":s}), "rejected"),
        Some(1),
        Some(4),
        "src/lib.rs:Store.read_view/child_membership",
        None,
    );
    let s = fixture(&e, "unknown-root");
    symlink(format!("../nodes/{}", id(4)), s.join("trees").join(id(4))).unwrap();
    issue(
        &n.validate(json!({"store":s}), "rejected"),
        None,
        Some(4),
        "src/lib.rs:Store.read_view/root_membership",
        None,
    );
    let s = fixture(&e, "unsafe-link");
    edge(&s, 1, 2);
    let link = s.join(format!("nodes/{}/children/{}", id(1), id(2)));
    fs::remove_file(&link).unwrap();
    symlink("/PRIVATE/secret", link).unwrap();
    issue(
        &n.validate(json!({"store":s}), "rejected"),
        Some(1),
        Some(2),
        "src/lib.rs:safe_link",
        None,
    );
    let s = fixture(&e, "unknown-order-identity");
    fs::write(
        s.join(format!("nodes/{}/children.order", id(1))),
        "PRIVATE-invalid-id\n",
    )
    .unwrap();
    issue(
        &n.validate(json!({"store":s}), "rejected"),
        Some(1),
        None,
        "src/lib.rs:Store.read_view/order_uuid",
        None,
    );
    let s = fixture(&e, "malformed");
    fs::write(file(&s, 1), "PRIVATE malformed secret").unwrap();
    let r = n.validate(json!({"store":s}), "rejected");
    assert!(r["rows"].as_array().unwrap().iter().all(|row| {
        ["source", "target", "law"]
            .iter()
            .all(|k| row[*k].is_null())
    }));
    let s = fixture(&e, "mismatch");
    node(&s, 1, json!({"id":id(2)}), "PRIVATE body");
    issue(
        &n.validate(json!({"store":s}), "rejected"),
        Some(1),
        Some(2),
        "src/lib.rs:Store.read_view/node_uuid_identity",
        None,
    );
    let s = fixture(&e, "regressions");
    node(&s, 1, json!({}), &" a".repeat(5000));
    assert!(
        n.validate(json!({"store":s}), "current")["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["tokens"] == 5000)
    );
    node(&s, 1, json!({}), &" a".repeat(5001));
    assert!(
        n.validate(json!({"store":s}), "rejected")["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["tokens"] == 5001 && r["check"] == "crystallization_over_limit")
    );
    node(
        &s,
        1,
        json!({"status":"stuck","blocker":" "}),
        "PRIVATE body",
    );
    assert!(
        n.validate(json!({"store":s}), "rejected")["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["check"] == "status_transition_or_reason")
    );
    node(&s, 1, json!({}), "PRIVATE body");
    let valid = n.validate(json!({"store":s}), "current");
    n.validate(json!({"store":s,"expected_version":"wrong"}), "rejected");
    assert_eq!(
        n.validate(json!({"store":s}), "current")["version"],
        valid["version"]
    );
    let ready = e.join("ready");
    let release = e.join("release");
    let input = e.join("race.json");
    let q = json!({"store":s,"pause_before_accept":{"ready":ready,"release":release}});
    fs::write(&input, q.to_string()).unwrap();
    let start = Instant::now();
    let mut child = command(&bin)
        .args(["validate-candidate", input.to_str().unwrap()])
        .spawn()
        .unwrap();
    while !ready.exists() {
        if start.elapsed() > Duration::from_secs(15) {
            child.kill().unwrap();
            let out = child.wait_with_output().unwrap();
            n.receipt(&q, &out, start.elapsed());
            panic!("missing acceptance barrier: {out:?}");
        }
        assert!(child.try_wait().unwrap().is_none());
        thread::sleep(Duration::from_millis(5));
    }
    let p = file(&s, 2);
    let replacement = p.with_extension("new");
    fs::write(&replacement, fs::read(&p).unwrap()).unwrap();
    fs::rename(replacement, &p).unwrap();
    fs::write(release, "go").unwrap();
    let out = child.wait_with_output().unwrap();
    n.receipt(&q, &out, start.elapsed());
    n.inspect(out, "stale");
    // The legacy counter excludes its separately executed race control.
    assert_eq!(n.cases, 23);
    fs::write(e.join("passed.json"),json!({"cases":n.cases,"receipts":n.calls,"baseline":baseline,"main":main_sha,"draft":draft_sha,"main_tree":main_tree,"draft_tree":draft_tree,"report":r,"limits":["synthetic three-node fixtures; not aggregate scale or speed","real compiled Bend; Rust diagnostics do not replace graph laws"]}).to_string()).unwrap();
}
