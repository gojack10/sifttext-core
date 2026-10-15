//! Process-level parity for retrieval_contention.mjs; lock unit probes stay independent.
#[path = "common/native_resources.rs"]
mod resources;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::{
        fs::{MetadataExt, symlink},
        process::ExitStatusExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn id(n: usize) -> String {
    format!("61000000-0000-4000-8000-{n:012}")
}
fn fixture(store: &Path) {
    fs::create_dir_all(store.join("trees")).unwrap();
    for n in 1..=7 {
        let p = store.join("nodes").join(id(n));
        fs::create_dir_all(p.join("children")).unwrap();
        fs::write(p.join("node.md"), format!("---\nid: {}\nname: {}\nmicro_summary: {}\nstatus: \"in_progress\"\nscope: \"Login users\"\nsee_also: []\n---\nLogin body {}.\n", json!(id(n)), json!(format!("Login {}", n-1)), json!(format!("Reset summary {}", n-1)), n-1)).unwrap();
        let children: Vec<_> = if n == 1 { (2..=7).collect() } else { vec![] };
        fs::write(
            p.join("children.order"),
            children.iter().map(|n| id(*n) + "\n").collect::<String>(),
        )
        .unwrap();
        for c in children {
            symlink(format!("../../{}", id(c)), p.join("children").join(id(c))).unwrap();
        }
    }
    symlink(
        format!("../nodes/{}", id(1)),
        store.join("trees").join(id(1)),
    )
    .unwrap();
}
#[derive(Debug, PartialEq, Eq)]
struct Files {
    authored: BTreeMap<String, String>,
    derived: BTreeMap<String, String>,
    identities: BTreeMap<String, (u64, u64, i64, i64)>,
}
fn files(store: &Path) -> Files {
    fn walk(store: &Path, dir: &Path, out: &mut Files) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            let m = fs::symlink_metadata(&p).unwrap();
            if m.file_type().is_symlink() {
                continue;
            }
            let name = p.file_name().unwrap().to_str().unwrap();
            if m.is_dir() {
                if name != "children" && name != ".sifttext" {
                    walk(store, &p, out);
                }
                continue;
            }
            let key = p.strip_prefix(store).unwrap().to_str().unwrap().to_owned();
            if name == "node.md" || name == "children.order" {
                out.authored
                    .insert(key, format!("{:x}", Sha256::digest(fs::read(&p).unwrap())));
            } else if name.starts_with("outline.") || name == "index.sqlite" {
                out.derived.insert(
                    key.clone(),
                    format!("{:x}", Sha256::digest(fs::read(&p).unwrap())),
                );
                out.identities
                    .insert(key, (m.dev(), m.ino(), m.ctime(), m.ctime_nsec()));
            }
        }
    }
    let mut out = Files {
        authored: BTreeMap::new(),
        derived: BTreeMap::new(),
        identities: BTreeMap::new(),
    };
    walk(store, store, &mut out);
    let db = store.join(".sifttext/index.sqlite");
    if db.exists() {
        let m = fs::metadata(&db).unwrap();
        let key = ".sifttext/index.sqlite".to_owned();
        out.derived.insert(
            key.clone(),
            format!("{:x}", Sha256::digest(fs::read(db).unwrap())),
        );
        out.identities
            .insert(key, (m.dev(), m.ino(), m.ctime(), m.ctime_nsec()));
    }
    out
}
struct Process {
    child: Child,
    start: Instant,
    stdout: PathBuf,
    stderr: PathBuf,
    request: Value,
    label: String,
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Process {
    fn until(&mut self, check: impl Fn() -> bool) {
        while !check() {
            assert!(
                self.start.elapsed() < Duration::from_secs(10),
                "observation deadline: {}",
                self.label
            );
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "early exit {}: {}",
                self.label,
                fs::read_to_string(&self.stderr).unwrap()
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn finish(mut self) -> Value {
        let status = loop {
            if let Some(s) = self.child.try_wait().unwrap() {
                break s;
            }
            assert!(
                self.start.elapsed() < Duration::from_secs(30),
                "settlement deadline {}",
                self.label
            );
            thread::sleep(Duration::from_millis(5));
        };
        let receipt = json!({"label":self.label,"request":self.request,"code":status.code(),"signal":status.signal(),"stdout":fs::read_to_string(&self.stdout).unwrap(),"stderr":fs::read_to_string(&self.stderr).unwrap(),"ms":self.start.elapsed().as_secs_f64()*1000.});
        fs::write(
            self.stdout.with_extension("receipt.json"),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
        receipt
    }
}
struct Native {
    bin: PathBuf,
    cli: PathBuf,
    evidence: PathBuf,
    calls: usize,
}
impl Native {
    fn launch(
        &mut self,
        request: Value,
        label: &str,
        args: Option<Vec<String>>,
        timed: bool,
    ) -> Process {
        let stdout = self
            .evidence
            .join(format!("{:03}-{label}.stdout", self.calls));
        let stderr = stdout.with_extension("stderr");
        self.calls += 1;
        let mut command = if timed {
            let mut c = Command::new("/usr/bin/time");
            #[cfg(target_os = "macos")]
            c.arg("-l");
            #[cfg(target_os = "linux")]
            c.arg("-v");
            c.arg(&self.bin);
            c
        } else {
            Command::new(if args.is_some() { &self.cli } else { &self.bin })
        };
        if let Some(ref args) = args {
            command.args(args);
        }
        let start = Instant::now();
        let mut child = command
            .env("SIFTTEXT_RETRIEVAL_TRACE", "1")
            .stdin(Stdio::piped())
            .stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        if args.is_none() {
            stdin.write_all(request.to_string().as_bytes()).unwrap();
        }
        drop(stdin);
        Process {
            child,
            start,
            stdout,
            stderr,
            request,
            label: label.into(),
        }
    }
    fn run(&mut self, request: Value, label: &str) -> Value {
        let r = self.launch(request, label, None, true).finish();
        let rss = resources::native_rss(r["stderr"].as_str().unwrap());
        fs::write(
            self.evidence.join(format!("{label}-rss.json")),
            json!({"native_rss_bytes":rss}).to_string(),
        )
        .unwrap();
        r
    }
    fn competing(
        &mut self,
        store: &Path,
        label: &str,
        action: impl FnOnce(),
        kind: &str,
        local: Option<&str>,
        accepted: &Files,
    ) -> (Value, Value) {
        let ready = self.evidence.join(format!("{label}.ready"));
        let release = ready.with_extension("release");
        let mut request = req(store);
        request["pause_before_accept"] = json!({"ready":ready,"release":release});
        let mut first = self.launch(request, label, None, false);
        first.until(|| ready.exists());
        let stage = store.join(".sifttext/outline-stage");
        assert!(stage.exists());
        let m = fs::metadata(&stage).unwrap();
        let staged:Vec<_>=fs::read_dir(&stage).unwrap().map(|e|{ let p=e.unwrap().path(); json!({"name":p.file_name().unwrap().to_str().unwrap(),"hash":format!("{:x}",Sha256::digest(fs::read(&p).unwrap())),"ino":fs::metadata(p).unwrap().ino()}) }).collect();
        assert_eq!(staged.len(), 28);
        let args = local.map(|scope| {
            let mut a = vec![
                "search".into(),
                store.to_str().unwrap().into(),
                "login".into(),
            ];
            if !scope.is_empty() {
                a.push(scope.into());
            }
            a
        });
        let mut second = self.launch(req(store), &format!("{label}-contender"), args, false);
        let err = second.stderr.clone();
        second.until(|| {
            fs::read_to_string(&err)
                .unwrap()
                .contains("retrieval-phase {\"phase\":\"waiting\"}")
        });
        assert!(
            !fs::read_to_string(&err)
                .unwrap()
                .contains("retrieval-phase {\"phase\":\"capture\"}")
        );
        assert!(second.child.try_wait().unwrap().is_none());
        let (_, _, sample) = resources::sample(second.child.id());
        action();
        let (a, b) = if kind == "timeout" {
            let b = second.finish();
            rejected(&b, "busy");
            assert!(b["ms"].as_f64().unwrap() >= 4500. && b["ms"].as_f64().unwrap() < 8000.);
            assert!(
                !b["stderr"]
                    .as_str()
                    .unwrap()
                    .contains("retrieval-phase {\"phase\":\"capture\"}")
            );
            assert_eq!(files(store).derived, accepted.derived);
            fs::write(&release, "release").unwrap();
            (first.finish(), b)
        } else {
            if kind == "kill" {
                first.child.kill().unwrap();
            } else {
                fs::write(&release, "release").unwrap();
            }
            (first.finish(), second.finish())
        };
        assert!(b["ms"].as_f64().unwrap() < 10000.);
        fs::write(self.evidence.join(format!("{label}-interleaving.json")),serde_json::to_vec_pretty(&json!({"ready":ready,"release":release,"stage":{"dev":m.dev(),"ino":m.ino(),"staged":staged},"first":a,"second":b,"family_sample":sample})).unwrap()).unwrap();
        (a, b)
    }
}
fn req(store: &Path) -> Value {
    json!({"op":"search","store":store,"query":"login","under_node_id":null,"readable_ids":(1..=7).map(id).collect::<Vec<_>>(),"limit":20,"offset":0})
}
fn result(r: &Value) -> Value {
    serde_json::from_str(r["stdout"].as_str().unwrap()).unwrap()
}
fn current(r: &Value) -> Value {
    assert_eq!(r["code"], 0, "{r:#}");
    let out = result(r);
    assert_eq!(out["status"], "current");
    assert!(out["version"].is_string());
    assert_eq!(out["total_matches"], 7);
    let mut ids: Vec<_> = out["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap().to_owned())
        .collect();
    ids.sort();
    assert_eq!(ids, (1..=7).map(id).collect::<Vec<_>>());
    out
}
fn unavailable(r: &Value, status: &str) {
    assert_eq!(r["code"], 1, "{r:#}");
    assert_eq!(
        result(r),
        json!({"status":status,"version":null,"total_matches":0,"results":[],"other_groups":[]})
    );
}
fn rejected(r: &Value, kind: &str) {
    assert_eq!(r["code"], 1, "{r:#}");
    assert_eq!(r["stdout"], "");
    let diagnostics: Vec<Value> = r["stderr"]
        .as_str()
        .unwrap()
        .lines()
        .filter(|s| s.starts_with('{'))
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(diagnostics, vec![json!({"kind":kind,"status":"error"})]);
}
fn edit(store: &Path, name: &str) {
    let p = store.join("nodes").join(id(4)).join("node.md");
    let old = fs::read_to_string(&p).unwrap();
    let text = old
        .lines()
        .map(|line| {
            if line.starts_with("name:") {
                format!("name: {}", json!(name))
            } else {
                line.into()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(p, text).unwrap();
}
#[test]
fn native_publisher_contention_scope_refusal_and_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("RETRIEVAL_CONTENTION_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("evidence"));
    fs::create_dir(&evidence).unwrap();
    let bins = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR").map(PathBuf::from);
    let mut n = Native {
        bin: bins
            .as_ref()
            .map(|p| p.join("core-observe"))
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_core-observe").into()),
        cli: bins
            .as_ref()
            .map(|p| p.join("sifttext"))
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_sifttext").into()),
        evidence: evidence.clone(),
        calls: 0,
    };
    let store = evidence.join("store");
    fixture(&store);
    let mut r = req(&store);
    r["store"] = json!(evidence.join("missing"));
    unavailable(&n.run(r, "missing-store"), "error");
    let mut r = req(&store);
    r["base_store"] = json!(store);
    unavailable(&n.run(r, "overlapping-base"), "error");
    assert!(!store.join(".sifttext").exists());
    let cold = current(&n.run(req(&store), "cold"));
    let mut accepted = files(&store);
    current(&n.run(req(&store), "warm"));
    assert_eq!(files(&store), accepted);
    let (a, b) = n.competing(&store, "two-writers", || {}, "normal", None, &accepted);
    assert_eq!(current(&a), cold);
    assert_eq!(current(&b), cold);
    assert_eq!(files(&store), accepted);
    for (label, scope, count) in [
        ("local-search", "", 7),
        ("local-search-leaf", id(4).as_str(), 1),
    ] {
        let (a, b) = n.competing(&store, label, || {}, "normal", Some(scope), &accepted);
        current(&a);
        assert_eq!(b["code"], 0);
        let out = result(&b);
        assert_eq!(out["version"], cold["version"]);
        assert_eq!(out["status"], "current");
        assert_eq!(out["total_matches"], count);
        let mut hits: Vec<_> = out["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h["id"].as_str().unwrap().to_owned())
            .collect();
        hits.sort();
        assert_eq!(
            hits,
            if count == 1 {
                vec![id(4)]
            } else {
                (1..=7).map(id).collect()
            }
        );
    }
    for prefix in ["".to_owned(), id(1)[..8].into(), "missing".into()] {
        let args = vec![
            "search".into(),
            store.to_str().unwrap().into(),
            "login".into(),
            prefix,
        ];
        let r = n
            .launch(Value::Null, "invalid-prefix", Some(args), false)
            .finish();
        assert_eq!(r["code"], 1);
        assert_eq!(r["stdout"], "");
        assert!(
            r["stderr"]
                .as_str()
                .unwrap()
                .contains("retrieval refused (scope: unknown)")
        );
        assert_eq!(files(&store), accepted);
    }
    edit(&store, "Changed Login record");
    let (a, b) = n.competing(&store, "changed-writers", || {}, "normal", None, &accepted);
    let changed = current(&a);
    assert_ne!(changed["version"], cold["version"]);
    assert_eq!(current(&b), changed);
    let (a, b) = n.competing(
        &store,
        "changing-source",
        || edit(&store, "New Login after capture"),
        "normal",
        None,
        &accepted,
    );
    unavailable(&a, "stale");
    let latest = current(&b);
    assert_ne!(latest["version"], changed["version"]);
    assert!(
        latest["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["title"] == "New Login after capture")
    );
    let path = store.join("nodes").join(id(4)).join("node.md");
    let valid = fs::read(&path).unwrap();
    let before = files(&store);
    let (a, b) = n.competing(
        &store,
        "invalid-source",
        || fs::write(&path, "invalid authored draft").unwrap(),
        "normal",
        None,
        &accepted,
    );
    unavailable(&a, "stale");
    unavailable(&b, "error");
    assert_eq!(files(&store).derived, before.derived);
    assert_eq!(fs::read_to_string(&path).unwrap(), "invalid authored draft");
    fs::write(&path, valid).unwrap();
    assert_eq!(current(&n.run(req(&store), "restore-invalid")), latest);
    let before = files(&store);
    let (a, b) = n.competing(&store, "interrupted-writer", || {}, "kill", None, &accepted);
    assert_eq!(a["signal"], 9);
    assert_eq!(current(&b), latest);
    assert_eq!(files(&store), before);
    assert!(!store.join(".sifttext/outline-stage").exists());
    accepted = files(&store);
    let (a, _) = n.competing(
        &store,
        "bounded-contention",
        || {},
        "timeout",
        None,
        &accepted,
    );
    assert_eq!(current(&a), latest);
    assert_eq!(current(&n.run(req(&store), "after-timeout")), latest);
    let stable = files(&store);
    let mut r = req(&store);
    r["query"] = json!("x".repeat(65537));
    rejected(&n.run(r, "resource-refusal"), "resource_exhausted");
    assert_eq!(files(&store), stable);
    let unsafe_stage = store.join(".sifttext/outline-stage");
    fs::create_dir(&unsafe_stage).unwrap();
    symlink(&path, unsafe_stage.join("unsafe-link")).unwrap();
    unavailable(&n.run(req(&store), "unsafe-stage"), "error");
    assert_eq!(files(&store), stable);
    fs::remove_file(unsafe_stage.join("unsafe-link")).unwrap();
    assert_eq!(current(&n.run(req(&store), "recover-unsafe-stage")), latest);
    assert!(!unsafe_stage.exists());
    assert_eq!(files(&store).authored, stable.authored);
    fs::write(evidence.join("result.json"),serde_json::to_vec_pretty(&json!({"status":"PASS","operations":n.calls,"source_version":latest["version"],"limits":["Seven synthetic nodes; no full-Core capacity/latency claim","Event-gated lock/capture order, unchanged five-second busy wait; no fairness/power-loss guarantee","SIGKILL only before acceptance; samples are observational, not continuous peaks"]})).unwrap()).unwrap();
    assert_eq!(n.calls, 28);
}
