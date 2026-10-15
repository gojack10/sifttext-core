//! Real observer process contracts; synthetic private strings must not escape failures.
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

const PRIVATE: &str = "PRIVATE-FIXTURE-TEXT-not-for-diagnostics";
fn id(n: usize) -> String {
    format!("50000000-0000-4000-8000-{n:012}")
}
fn fixture(count: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("trees")).unwrap();
    for n in 1..=count {
        let p = dir.path().join("nodes").join(id(n));
        fs::create_dir_all(p.join("children")).unwrap();
        let fields = json!({"id":id(n),"name":format!("Node {n}"),
            "micro_summary":"Useful summary", "status":"in_progress", "see_also":[]});
        let text = "---\n".to_owned()
            + &fields
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| format!("{k}: {v}\n"))
                .collect::<String>()
            + "---\n"
            + PRIVATE
            + "\n";
        fs::write(p.join("node.md"), text).unwrap();
        let children: Vec<_> = if n == 1 {
            (2..=count).collect()
        } else {
            vec![]
        };
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
        dir.path().join("trees").join(id(1)),
    )
    .unwrap();
    dir
}
fn source(store: &Path) -> PathBuf {
    store.join("nodes").join(id(1)).join("node.md")
}
fn binary() -> PathBuf {
    std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug"))
        .join("core-observe")
}
fn command() -> Command {
    assert!(
        binary().is_file(),
        "build --locked --bins before these process tests"
    );
    let mut c = Command::new(binary());
    c.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c
}
fn spawn(c: &mut Command, bytes: &[u8]) -> Child {
    let mut child = c.spawn().unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    child
}
fn run(request: &Value) -> Output {
    spawn(&mut command(), &serde_json::to_vec(request).unwrap())
        .wait_with_output()
        .unwrap()
}
fn index(store: &Path) -> Value {
    json!({"op":"index","store":store,"mode":"incremental"})
}
fn search(store: &Path) -> Value {
    json!({"op":"search","store":store,"query":"Useful","limit":10,"offset":0,"readable_ids":[id(1),id(2)]})
}
fn outline(store: &Path) -> Value {
    json!({"op":"outline_inspect","store":store})
}
fn token(store: &Path) -> Value {
    json!({"op":"token_refresh","path":source(store)})
}
fn record(name: &str, out: &Output) {
    let trace = json!({"case":name,"exit_code":out.status.code(),"success":out.status.success(),
        "stdout":String::from_utf8_lossy(&out.stdout),"stderr":String::from_utf8_lossy(&out.stderr)});
    if let Some(dir) = std::env::var_os("CLI_OUTCOMES_EVIDENCE") {
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            Path::new(&dir).join(format!("{name}.json")),
            serde_json::to_vec_pretty(&trace).unwrap(),
        )
        .unwrap();
    }
    println!("{trace}");
}
fn current(name: &str, out: &Output) -> Value {
    record(name, out);
    assert!(out.status.success());
    assert!(out.stderr.is_empty());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["status"] == "current" || (v["status"].is_null() && v["neighborhood"].is_object()));
    v
}
fn failed(name: &str, out: &Output, forbidden: &Path, status: &str, kind: &str) -> Option<Value> {
    record(name, out);
    assert_eq!(out.status.code(), Some(1));
    for bytes in [&out.stdout, &out.stderr] {
        let s = String::from_utf8_lossy(bytes);
        assert!(!s.contains(PRIVATE), "failure leaked authored text");
        assert!(
            !s.contains(forbidden.to_str().unwrap()),
            "failure leaked private path"
        );
    }
    let diagnostic: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(diagnostic, json!({"status":status,"kind":kind}));
    if out.stdout.is_empty() {
        return None;
    }
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], status);
    assert_ne!(v["status"], "current");
    if v.get("version").is_some() {
        assert!(v["version"].is_null());
    }
    for key in ["nodes", "links", "results", "files"] {
        if let Some(rows) = v.get(key) {
            assert_eq!(rows, &json!([]));
        }
    }
    Some(v)
}
fn await_ready(child: &mut Child, ready: &Path) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !ready.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "observer exited before barrier"
        );
        assert!(Instant::now() < deadline, "barrier not reached");
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn current_operations_exit_zero_and_library_unavailable_remains_inspectable() {
    let store = fixture(2);
    let before = fixture(2);
    current("index-current", &run(&index(store.path())));
    current("outline-current", &run(&outline(store.path())));
    current("search-current", &run(&search(store.path())));
    current("token-current", &run(&token(store.path())));
    current(
        "neighborhood-current",
        &run(
            &json!({"op":"neighborhood","store":store.path(),"id":id(1),"readable_ids":[id(1),id(2)]}),
        ),
    );
    current(
        "status-current",
        &run(&json!({"op":"status","before":before.path(),"after":store.path(),"explicit":[]})),
    );
    let empty = fixture(1);
    failed(
        "outline-pending",
        &run(&outline(empty.path())),
        empty.path(),
        "pending",
        "unavailable",
    );
    let policy = crate::Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(
        crate::outlines::inspect(&policy, empty.path(), None)
            .unwrap()
            .status,
        "pending"
    );
    fs::write(source(empty.path()), format!("invalid metadata {PRIVATE}")).unwrap();
    let view =
        crate::index::refresh(&policy, empty.path(), None, crate::index::Mode::Full, None).unwrap();
    assert_eq!(view.status, "error");
    assert!(view.nodes.is_empty());
}

