use crate::{Policy, index::Refusal, search};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    os::unix::{
        fs::PermissionsExt,
        process::{CommandExt, ExitStatusExt},
    },
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
const PRIVATE: &str = "PRIVATE-refusal-sentinel";
fn id(n: usize) -> String {
    format!("59000000-0000-4000-8000-{n:012}")
}
fn fixture(count: usize) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir(d.path().join("trees")).unwrap();
    for n in 1..=count {
        let p = d.path().join("nodes").join(id(n));
        fs::create_dir_all(p.join("children")).unwrap();
        fs::write(p.join("node.md"), format!("---\nid: {:?}\nname: \"Useful\"\nmicro_summary: \"Useful summary\"\nstatus: \"in_progress\"\nsee_also: []\n---\n{PRIVATE}\n", id(n))).unwrap();
        // Keep the larger real-helper specimen alive long enough to inject;
        // one/two-node refusal specimens retain their original topology.
        let children: Vec<_> = if n < count { vec![n + 1] } else { vec![] };
        fs::write(
            p.join("children.order"),
            children.iter().map(|n| id(*n) + "\n").collect::<String>(),
        )
        .unwrap();
        for child in children {
            std::os::unix::fs::symlink(
                format!("../../{}", id(child)),
                p.join("children").join(id(child)),
            )
            .unwrap();
        }
    }
    std::os::unix::fs::symlink(
        format!("../nodes/{}", id(1)),
        d.path().join("trees").join(id(1)),
    )
    .unwrap();
    d
}
fn source(p: &Path) -> PathBuf {
    p.join("nodes").join(id(1)).join("node.md")
}
fn request(p: &Path, query: &str) -> Value {
    json!({"op":"search","store":p,"query":query,"limit":10,"offset":0,"readable_ids":[id(1)],"diagnostics":true})
}
fn binary(name: &str) -> PathBuf {
    crate::native_bin_dir()
        .join(name)
}
fn command() -> Command {
    let mut c = Command::new(binary("core-observe"));
    c.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c
}
fn run(req: &Value) -> Output {
    let mut child = command().spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(req).unwrap().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn refusal(label: &str, out: &Output, p: &Path, stage: &str, kind: &str) {
    let record = json!({"case":label,"exit":out.status.code(),"stdout":String::from_utf8_lossy(&out.stdout),"stderr":String::from_utf8_lossy(&out.stderr)});
    if let Some(e) = std::env::var_os("RETRIEVAL_REFUSALS_EVIDENCE") {
        fs::create_dir_all(&e).unwrap();
        fs::write(
            Path::new(&e).join(format!("{label}.json")),
            serde_json::to_vec_pretty(&record).unwrap(),
        )
        .unwrap();
    }
    println!("{record}");
    assert_eq!(out.status.code(), Some(1));
    for text in [&out.stdout, &out.stderr] {
        let text = String::from_utf8_lossy(text);
        assert!(!text.contains(PRIVATE));
        assert!(!text.contains(p.to_str().unwrap()));
    }
    let status = if stage == "source_freshness" {
        "stale"
    } else {
        "error"
    };
    let v: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(v, json!({"status":status,"stage":stage,"kind":kind}));
    if !out.stdout.is_empty() {
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["status"], status);
        assert!(v["version"].is_null());
        assert_eq!(v["results"], json!([]));
    }
}
#[test]
fn library_carries_refusals_without_changing_legacy_success_or_unavailable_json() {
    let d = fixture(1);
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let readable = BTreeSet::from([id(1)]);
    let ranking = search::Ranking::default();
    let req = search::Request {
        store: d.path(),
        base_store: None,
        query: "Useful",
        under_node_id: None,
        readable_ids: &readable,
        limit: 10,
        offset: 0,
        ranking: &ranking,
        pause_before_accept: None,
    };
    let out = search::search(&policy, &req).unwrap();
    assert_eq!(out.status, "current");
    assert_eq!(out.total_matches, 1);
    assert!(out.refusal.is_none());
    let original = fs::read(source(d.path())).unwrap();
    fs::write(
        source(d.path()),
        String::from_utf8(original.clone())
            .unwrap()
            .replace("Useful summary", ""),
    )
    .unwrap();
    let out = search::search(&policy, &req).unwrap();
    assert_eq!(out.status, "error");
    assert_eq!(out.refusal.unwrap().stage, "source_capture");
    assert_eq!(
        search::observation(&out),
        json!({"status":"error","version":null,"total_matches":0,"results":[],"other_groups":[]})
    );
    fs::write(source(d.path()), &original).unwrap();
    assert_eq!(search::search(&policy, &req).unwrap().status, "current");
    let unknown = Refusal::from_error("source_capture", &std::io::Error::other(PRIVATE));
    assert_eq!(unknown.kind, "io");
    assert!(!unknown.to_string().contains(PRIVATE));
    let unknown: Box<dyn std::error::Error> = "arbitrary private failure".into();
    assert_eq!(
        Refusal::from_error("query", unknown.as_ref()).kind,
        "unknown"
    );
}
#[test]
fn capture_permission_resource_and_cache_contention_remain_failures_and_recover() {
    let d = fixture(1);
    let req = request(d.path(), "");
    assert!(run(&req).status.success());
    let missing = d.path().join(PRIVATE);
    refusal(
        "missing",
        &run(&request(&missing, "")),
        d.path(),
        "source_capture",
        "not_found",
    );
    let original = fs::read(source(d.path())).unwrap();
    fs::set_permissions(source(d.path()), fs::Permissions::from_mode(0o0)).unwrap();
    assert!(
        fs::File::open(source(d.path())).is_err(),
        "permission denial must actually occur"
    );
    refusal(
        "permission",
        &run(&req),
        d.path(),
        "source_capture",
        "permission_denied",
    );
    fs::set_permissions(source(d.path()), fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(source(d.path()), vec![b'x'; 1_048_577]).unwrap();
    refusal(
        "resource",
        &run(&req),
        d.path(),
        "source_capture",
        "unsafe_or_oversized_source",
    );
    fs::write(source(d.path()), original).unwrap();
    let db = rusqlite::Connection::open(d.path().join(".sifttext/index.sqlite")).unwrap();
    db.execute_batch("BEGIN IMMEDIATE").unwrap();
    refusal("contention", &run(&req), d.path(), "index_cache", "busy");
    db.execute_batch("ROLLBACK").unwrap();
    assert!(run(&req).status.success());
}
#[test]
fn real_checked_helper_failure_and_timeout_are_preserved_not_guessed() {
    for (signal, kind) in [(libc::SIGSTOP, "timeout"), (libc::SIGKILL, "helper_failed")] {
        let d = fixture(256);
        let mut c = command();
        c.process_group(0);
        let mut child = c.spawn().unwrap();
        struct Group(i32);
        impl Drop for Group {
            fn drop(&mut self) {
                unsafe {
                    libc::kill(-self.0, libc::SIGKILL);
                }
            }
        }
        let group = Group(child.id() as i32);
        child
            .stdin
            .take()
            .unwrap()
            .write_all(
                serde_json::to_string(&request(d.path(), ""))
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            assert!(
                child.try_wait().unwrap().is_none(),
                "helper escaped injection"
            );
            assert!(Instant::now() < deadline);
            let p = Command::new("pgrep")
                .args(["-P", &child.id().to_string()])
                .output()
                .unwrap();
            if let Some(pid) = String::from_utf8_lossy(&p.stdout)
                .lines()
                .next()
                .and_then(|x| x.parse::<i32>().ok())
            {
                assert_eq!(unsafe { libc::kill(pid, signal) }, 0);
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        refusal(
            kind,
            &child.wait_with_output().unwrap(),
            d.path(),
            "source_validation",
            kind,
        );
        drop(group);
        assert!(run(&request(d.path(), "")).status.success());
    }
}

// Mirrors every native/API/privacy/recovery assertion in the retired MJS runner.
#[test]
fn native_search_browse_diagnostics_nonpublication_and_recovery() {
    let d = fixture(2);
    let p = source(d.path());
    let original = fs::read(&p).unwrap();
    let cache = d.path().join(".sifttext/index.sqlite");
    let mut req = request(d.path(), "Useful");
    req["readable_ids"] = json!([id(1), id(2)]);
    let first = run(&req);
    assert!(first.status.success());
    assert!(first.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&first.stdout).unwrap()["total_matches"],
        2
    );
    let mut legacy = req.clone();
    legacy["diagnostics"] = json!(false);
    let legacy = run(&legacy);
    assert!(legacy.status.success());
    assert_eq!(legacy.stdout, first.stdout);
    let mut browse = req.clone();
    browse["query"] = json!("");
    let out = run(&browse);
    assert!(out.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["total_matches"],
        2
    );
    let cached = fs::read(&cache).unwrap();

    fs::write(
        &p,
        String::from_utf8(original.clone()).unwrap().replace(
            "status: \"in_progress\"",
            "status: \"stuck\"\nblocker: \"\"",
        ),
    )
    .unwrap();
    refusal(
        "invalid-status",
        &run(&browse),
        d.path(),
        "source_validation",
        "unknown",
    );
    assert_eq!(fs::read(&cache).unwrap(), cached);
    let cli = |stage: &str| {
        let out = Command::new(binary("sifttext"))
            .arg("search")
            .arg(d.path())
            .arg("Useful")
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1));
        for text in [&out.stdout, &out.stderr] {
            let text = String::from_utf8_lossy(text);
            assert!(!text.contains(PRIVATE));
            assert!(!text.contains(d.path().to_str().unwrap()));
        }
        assert!(String::from_utf8_lossy(&out.stderr).contains(&format!("{stage}: unknown")));
    };
    cli("source_validation");
    fs::write(&p, &original).unwrap();
    assert_eq!(run(&req).stdout, first.stdout);
    fs::write(&p, format!("malformed {PRIVATE}")).unwrap();
    refusal(
        "malformed-browse",
        &run(&browse),
        d.path(),
        "source_capture",
        "unknown",
    );
    let candidate = run(&json!({"op":"validate_candidate","store":d.path(),"diagnostics":true}));
    assert_eq!(candidate.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&candidate.stdout).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&candidate.stderr).unwrap(),
        json!({"status":report["status"],"kind":report["failure"]["kind"],"stage":report["failure"]["stage"]})
    );
    let diagnostic = String::from_utf8_lossy(&candidate.stderr);
    assert!(!diagnostic.contains(PRIVATE));
    assert!(!diagnostic.contains(d.path().to_str().unwrap()));
    let mut missing = req.clone();
    missing["store"] = json!(d.path().join("missing-private"));
    refusal(
        "missing-search",
        &run(&missing),
        d.path(),
        "source_capture",
        "not_found",
    );
    cli("source_capture");
    fs::write(&p, &original).unwrap();
    let mut scope = req.clone();
    scope["under_node_id"] = json!(PRIVATE);
    refusal(
        "invalid-scope",
        &run(&scope),
        d.path(),
        "operation",
        "unknown",
    );
    let mut flag = req.clone();
    flag["diagnostics"] = json!(PRIVATE);
    let out = run(&flag);
    assert_eq!(out.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&out.stderr).contains(PRIVATE));

    // Own and reap a paused native child even if the readiness assertion panics.
    let paused = |kill: bool| {
        let barrier = tempfile::tempdir().unwrap();
        let ready = barrier.path().join("ready");
        let release = barrier.path().join("release");
        let mut q = browse.clone();
        q["pause_before_accept"] = json!({"ready":ready,"release":release});
        let mut child = command().spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&q).unwrap())
            .unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let deadline = Instant::now() + Duration::from_secs(20);
            while !ready.exists() {
                assert!(
                    child.try_wait().unwrap().is_none(),
                    "native exited before barrier"
                );
                assert!(Instant::now() < deadline, "barrier not reached");
                thread::sleep(Duration::from_millis(5));
            }
            if kill {
                child.kill().unwrap();
            } else {
                let mut changed = original.clone();
                changed.extend_from_slice(b"\nchanged");
                fs::write(&p, changed).unwrap();
                fs::write(&release, "release").unwrap();
            }
        }));
        if result.is_err() {
            let _ = child.kill();
        }
        let out = child.wait_with_output().unwrap();
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
        out
    };
    refusal(
        "freshness",
        &paused(false),
        d.path(),
        "source_freshness",
        "source_changed",
    );
    assert_eq!(
        fs::read(&cache).unwrap(),
        cached,
        "stale query published index"
    );
    fs::write(&p, &original).unwrap();
    assert_eq!(run(&req).stdout, first.stdout);
    assert_eq!(paused(true).status.signal(), Some(libc::SIGKILL));
    assert_eq!(
        fs::read(&cache).unwrap(),
        cached,
        "killed query published index"
    );
    assert_eq!(run(&req).stdout, first.stdout);
    fs::remove_file(&cache).unwrap();
    assert_eq!(run(&req).stdout, first.stdout);
    assert_eq!(fs::read(&p).unwrap(), original);
}
