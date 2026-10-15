//! Integrity/recovery slice of incremental_retrieval.mjs; warm resource/settings
//! observations are independently exercised by the shared warm consumer.
include!("common/warm_retrieval.rs");
#[path = "common/counted_process.rs"]
mod counted_process;
use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::{fs::symlink, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
fn id(n: usize) -> String {
    format!("60000000-0000-4000-8000-{n:012}")
}
fn node(p: &Path, n: usize) -> PathBuf {
    p.join("nodes").join(id(n))
}
fn db(p: &Path) -> PathBuf {
    p.join(".sifttext/index.sqlite")
}
fn copy(src: &Path, dst: &Path) {
    let meta = fs::symlink_metadata(src).unwrap();
    if meta.file_type().is_symlink() {
        symlink(fs::read_link(src).unwrap(), dst).unwrap();
    } else if meta.is_dir() {
        fs::create_dir(dst).unwrap();
        for e in fs::read_dir(src).unwrap() {
            let e = e.unwrap();
            copy(&e.path(), &dst.join(e.file_name()));
        }
    } else {
        fs::copy(src, dst).unwrap();
    }
}
fn field(p: &Path, n: usize, key: &str, value: Value) {
    let file = node(p, n).join("node.md");
    let text = fs::read_to_string(&file).unwrap();
    let prefix = format!("{key}: ");
    assert_eq!(text.lines().filter(|s| s.starts_with(&prefix)).count(), 1);
    fs::write(
        file,
        text.lines()
            .map(|s| {
                if s.starts_with(&prefix) {
                    format!("{prefix}{value}\n")
                } else {
                    format!("{s}\n")
                }
            })
            .collect::<String>(),
    )
    .unwrap();
}
fn fixture(p: &Path) {
    fs::create_dir_all(p.join("trees")).unwrap();
    for n in 1..=7 {
        let dir = node(p, n);
        fs::create_dir_all(dir.join("children")).unwrap();
        let fields = json!({"id":id(n),"name":if n==5 {"Other root".into()}else{format!("Login {}",n-1)},
            "micro_summary":format!("Summary {} Ω",n-1),"status":"in_progress","scope":"Reset users",
            "see_also":if n==2 {json!([{"target":id(6),"reason":"Cross tree"}])}else{json!([])}});
        let metadata = fields
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| format!("{k}: {v}\n"))
            .collect::<String>();
        fs::write(
            dir.join("node.md"),
            format!("---\n{metadata}---\nLogin reset. Another sentence.\n"),
        )
        .unwrap();
        fs::write(dir.join("children.order"), "").unwrap();
    }
    for (n, children) in [(1, vec![2, 3]), (2, vec![4]), (5, vec![6, 7])] {
        order(p, n, &children);
        for c in children {
            symlink(
                format!("../../{}", id(c)),
                node(p, n).join("children").join(id(c)),
            )
            .unwrap();
        }
    }
    for n in [1, 5] {
        symlink(format!("../nodes/{}", id(n)), p.join("trees").join(id(n))).unwrap();
    }
}
fn order(p: &Path, n: usize, children: &[usize]) {
    fs::write(
        node(p, n).join("children.order"),
        children.iter().map(|n| id(*n) + "\n").collect::<String>(),
    )
    .unwrap();
}
fn artifacts(p: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(p: &Path, root: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for e in fs::read_dir(p).unwrap() {
            let e = e.unwrap();
            let name = e.file_name();
            let name = name.to_str().unwrap();
            let meta = fs::symlink_metadata(e.path()).unwrap();
            if meta.is_dir() && name != "children" {
                walk(&e.path(), root, out);
            } else if meta.is_file() && (name.starts_with("outline.") || name == "index.sqlite") {
                out.insert(
                    e.path().strip_prefix(root).unwrap().into(),
                    fs::read(e.path()).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(&p.join("nodes"), p, &mut out);
    walk(&p.join(".sifttext"), p, &mut out);
    out
}
struct Process(Option<Child>);
impl Drop for Process {
    fn drop(&mut self) {
        if let Some(c) = self.0.as_mut() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}
struct Native {
    bin: PathBuf,
    evidence: PathBuf,
    serial: usize,
}
impl Native {
    fn request(p: &Path) -> Value {
        json!({"op":"search","store":p,"query":"login","under_node_id":null,"readable_ids":(1..=7).map(id).collect::<Vec<_>>(),"limit":3,"offset":0})
    }
    fn start(&mut self, request: &Value) -> (Process, PathBuf, PathBuf) {
        let stdout = self.evidence.join(format!("{}.stdout", self.serial));
        let stderr = self.evidence.join(format!("{}.stderr", self.serial));
        self.serial += 1;
        let mut c = Command::new(&self.bin)
            .env("SIFTTEXT_RETRIEVAL_TRACE", "1")
            .stdin(Stdio::piped())
            .stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap();
        c.stdin
            .take()
            .unwrap()
            .write_all(request.to_string().as_bytes())
            .unwrap();
        (Process(Some(c)), stdout, stderr)
    }
    fn call(&mut self, request: Value, expected: &str) -> Value {
        let start = Instant::now();
        let (mut process, stdout, stderr) = self.start(&request);
        let status = process.0.as_mut().unwrap().wait().unwrap();
        process.0.take();
        let bytes = fs::read(stdout).unwrap();
        let err = fs::read_to_string(stderr).unwrap();
        let result: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        let receipt = json!({"request":request,"exit":status.code(),"signal":status.signal(),"result":result,"stderr":err,"ms":start.elapsed().as_secs_f64()*1000.});
        fs::write(
            self.evidence.join(format!("{}-receipt.json", self.serial)),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
        assert_eq!(
            status.code(),
            Some(if expected == "current" { 0 } else { 1 }),
            "{receipt:#}"
        );
        if expected == "refused" {
            assert_ne!(result["status"], "current", "{receipt:#}");
        } else {
            assert_eq!(result["status"], expected, "{receipt:#}");
        }
        result
    }
    fn search(&mut self, p: &Path) -> Value {
        self.call(Self::request(p), "current")
    }
    fn query(&mut self, p: &Path, query: &str, scope: Option<usize>) -> Value {
        let mut request = Self::request(p);
        request["query"] = json!(query);
        request["under_node_id"] = json!(scope.map(id));
        self.call(request, "current")
    }
    fn equality(&mut self, p: &Path) -> Value {
        let before = self.search(p);
        let files = artifacts(p);
        assert_eq!(
            self.call(json!({"op":"index","store":p,"mode":"full"}), "current")["version"],
            before["version"]
        );
        assert_eq!(self.search(p), before);
        let after = artifacts(p);
        for (path, bytes) in files {
            if path != Path::new(".sifttext/index.sqlite") {
                assert_eq!(after[&path], bytes, "{}", path.display());
            }
        }
        before
    }
    fn paused(&mut self, p: &Path, action: impl FnOnce(), kill: bool, lock_probe: bool) {
        let ready = self.evidence.join(format!("ready-{}", self.serial));
        let release = ready.with_extension("release");
        let mut request = Self::request(p);
        request["pause_before_accept"] = json!({"ready":ready,"release":release});
        let (mut process, stdout, stderr) = self.start(&request);
        let start = Instant::now();
        while !ready.exists() {
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "barrier unavailable: {}",
                fs::read_to_string(&stderr).unwrap()
            );
            assert!(
                process.0.as_mut().unwrap().try_wait().unwrap().is_none(),
                "request exited before barrier"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let mut contender = if lock_probe {
            Some(self.start(&Self::request(p)))
        } else {
            None
        };
        if let Some((child, _, _)) = contender.as_mut() {
            thread::sleep(Duration::from_millis(100));
            assert!(
                child.0.as_mut().unwrap().try_wait().unwrap().is_none(),
                "contender bypassed publisher lock"
            );
        }
        action();
        if kill {
            process.0.as_mut().unwrap().kill().unwrap();
        } else {
            fs::write(&release, "release").unwrap();
        }
        let status = process.0.as_mut().unwrap().wait().unwrap();
        process.0.take();
        let bytes = fs::read(stdout).unwrap();
        let err = fs::read_to_string(stderr).unwrap();
        let result: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        let receipt = json!({"request":request,"exit":status.code(),"signal":status.signal(),"result":result,"stderr":err});
        fs::write(
            self.evidence.join(format!("race-{}.json", self.serial)),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
        if kill {
            assert_eq!(status.signal(), Some(9), "{receipt:#}");
            assert!(bytes.is_empty());
        } else if lock_probe {
            assert!(status.success(), "{receipt:#}");
            assert_eq!(result["status"], "current");
        } else {
            assert!(!status.success(), "{receipt:#}");
            assert_ne!(result["status"], "current", "{receipt:#}");
        }
        if let Some((mut child, out, err)) = contender {
            let status = child.0.as_mut().unwrap().wait().unwrap();
            child.0.take();
            let result: Value = serde_json::from_slice(&fs::read(out).unwrap()).unwrap();
            assert!(status.success(), "{}", fs::read_to_string(err).unwrap());
            assert_eq!(result["status"], "current");
            fs::write(
                self.evidence.join("lock-contender.json"),
                serde_json::to_vec_pretty(&result).unwrap(),
            )
            .unwrap();
        }
    }
}
#[test]
fn native_authored_changes_cache_forgery_sparse_races_and_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("INCREMENTAL_RETRIEVAL_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("evidence"));
    assert!(evidence.is_absolute());
    fs::create_dir(&evidence).unwrap();
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .map(|p| p.join("core-observe"))
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_core-observe")));
    let mut native = Native {
        bin,
        evidence: evidence.clone(),
        serial: 0,
    };
    let store = evidence.join("store");
    fixture(&store);
    native.search(&store);
    field(&store, 2, "see_also", json!([]));
    native.equality(&store);
    assert_eq!(
        Connection::open(db(&store))
            .unwrap()
            .query_row("SELECT count(*) FROM links", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    field(&store, 4, "name", json!("A unique rename"));
    assert_eq!(
        native.query(&store, "unique", None)["results"][0]["id"],
        id(4)
    );
    native.equality(&store);
    fs::rename(
        node(&store, 2).join("children").join(id(4)),
        node(&store, 3).join("children").join(id(4)),
    )
    .unwrap();
    order(&store, 2, &[]);
    order(&store, 3, &[4]);
    assert_eq!(native.query(&store, "unique", Some(2))["total_matches"], 0);
    assert_eq!(
        native.query(&store, "unique", Some(3))["results"][0]["id"],
        id(4)
    );
    native.equality(&store);
    order(&store, 1, &[3, 2]);
    native.equality(&store);
    // Sorted UUID insertion/removal changes posting suffixes; retained rows must stay correct.
    let addition = evidence.join("addition");
    fixture(&addition);
    copy(&node(&addition, 4), &node(&addition, 8));
    field(&addition, 8, "id", json!(id(8)));
    field(&addition, 8, "name", json!("Extra addition"));
    symlink(
        format!("../../{}", id(8)),
        node(&addition, 5).join("children").join(id(8)),
    )
    .unwrap();
    order(&addition, 5, &[6, 7, 8]);
    let mut request = Native::request(&addition);
    request["query"] = json!("addition");
    request["readable_ids"] = json!((1..=8).map(id).collect::<Vec<_>>());
    assert_eq!(native.call(request.clone(), "current")["total_matches"], 1);
    fs::remove_dir_all(node(&addition, 8)).unwrap();
    fs::remove_file(node(&addition, 5).join("children").join(id(8))).unwrap();
    order(&addition, 5, &[6, 7]);
    assert_eq!(native.call(request, "current")["total_matches"], 0);
    let expected = native.equality(&store);
    for mutation in [
        "record-column",
        "posting",
        "missing-fts",
        "incompatible-fts",
    ] {
        let c = Connection::open(db(&store)).unwrap();
        match mutation {
            "record-column" => {
                c.execute("UPDATE nodes SET name='forged'", []).unwrap();
            }
            "posting" => {
                c.execute("UPDATE search_fts SET title='forged' WHERE rowid=1", [])
                    .unwrap();
            }
            "missing-fts" => {
                c.execute_batch("DROP TABLE search_fts").unwrap();
            }
            _ => {
                c.execute_batch(
                    "DROP TABLE search_fts; CREATE TABLE search_fts(rowid INTEGER,title TEXT)",
                )
                .unwrap();
            }
        }
        drop(c);
        assert_eq!(native.search(&store), expected, "{mutation}");
    }
    let outline = node(&store, 1).join("outline.full.lean");
    let c = Connection::open(db(&store)).unwrap();
    let text: String = c
        .query_row("SELECT value FROM metadata WHERE key='outlines'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let mut manifest: Value = serde_json::from_str(&text).unwrap();
    fs::write(
        &outline,
        fs::read_to_string(&outline)
            .unwrap()
            .replace("Summary", "Forged"),
    )
    .unwrap();
    manifest["files"][format!("nodes/{}/outline.full.lean", id(1))] =
        json!(format!("{:x}", Sha256::digest(fs::read(&outline).unwrap())));
    c.execute(
        "UPDATE metadata SET value=?1 WHERE key='outlines'",
        [manifest.to_string()],
    )
    .unwrap();
    drop(c);
    assert_eq!(native.search(&store), expected);
    assert!(!fs::read_to_string(&outline).unwrap().contains("Forged"));
    fs::remove_file(db(&store)).unwrap();
    assert_eq!(native.search(&store), expected);
    let corrupt = evidence.join("corrupt");
    copy(&store, &corrupt);
    fs::write(db(&corrupt), "corrupt").unwrap();
    native.call(Native::request(&corrupt), "refused");
    let source = node(&store, 4).join("node.md");
    let valid = fs::read(&source).unwrap();
    let files = artifacts(&store);
    fs::write(&source, "invalid draft").unwrap();
    assert_eq!(
        native.call(Native::request(&store), "error"),
        json!({"status":"error","version":null,"total_matches":0,"results":[],"other_groups":[]})
    );
    assert_eq!(artifacts(&store), files);
    fs::write(&source, &valid).unwrap();
    assert_eq!(native.search(&store), expected);
    let isolated = evidence.join("isolated");
    copy(&store, &isolated);
    field(&isolated, 4, "name", json!("Isolated unique"));
    assert_ne!(
        native.search(&isolated)["version"],
        native.search(&store)["version"]
    );
    let sparse = evidence.join("sparse");
    copy(&store, &sparse);
    fs::remove_dir_all(node(&sparse, 4)).unwrap();
    let mut request = Native::request(&sparse);
    request["base_store"] = json!(store);
    assert_eq!(native.call(request.clone(), "current"), expected);
    copy(&node(&store, 4), &node(&sparse, 4));
    assert_eq!(native.call(request, "current"), expected);
    native.paused(
        &store,
        || field(&store, 4, "name", json!("Race changed")),
        false,
        false,
    );
    assert_eq!(native.query(&store, "race", None)["total_matches"], 1);
    native.paused(
        &store,
        || fs::write(&outline, fs::read(&outline).unwrap()).unwrap(),
        false,
        false,
    );
    native.search(&store);
    native.paused(
        &store,
        || fs::rename(db(&store), store.join(".sifttext/old-cache.sqlite")).unwrap(),
        false,
        false,
    );
    native.search(&store);
    native.paused(&store, || {}, true, false);
    native.search(&store);
    field(&store, 4, "name", json!("Interrupted changed record"));
    native.paused(&store, || {}, true, false);
    native.search(&store);
    native.paused(&store, || {}, false, true);
    native.equality(&store);
    fs::write(evidence.join("integrity-passed.json"),serde_json::to_vec_pretty(&json!({"requests_started":native.serial,
        "scope":"7/8-node synthetic changes, full equality, cache/outline forgery, sparse isolation, acceptance races, killed refresh and lock blocking",
        "remaining":"warm identity/no-staging/resource benchmarks and complete query/settings matrix remain in MJS; no legacy retirement or universal crash/capacity claim"})).unwrap()).unwrap();
}

#[test]
fn counted_process_stop_timeout_and_drain() {
    const CHILD: &str = "SIFTTEXT_COUNTED_PROCESS_CONTROL";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let mode = std::env::var("SIFTTEXT_COUNTED_PROCESS_MODE").unwrap();
        let bound = if mode == "timeout-tree" {
            Duration::from_millis(100)
        } else if mode == "timeout" {
            Duration::from_millis(50)
        } else {
            Duration::from_secs(10)
        };
        let mut producer = Command::new("/bin/sh");
        if mode == "timeout-tree" {
            producer
                .args([
                    "-c",
                    "trap '' TERM INT; sleep 60 & printf '%s' \"$!\" > \"$1\"; wait",
                    "counted-tree",
                ])
                .arg(root.join("descendant.pid"));
        } else {
            producer.args(["-c", "exec sleep 60"]);
        }
        counted_process::run(&mut producer, bound, &root.join("producer"));
        panic!("control incorrectly succeeded");
    }
    let temp = tempfile::tempdir().unwrap();
    for (mode, signal) in [
        ("timeout", 0),
        ("timeout-tree", 0),
        ("sigterm", libc::SIGTERM),
        ("sigint", libc::SIGINT),
    ] {
        let root = temp.path().join(mode);
        fs::create_dir(&root).unwrap();
        let watched = root.clone();
        let notifier = thread::spawn(move || {
            if signal == 0 {
                return;
            }
            let started = Instant::now();
            while !watched.join("producer.start.json").exists() {
                assert!(
                    started.elapsed() < Duration::from_secs(2),
                    "producer never started"
                );
                thread::sleep(Duration::from_millis(5));
            }
            let child: Value =
                serde_json::from_slice(&fs::read(watched.join("control.start.json")).unwrap())
                    .unwrap();
            assert_eq!(
                unsafe { libc::kill(child["pid"].as_u64().unwrap() as i32, signal) },
                0
            );
        });
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "counted_process_stop_timeout_and_drain",
                "--nocapture",
            ])
            .env(CHILD, &root)
            .env("SIFTTEXT_COUNTED_PROCESS_MODE", mode)
            .stdin(Stdio::null())
            .stdout(fs::File::create(root.join("stdout")).unwrap())
            .stderr(fs::File::create(root.join("stderr")).unwrap());
        let (status, _) =
            counted_process::run(&mut command, Duration::from_secs(5), &root.join("control"));
        notifier.join().unwrap();
        assert!(!status.success(), "cancelled child passed");
        let receipt: Value =
            serde_json::from_slice(&fs::read(root.join("producer.process.json")).unwrap()).unwrap();
        eprintln!("counted-process-control {receipt}");
        assert_eq!(receipt["drained"], true);
        assert_eq!(receipt["cancelled_signal"], signal);
        assert_eq!(receipt["runner_timeout"], signal == 0);
        if mode == "timeout-tree" {
            assert_eq!(receipt["signal"], libc::SIGKILL);
            let descendant: i32 = fs::read_to_string(root.join("descendant.pid"))
                .unwrap()
                .parse()
                .unwrap();
            eprintln!("counted-process-tree descendant={descendant} expected=ESRCH");
            assert_eq!(unsafe { libc::kill(descendant, 0) }, -1);
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ESRCH)
            );
        }
        assert_eq!(
            unsafe { libc::kill(-(receipt["pgid"].as_u64().unwrap() as i32), 0) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }
}

// This is a counted observation, not a speed threshold or a durable proof cache.
// Each child calls the observer's search API in a fresh process, but does not
// qualify the observer JSON transport or its embedded policy-home selection.
#[test]
fn fresh_process_warm_search_counts_real_graph_checks() {
    const CHILD: &str = "SIFTTEXT_COUNTED_SEARCH_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let policy = sifttext_file_graph::Policy::checked(&root.join("policy")).unwrap();
        let store = root.join("store");
        let size = std::env::var("SIFTTEXT_COUNTED_SEARCH_SIZE")
            .map(|s| s.parse::<usize>().unwrap())
            .unwrap_or(7);
        let readable = (1..=size).map(id).collect();
        let outcome = sifttext_file_graph::search::search(
            &policy,
            &sifttext_file_graph::search::Request {
                store: &store,
                base_store: None,
                query: "login",
                under_node_id: None,
                readable_ids: &readable,
                limit: 3,
                offset: 0,
                ranking: &Default::default(),
                pause_before_accept: None,
            },
        )
        .unwrap();
        fs::write(
            root.join("result.json"),
            serde_json::to_vec(&outcome).unwrap(),
        )
        .unwrap();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("COUNTED_WARM_SEARCH_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("evidence"));
    assert!(root.is_absolute());
    fs::create_dir(&root).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let real_hash = counted_policy(&root, false);
    let project = root.join("policy");
    let store = root.join("store");
    fixture(&store);
    let mut receipts = vec![];
    let mut query = |label: &str| {
        let calls = project.join("build/calls");
        fs::write(&calls, "").unwrap();
        let result = root.join("result.json");
        if result.exists() {
            fs::remove_file(&result).unwrap();
        }
        let stdout = root.join(format!("{label}.stdout"));
        let stderr = root.join(format!("{label}.stderr"));
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "fresh_process_warm_search_counts_real_graph_checks",
                "--nocapture",
            ])
            .env(CHILD, &root)
            .env("SIFTTEXT_RETRIEVAL_TRACE", "1")
            .stdin(Stdio::null())
            .stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap());
        let (status, process) =
            counted_process::run(&mut command, Duration::from_secs(60), &root.join(label));
        let count = fs::read_to_string(&calls).unwrap().lines().count();
        let output: Value = if result.exists() {
            serde_json::from_slice(&fs::read(result).unwrap()).unwrap()
        } else {
            Value::Null
        };
        let receipt = json!({"label":label,"process":process,"exit":status.code(),"signal":status.signal(),
            "ms":process["ms"],"helper_calls":count,"outcome":output,
            "stderr":fs::read_to_string(stderr).unwrap(),"real_helper_sha256":real_hash});
        receipts.push(receipt.clone());
        fs::write(
            root.join("receipts.json"),
            serde_json::to_vec_pretty(&receipts).unwrap(),
        )
        .unwrap();
        assert!(status.success(), "{receipt:#}");
        assert_eq!(count, 1, "{receipt:#}");
        assert_eq!(
            sifttext_file_graph::sha256(
                &fs::read(project.join("build/graph-policy.real")).unwrap()
            ),
            real_hash
        );
        output
    };
    let cold = query("cold");
    assert_eq!(cold["status"], "current");
    assert_eq!(cold["total_matches"], 7);
    assert_eq!(cold["hits"][0]["id"], id(1));
    let before = identities(&store);
    for label in ["warm-first", "warm-second"] {
        assert_eq!(query(label), cold);
        assert_eq!(identities(&store), before);
    }
    let db = Connection::open(db(&store)).unwrap();
    db.execute_batch("UPDATE nodes SET name='forged'").unwrap();
    drop(db);
    assert_eq!(query("forged-cache"), cold);
    assert_eq!(query("repaired-warm"), cold);
    let accepted = artifacts(&store);
    // Two parents for one child pass physical membership but fail the real law.
    symlink(
        format!("../../{}", id(4)),
        node(&store, 3).join("children").join(id(4)),
    )
    .unwrap();
    order(&store, 3, &[4]);
    let refused = query("invalid-authored-graph");
    assert_eq!(refused["status"], "error");
    assert!(refused["version"].is_null());
    assert_eq!(refused["total_matches"], 0);
    assert!(refused["hits"].as_array().unwrap().is_empty());
    assert_eq!(artifacts(&store), accepted);
    fs::remove_file(node(&store, 3).join("children").join(id(4))).unwrap();
    order(&store, 3, &[]);
    assert_eq!(query("restored"), cold);
    sifttext_file_graph::Policy::checked(source).unwrap();
    assert_eq!(
        sifttext_file_graph::sha256(&fs::read(source.join("build/graph-policy")).unwrap()),
        real_hash
    );
}

fn counted_policy(root: &Path, record_pid: bool) -> String {
    let project = root.join("policy");
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    sifttext_file_graph::Policy::checked(source).unwrap();
    // Copy only the complete checked source set; never restamp the donor.
    let manifest = fs::read_to_string(source.join("build/candidate.sha256")).unwrap();
    let mut files = manifest
        .lines()
        .map(|line| line.split_once("  ").unwrap().1)
        .collect::<Vec<_>>();
    files.extend([
        "build/candidate.sha256",
        "build/graph-policy.sha256",
        "build/graph-policy",
    ]);
    for file in files {
        assert!(!Path::new(file).is_absolute() && !file.split('/').any(|c| c == ".."));
        let target = project.join(file);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source.join(file), target).unwrap();
    }
    let helper = project.join("build/graph-policy");
    let real = project.join("build/graph-policy.real");
    fs::rename(&helper, &real).unwrap();
    let real_hash = sifttext_file_graph::sha256(&fs::read(&real).unwrap());
    let pid = if record_pid {
        "printf '%s' \"$$\" > \"$d/helper.pid\"\n"
    } else {
        ""
    };
    fs::write(&helper, format!("#!/bin/sh\nset -eu\nd=$(dirname \"$0\")\nprintf 'call\\n' >> \"$d/calls\"\n{pid}exec \"$d/graph-policy.real\" \"$@\"\n")).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        project.join("build/graph-policy.sha256"),
        format!(
            "{}  build/graph-policy\n",
            sifttext_file_graph::sha256(&fs::read(&helper).unwrap())
        ),
    )
    .unwrap();
    real_hash
}

#[test]
fn counted_search_real_helper_stop_and_drain() {
    const CHILD: &str = "SIFTTEXT_COUNTED_REAL_STOP_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let mut search = Command::new(std::env::current_exe().unwrap());
        search
            .args([
                "--exact",
                "fresh_process_warm_search_counts_real_graph_checks",
                "--nocapture",
            ])
            .env("SIFTTEXT_COUNTED_SEARCH_CHILD", &root)
            .env("SIFTTEXT_COUNTED_SEARCH_SIZE", "512")
            .stdout(fs::File::create(root.join("search.stdout")).unwrap())
            .stderr(fs::File::create(root.join("search.stderr")).unwrap());
        counted_process::run(&mut search, Duration::from_secs(10), &root.join("search"));
        panic!("stopped search incorrectly succeeded");
    }
    let temp = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("COUNTED_REAL_STOP_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("evidence"));
    fs::create_dir(&evidence).unwrap();
    for signal in [libc::SIGTERM, libc::SIGINT] {
        let root = evidence.join(signal.to_string());
        fs::create_dir(&root).unwrap();
        let helper_hash = counted_policy(&root, true);
        let store = root.join("store");
        fixture(&store);
        for n in 8..=512 {
            copy(&node(&store, 4), &node(&store, n));
            field(&store, n, "id", json!(id(n)));
            symlink(
                format!("../../{}", id(n)),
                node(&store, 1).join("children").join(id(n)),
            )
            .unwrap();
        }
        order(&store, 1, &[vec![2, 3], (8..=512).collect()].concat());
        let watched = root.clone();
        let notifier = thread::spawn(move || {
            let start = Instant::now();
            loop {
                assert!(
                    start.elapsed() < Duration::from_secs(5),
                    "real helper never observed"
                );
                if let Ok(pid) = fs::read_to_string(watched.join("policy/build/helper.pid")) {
                    let output = Command::new("/bin/ps")
                        .args(["-p", &pid, "-o", "pgid=,args="])
                        .output()
                        .unwrap();
                    let text = String::from_utf8(output.stdout).unwrap();
                    if text.contains("graph-policy.real") {
                        let pgid: i32 = text.split_whitespace().next().unwrap().parse().unwrap();
                        fs::write(watched.join("observed-helper.txt"), &text).unwrap();
                        let child: Value = serde_json::from_slice(
                            &fs::read(watched.join("envelope.start.json")).unwrap(),
                        )
                        .unwrap();
                        let signalled = Instant::now();
                        assert_eq!(
                            unsafe { libc::kill(child["pid"].as_u64().unwrap() as i32, signal) },
                            0
                        );
                        return (pid.parse::<i32>().unwrap(), pgid, signalled);
                    }
                }
                thread::sleep(Duration::from_millis(5));
            }
        });
        let mut envelope = Command::new(std::env::current_exe().unwrap());
        envelope
            .args([
                "--exact",
                "counted_search_real_helper_stop_and_drain",
                "--nocapture",
            ])
            .env(CHILD, &root)
            .stdout(fs::File::create(root.join("envelope.stdout")).unwrap())
            .stderr(fs::File::create(root.join("envelope.stderr")).unwrap());
        let (status, _) = counted_process::run(
            &mut envelope,
            Duration::from_secs(15),
            &root.join("envelope"),
        );
        let (helper, pgid, signalled) = notifier.join().unwrap();
        assert!(!status.success());
        let receipt: Value =
            serde_json::from_slice(&fs::read(root.join("search.process.json")).unwrap()).unwrap();
        assert_eq!(receipt["cancelled_signal"], signal);
        assert_eq!(receipt["runner_timeout"], false);
        assert_eq!(receipt["drained"], true);
        for pid in [helper, -pgid, -(receipt["pgid"].as_u64().unwrap() as i32)] {
            assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ESRCH)
            );
        }
        let stop_ms = signalled.elapsed().as_secs_f64() * 1000.;
        assert!(stop_ms < 1000., "Stop exceeded launcher grace: {stop_ms}ms");
        assert!(!root.join("result.json").exists());
        // Opening the publisher lock may leave an empty SQLite file; that is
        // not publication. No accepted-version marker may survive cancellation.
        if db(&store).exists() {
            let db = Connection::open(db(&store)).unwrap();
            let tables: i64 = db
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='metadata'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            if tables != 0 {
                let versions: i64 = db
                    .query_row(
                        "SELECT count(*) FROM metadata WHERE key='version'",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(versions, 0);
            }
        }
        assert_eq!(
            sifttext_file_graph::sha256(
                &fs::read(root.join("policy/build/graph-policy.real")).unwrap()
            ),
            helper_hash
        );
        fs::write(root.join("stop.json"), serde_json::to_vec_pretty(&json!({"signal":signal,"helper_pid":helper,"helper_pgid":pgid,"stop_ms":stop_ms,"group_absence":"ESRCH","process":receipt,"real_helper_sha256":helper_hash})).unwrap()).unwrap();
        eprintln!("real-helper Stop signal={signal} drained in {stop_ms:.2}ms");
    }
}