#[test]
fn invalid_missing_and_malformed_requests_are_not_success_or_private_diagnostics() {
    let store = fixture(1);
    current("invalid-prior-current", &run(&index(store.path())));
    fs::write(source(store.path()), format!("malformed {PRIVATE}")).unwrap();
    for (name, request) in [
        ("index-invalid", index(store.path())),
        ("search-invalid", search(store.path())),
        ("outline-invalid", outline(store.path())),
    ] {
        failed(name, &run(&request), store.path(), "error", "unavailable");
    }
    for (name, request) in [
        (
            "neighborhood-invalid",
            json!({"op":"neighborhood","store":store.path(),"id":id(1),"readable_ids":[id(1)]}),
        ),
        (
            "status-invalid",
            json!({"op":"status","before":store.path(),"after":store.path(),"explicit":[]}),
        ),
    ] {
        failed(name, &run(&request), store.path(), "error", "rejected");
    }
    let missing = store.path().join(PRIVATE);
    failed(
        "index-missing",
        &run(&index(&missing)),
        store.path(),
        "error",
        "unavailable",
    );
    failed(
        "token-missing",
        &run(&json!({"op":"token_refresh","path":missing.join("node.md")})),
        store.path(),
        "error",
        "not_found",
    );
    failed(
        "invalid-json",
        &spawn(&mut command(), format!("not-json {PRIVATE}").as_bytes())
            .wait_with_output()
            .unwrap(),
        store.path(),
        "error",
        "invalid_request",
    );
    failed(
        "invalid-scope",
        &run(
            &json!({"op":"search","store":store.path(),"query":"x","under_node_id":PRIVATE,"limit":1,"offset":0,"readable_ids":[]}),
        ),
        store.path(),
        "error",
        "rejected",
    );
}

#[test]
fn stale_native_results_and_nonrefreshing_reads_exit_nonzero() {
    for (name, make_request) in [
        ("index-stale", index as fn(&Path) -> Value),
        ("search-stale", search),
        ("token-stale", token),
    ] {
        let store = fixture(2);
        let barrier = tempfile::tempdir().unwrap();
        let ready = barrier.path().join("ready");
        let release = barrier.path().join("release");
        let mut request = make_request(store.path());
        request["pause_before_accept"] = json!({"ready":ready,"release":release});
        let mut child = spawn(&mut command(), &serde_json::to_vec(&request).unwrap());
        await_ready(&mut child, &ready);
        let p = source(store.path());
        let text = fs::read_to_string(&p).unwrap();
        fs::write(&p, text + "changed\n").unwrap();
        fs::write(release, b"release").unwrap();
        let output = child.wait_with_output().unwrap();
        if name == "token-stale" {
            record(name, &output);
            assert_eq!(output.status.code(), Some(1));
            assert_eq!(
                serde_json::from_slice::<Value>(&output.stderr).unwrap(),
                json!({"status":"stale","kind":"unavailable"})
            );
            let v: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(v["status"], "stale");
            assert!(!String::from_utf8_lossy(&output.stdout).contains(PRIVATE));
            assert!(
                !store
                    .path()
                    .join("nodes")
                    .join(id(1))
                    .join("node.meta.json")
                    .exists()
            );
        } else {
            failed(name, &output, store.path(), "stale", "unavailable");
        }
    }
    let store = fixture(2);
    current("outline-before-edit", &run(&index(store.path())));
    fs::write(
        source(store.path()),
        fs::read_to_string(source(store.path())).unwrap() + "edit\n",
    )
    .unwrap();
    failed(
        "outline-stale",
        &run(&outline(store.path())),
        store.path(),
        "stale",
        "unavailable",
    );
    current("outline-recovered-index", &run(&index(store.path())));
    current("outline-recovered", &run(&outline(store.path())));
}

#[test]
fn native_permission_and_resource_failures_are_redacted() {
    let store = fixture(1);
    let p = source(store.path());
    fs::set_permissions(&p, fs::Permissions::from_mode(0o0)).unwrap();
    assert!(
        File::open(&p).is_err(),
        "permission fault must be observed, not bypassed by root"
    );
    failed(
        "token-permission",
        &run(&token(store.path())),
        store.path(),
        "error",
        "permission_denied",
    );
    failed(
        "index-permission",
        &run(&index(store.path())),
        store.path(),
        "error",
        "unavailable",
    );
    fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
    let bytes = vec![b'x'; 1_048_577];
    failed(
        "request-resource",
        &spawn(&mut command(), &bytes).wait_with_output().unwrap(),
        store.path(),
        "error",
        "resource_exhausted",
    );
    fs::write(&p, &bytes).unwrap();
    failed(
        "token-resource",
        &run(&token(store.path())),
        store.path(),
        "error",
        "resource_exhausted",
    );
    failed(
        "index-resource",
        &run(&index(store.path())),
        store.path(),
        "error",
        "unavailable",
    );
    // Real kernel file-size pressure, with SIGXFSZ ignored so EFBIG is returned.
    // This is NOT a claim that the host disk was exhausted.
    let store = fixture(1);
    let mut c = command();
    unsafe {
        c.pre_exec(|| {
            libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
            let limit = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    failed(
        "kernel-file-pressure",
        &spawn(&mut c, &serde_json::to_vec(&token(store.path())).unwrap())
            .wait_with_output()
            .unwrap(),
        store.path(),
        "error",
        "resource_exhausted",
    );
}

#[test]
fn native_sqlite_and_token_lock_timeouts_fail_and_then_recover() {
    let store = fixture(1);
    current("lock-before-index", &run(&index(store.path())));
    let db = rusqlite::Connection::open(store.path().join(".sifttext/index.sqlite")).unwrap();
    db.execute_batch("BEGIN IMMEDIATE").unwrap();
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(source(store.path()).with_file_name("node.meta.lock"))
        .unwrap();
    lock.lock().unwrap();
    // All four calls are blocked before mutation. Keep both locks held until
    // every call is drained, and retain each real deadline/refusal assertion.
    thread::scope(|scope| {
        let pending = [
            ("sqlite-lock", index(store.path()), "busy", 4),
            ("sqlite-search-lock", search(store.path()), "busy", 4),
            ("sqlite-outline-lock", outline(store.path()), "busy", 4),
            ("token-lock", token(store.path()), "timeout", 9),
        ]
        .map(|(name, request, kind, minimum)| {
            scope.spawn(move || {
                let started = Instant::now();
                let output = run(&request);
                (name, output, kind, minimum, started.elapsed())
            })
        });
        let completed = pending.map(|call| call.join().unwrap());
        for (name, output, kind, minimum, elapsed) in completed {
            failed(name, &output, store.path(), "error", kind);
            assert!(elapsed >= Duration::from_secs(minimum));
            println!("lock wait {name}: {elapsed:?}");
        }
    });
    db.execute_batch("ROLLBACK").unwrap();
    current("sqlite-lock-recovery", &run(&index(store.path())));
    lock.unlock().unwrap();
    current("token-lock-recovery", &run(&token(store.path())));
}

#[test]
fn real_policy_process_timeout_cannot_return_a_successful_view() {
    // Fault-inject the owned helper process, not the executable, stamp or policy.
    // Stop it after spawn so invoke's unchanged real deadline/kill path executes.
    let store = fixture(256);
    // A wide graph now finishes before process discovery. A chain keeps the
    // same 10s request budget while making the real helper observable.
    for n in 1..=256 {
        let node = store.path().join("nodes").join(id(n));
        fs::remove_dir_all(node.join("children")).unwrap();
        fs::create_dir(node.join("children")).unwrap();
        let order = if n < 256 {
            std::os::unix::fs::symlink(
                format!("../../{}", id(n + 1)),
                node.join("children").join(id(n + 1)),
            )
            .unwrap();
            id(n + 1) + "\n"
        } else {
            String::new()
        };
        fs::write(node.join("children.order"), order).unwrap();
    }
    current("policy-timeout-fixture", &run(&index(store.path())));
    let mut c = command();
    c.process_group(0);
    let mut child = spawn(&mut c, &serde_json::to_vec(&index(store.path())).unwrap());
    struct Group(i32);
    impl Drop for Group {
        fn drop(&mut self) {
            unsafe {
                libc::kill(-self.0, libc::SIGKILL);
            }
        }
    }
    let group = Group(child.id() as i32);
    let deadline = Instant::now() + Duration::from_secs(20);
    let helper = loop {
        assert!(
            child.try_wait().unwrap().is_none(),
            "policy helper escaped timeout injection"
        );
        assert!(Instant::now() < deadline, "no owned policy child found");
        let pids = Command::new("pgrep")
            .args(["-P", &child.id().to_string()])
            .output()
            .unwrap();
        if let Some(pid) = String::from_utf8_lossy(&pids.stdout)
            .lines()
            .next()
            .and_then(|s| s.trim().parse::<i32>().ok())
        {
            // The observer spawns only its policy helper. Stop the owned direct
            // child before a separate ps/name probe gives it time to finish.
            assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
            break pid;
        }
        thread::sleep(Duration::from_millis(1));
    };
    // Signal delivery is asynchronous. Require the actual stopped state, not
    // merely a successful kill syscall or an immediately sampled ps result.
    let stopped_deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let state = Command::new("ps")
            .args(["-p", &helper.to_string(), "-o", "state=", "-o", "comm="])
            .output()
            .unwrap();
        let state = String::from_utf8_lossy(&state.stdout);
        if state
            .split_whitespace()
            .next()
            .is_some_and(|s| s.contains('T'))
        {
            assert!(
                state.contains("graph-policy"),
                "stopped child was not the policy helper"
            );
            break;
        }
        assert!(
            Instant::now() < stopped_deadline,
            "helper was not stopped: {state}"
        );
        thread::sleep(Duration::from_millis(1));
    }
    let started = Instant::now();
    let output = child.wait_with_output().unwrap();
    // index records deliberately preserve a structured unavailable view for a
    // failed validator. The cause is discarded there, not guessed by the CLI.
    failed(
        "policy-timeout",
        &output,
        store.path(),
        "error",
        "unavailable",
    );
    assert!(started.elapsed() >= Duration::from_secs(8));
    drop(group);
}

#[test]
fn native_acceptance_barrier_timeout_is_not_current() {
    let store = fixture(1);
    let barrier = tempfile::tempdir().unwrap();
    let mut request = index(store.path());
    request["pause_before_accept"] = json!({"ready":barrier.path().join("ready"),"release":barrier.path().join("never-released")});
    failed(
        "acceptance-timeout",
        &run(&request),
        store.path(),
        "error",
        "timeout",
    );
    current("acceptance-timeout-recovery", &run(&index(store.path())));
}
