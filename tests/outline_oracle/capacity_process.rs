// Capacity-only process envelope. Stop is sticky: no later request/shape starts.
use std::os::unix::process::CommandExt;
use std::sync::Once;
use std::sync::atomic::{AtomicI32, Ordering};
static STOP: AtomicI32 = AtomicI32::new(0);
static SIGNALS: Once = Once::new();
extern "C" fn stop(signal: libc::c_int) {
    let _ = STOP.compare_exchange(0, signal, Ordering::Relaxed, Ordering::Relaxed);
}
// Libtest cannot see the sticky flag while pure work finishes. Never let its
// normal exit report success after Stop; request groups retain their own drain.
extern "C" fn refuse_stopped_exit() {
    let signal = STOP.load(Ordering::Relaxed);
    if signal != 0 {
        unsafe { libc::_exit(128 + signal) };
    }
}
pub(super) fn install_stop() {
    SIGNALS.call_once(|| unsafe {
        assert_eq!(libc::atexit(refuse_stopped_exit), 0);
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = stop as *const () as usize;
        libc::sigemptyset(&mut action.sa_mask);
        for signal in [libc::SIGTERM, libc::SIGINT] {
            assert_eq!(libc::sigaction(signal, &action, std::ptr::null_mut()), 0);
        }
    });
}
// Reuse the producer envelope without installing sticky handlers earlier.
// Calls already in flight also observe Stop if capacity installs handlers later.
pub(super) fn observer_output(command: Command, input: Vec<u8>) -> Output {
    let evidence = TempDir::new().unwrap();
    let root = std::env::var_os("SHARED_OBSERVER_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| evidence.path().to_owned());
    // The command is consumed once; failure is rethrown, never retried.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        bounded_request_without_install(
            command,
            input,
            &root.join("observer"),
            Duration::from_secs(900),
            None,
        )
        .0
    }));
    match result {
        Ok(out) => out,
        Err(error) => {
            eprintln!("failed observer evidence: {}", evidence.keep().display());
            std::panic::resume_unwind(error)
        }
    }
}
fn probe_group(pid: u32) -> std::io::Result<bool> {
    if unsafe { libc::kill(-(pid as i32), 0) } == 0 {
        return Ok(true);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(false)
    } else {
        Err(error)
    }
}
fn group_alive(pid: u32) -> bool {
    // A denied probe does not establish absence.
    probe_group(pid).unwrap_or(true)
}
struct RequestGroup {
    child: Child,
    cancelled: bool,
    cancel_errno: Option<i32>,
    child_kill_errno: Option<i32>,
    probe_errno: Option<i32>,
}
impl RequestGroup {
    fn alive(&mut self) -> bool {
        match probe_group(self.child.id()) {
            Ok(alive) => alive,
            Err(error) => {
                self.probe_errno = error.raw_os_error();
                true
            }
        }
    }
    fn cancel(&mut self) {
        self.cancel_with(|pid| {
            if unsafe { libc::kill(-(pid as i32), libc::SIGKILL) } == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }
    fn cancel_with(&mut self, kill: impl FnOnce(u32) -> std::io::Result<()>) {
        if !self.cancelled {
            // Mark before cleanup: unwinding must not retry a denied group signal.
            self.cancelled = true;
            if let Err(error) = kill(self.child.id()) {
                if error.raw_os_error() != Some(libc::ESRCH) {
                    self.cancel_errno = error.raw_os_error();
                    if let Err(error) = self.child.kill() {
                        self.child_kill_errno = error.raw_os_error();
                    }
                }
            }
        }
    }
}
impl Drop for RequestGroup {
    fn drop(&mut self) {
        self.cancel();
        // No blocking wait or panic during unwinding if signalling was refused.
        let start = Instant::now();
        while matches!(self.child.try_wait(), Ok(None))
            && start.elapsed() < Duration::from_millis(500)
        {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
// 900s is the retained runner allowance, not permission for an unlimited producer.
fn bounded_request(
    command: Command,
    input: Vec<u8>,
    prefix: &Path,
    bound: Duration,
    kill_ready: Option<PathBuf>,
) -> (Output, Value) {
    install_stop();
    bounded_request_without_install(command, input, prefix, bound, kill_ready)
}
pub(super) fn bounded_request_without_install(
    command: Command,
    input: Vec<u8>,
    prefix: &Path,
    bound: Duration,
    kill_ready: Option<PathBuf>,
) -> (Output, Value) {
    bounded_sampled_request_without_install(
        command,
        input,
        prefix,
        bound,
        kill_ready,
        Duration::from_millis(50),
    )
}
pub(super) fn bounded_sampled_request_without_install(
    command: Command,
    input: Vec<u8>,
    prefix: &Path,
    bound: Duration,
    kill_ready: Option<PathBuf>,
    sample_interval: Duration,
) -> (Output, Value) {
    finish_request(
        start_request(command, input, prefix, bound),
        kill_ready,
        sample_interval,
    )
}
struct PendingRequest {
    group: RequestGroup,
    writer: std::thread::JoinHandle<std::io::Result<()>>,
    prefix: PathBuf,
    bound: Duration,
    start: Instant,
}
fn start_request(
    mut command: Command,
    input: Vec<u8>,
    prefix: &Path,
    bound: Duration,
) -> PendingRequest {
    assert_eq!(
        STOP.load(Ordering::Relaxed),
        0,
        "runner stopped; no next request"
    );
    let stdout = prefix.with_extension("stdout");
    let stderr = prefix.with_extension("stderr");
    command
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(fs::File::create(&stdout).unwrap())
        .stderr(fs::File::create(&stderr).unwrap());
    let start = Instant::now();
    let child = command.spawn().unwrap_or_else(|error| {
        fs::write(
            prefix.with_extension("receipt.json"),
            json!({"spawn_error":error.to_string(),
            "runner_timeout":false,"drained":true})
            .to_string(),
        )
        .unwrap();
        panic!("request spawn failed: {error}");
    });
    let mut group = RequestGroup {
        child,
        cancelled: false,
        cancel_errno: None,
        child_kill_errno: None,
        probe_errno: None,
    };
    let pid = group.child.id();
    fs::write(
        prefix.with_extension("start.json"),
        json!({"pid":pid,"pgid":pid}).to_string(),
    )
    .unwrap();
    // Writing a large request must not bypass the producer deadline.
    let mut stdin = group.child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    PendingRequest {
        group,
        writer,
        prefix: prefix.to_owned(),
        bound,
        start,
    }
}
fn finish_request(
    pending: PendingRequest,
    kill_ready: Option<PathBuf>,
    sample_interval: Duration,
) -> (Output, Value) {
    let PendingRequest {
        mut group,
        writer,
        prefix,
        bound,
        start,
    } = pending;
    let pid = group.child.id();
    let stdout = prefix.with_extension("stdout");
    let stderr = prefix.with_extension("stderr");
    let mut status = None;
    let mut timeout = false;
    let mut cancelled_signal = 0;
    let mut cancel_at = group.cancelled.then(Instant::now);
    let mut samples = vec![];
    let (mut peak_rss, mut peak_fd) = (0, 0);
    let mut sample_at = Instant::now();
    let mut barrier_reached = false;
    let mut sample_failure = None;
    let mut sample_ms = 0.;
    loop {
        let signal = STOP.load(Ordering::Relaxed);
        if signal != 0 {
            cancelled_signal = signal;
        }
        if cancel_at.is_none() && (signal != 0 || start.elapsed() >= bound) {
            cancelled_signal = signal;
            timeout = signal == 0;
            group.cancel();
            cancel_at = Some(Instant::now());
        }
        if !barrier_reached
            && cancel_at.is_none()
            && kill_ready.as_ref().is_some_and(|p| p.exists())
        {
            group.child.kill().unwrap();
            barrier_reached = true;
        }
        if status.is_none() {
            status = group.child.try_wait().unwrap();
        }
        if status.is_some() && !group.alive() {
            break;
        }
        if cancel_at.is_some_and(|t| t.elapsed() >= Duration::from_secs(5)) {
            break;
        }
        if cancel_at.is_none()
            && (std::env::var_os("CAPACITY_PROCESS_CONTROL").is_none()
                || std::env::var_os("CAPACITY_SAMPLE_CONTROL").is_some())
            && sample_at.elapsed() >= sample_interval
        {
            let sample_start = Instant::now();
            match std::panic::catch_unwind(|| {
                native_resources::sample_cancellable(pid, || STOP.load(Ordering::Relaxed) != 0)
            }) {
                Ok((rss, fd, raw)) => {
                    peak_rss = peak_rss.max(rss);
                    peak_fd = peak_fd.max(fd);
                    samples.push(raw);
                }
                Err(error) => {
                    sample_failure = Some(
                        error
                            .downcast_ref::<String>()
                            .cloned()
                            .or_else(|| error.downcast_ref::<&str>().map(|s| s.to_string()))
                            .unwrap_or_else(|| "resource sampler failed".into()),
                    );
                    group.cancel();
                    cancel_at = Some(Instant::now());
                }
            }
            sample_ms += sample_start.elapsed().as_secs_f64() * 1000.;
            sample_at = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let drained = status.is_some() && !group.alive();
    // Stop can arrive after the loop's last poll but before completion is consumed.
    cancelled_signal = cancelled_signal.max(STOP.load(Ordering::Relaxed));
    let receipt = json!({"pid":pid,"pgid":pid,"exit":status.and_then(|s|s.code()),
        "signal":status.and_then(|s|s.signal()),"runner_timeout":timeout,
        "cancelled_signal":cancelled_signal,"runner_bound_ms":bound.as_millis(),
        "ms":start.elapsed().as_secs_f64()*1000.,"drained":drained,"barrier_reached":barrier_reached,
        "sampled_helper_inclusive_rss_bytes":peak_rss,"sampled_numeric_fd_peak":peak_fd,"samples":samples,
        "sampling_ms":sample_ms,"sampling_failure":sample_failure,
        "cancel_errno":group.cancel_errno,"child_kill_errno":group.child_kill_errno,
        "probe_errno":group.probe_errno});
    // Persist failed cancellation receipts before assertions can unwind the runner.
    fs::write(
        prefix.with_extension("process.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    fs::write(
        prefix.with_extension("receipt.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    assert!(drained, "request group did not drain: {receipt}");
    group.cancelled = true; // Reaped and absent: never signal a recycled process-group ID.
    let write = writer.join().unwrap();
    let out = Output {
        status: status.unwrap(),
        stdout: fs::read(stdout).unwrap(),
        stderr: fs::read(stderr).unwrap(),
    };
    assert!(
        !timeout
            && cancelled_signal == 0
            && sample_failure.is_none()
            && group.cancel_errno.is_none()
            && group.child_kill_errno.is_none()
            && group.probe_errno.is_none(),
        "FAILED bounded request: {receipt}"
    );
    write.unwrap();
    (out, receipt)
}
include!("observer_lifetime.rs");
#[test]
fn denied_cancellation_preserves_receipt() {
    let dir = TempDir::new().unwrap();
    let prefix = dir.path().join("denied");
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "exec sleep 30"]);
    let mut pending = start_request(command, vec![], &prefix, Duration::from_secs(2));
    let pid = pending.group.child.id();
    pending
        .group
        .cancel_with(|_| Err(std::io::Error::from_raw_os_error(libc::EPERM)));
    assert_eq!(pending.group.cancel_errno, Some(libc::EPERM));
    pending
        .group
        .cancel_with(|_| panic!("cancellation retried"));
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        finish_request(pending, None, Duration::from_secs(1))
    }));
    assert!(failed.is_err());
    let receipt: Value =
        serde_json::from_slice(&fs::read(prefix.with_extension("receipt.json")).unwrap()).unwrap();
    assert_eq!(receipt["cancel_errno"], libc::EPERM);
    assert_eq!(receipt["child_kill_errno"], Value::Null);
    assert_eq!(receipt["signal"], libc::SIGKILL);
    assert_eq!(receipt["drained"], true);
    assert!(!group_alive(pid));
    assert_eq!(
        fs::read(prefix.with_extension("receipt.json")).unwrap(),
        fs::read(prefix.with_extension("process.json")).unwrap()
    );
}
#[test]
fn bounded_producer_and_stop_controls() {
    // Stop is process-wide, so exercise it in a fresh copy of this test executable.
    if let Ok(case) = std::env::var("CAPACITY_PROCESS_CONTROL") {
        let root = PathBuf::from(std::env::var_os("CAPACITY_PROCESS_EVIDENCE").unwrap());
        let prefix = root.join("request");
        if case.starts_with("terminal") {
            install_stop();
            if case != "terminal-success" {
                assert_eq!(
                    unsafe {
                        libc::raise(if case.ends_with("term") {
                            libc::SIGTERM
                        } else {
                            libc::SIGINT
                        })
                    },
                    0
                );
            }
            // No request adapter runs after the signal: terminal status itself
            // must refuse success, even though this pure-work body returns.
            fs::write(root.join("pure-return"), "completed").unwrap();
            return;
        }
        if case.starts_with("inspect") {
            use std::os::unix::fs::PermissionsExt;
            fs::create_dir(root.join("nodes")).unwrap();
            let bin = root.join("core-observe");
            fs::write(&bin, "#!/bin/sh\ncat >/dev/null\nprintf ready > \"$CAPACITY_PROCESS_EVIDENCE/ready\"\nexec sleep 30\n").unwrap();
            fs::set_permissions(&bin, fs::Permissions::from_mode(0o700)).unwrap();
            unsafe {
                std::env::set_var("SIFTTEXT_TEST_NATIVE_BIN_DIR", &root);
            }
            install_stop();
            if case.starts_with("inspect-before") {
                assert_eq!(
                    unsafe {
                        libc::raise(if case.ends_with("term") {
                            libc::SIGTERM
                        } else {
                            libc::SIGINT
                        })
                    },
                    0
                );
            }
            let mut runner = Runner {
                evidence: root.clone(),
                serial: 0,
                fd_limit: 256,
            };
            runner.check_native("inspect", &root, "global", &ORACLE_BPE);
            fs::write(root.join("next-shape"), "started").unwrap();
            return;
        }
        if case.starts_with("outer") {
            install_stop();
            let mut guard = PausedIndex(Some(
                Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "capacity::bounded_producer_and_stop_controls"])
                    .env("CAPACITY_PROCESS_CONTROL", "stop-child")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            ));
            let start = Instant::now();
            let mut forwarded = false;
            let status = loop {
                let signal = STOP.load(Ordering::Relaxed);
                if signal != 0 && !forwarded {
                    assert_eq!(
                        unsafe { libc::kill(guard.0.as_ref().unwrap().id() as i32, signal) },
                        0
                    );
                    forwarded = true;
                }
                if let Some(status) = guard.0.as_mut().unwrap().try_wait().unwrap() {
                    break status;
                }
                assert!(
                    start.elapsed() < Duration::from_secs(7),
                    "outer control did not drain"
                );
                std::thread::sleep(Duration::from_millis(5));
            };
            guard.0.take();
            fs::write(root.join("outer.json"), json!({"exit":status.code(),"forwarded":forwarded,"signal":STOP.load(Ordering::Relaxed)}).to_string()).unwrap();
            assert!(!forwarded, "outer stopped; no next control or shape");
            return;
        }
        let mut command = Command::new("/bin/sh");
        let script = match case.as_str() {
            "success" => "printf complete",
            "failed" => "exit 1",
            "malformed" => "printf not-json",
            "delayed" => "sleep 0.1; printf ready > \"$1/ready\"; exec sleep 30",
            // Leader exits while its child retains the output descriptors.
            "inherited" => "sleep 30 & exit 0",
            _ => "printf ready > \"$1/ready\"; exec sleep 30",
        };
        command.args(["-c", script, "control"]).arg(&root);
        // Readiness is a positive control; allow scheduling delay under full Cargo load.
        // Timeout discriminators and the parent's one-second Stop ceiling stay unchanged.
        let bound =
            if case == "delayed" || case.starts_with("stop") || case.starts_with("sampler-stop") {
                Duration::from_secs(5)
            } else {
                Duration::from_millis(200)
            };
        let input = if case == "hung" || case.starts_with("stop") {
            vec![b'x'; 1024 * 1024]
        } else {
            vec![]
        };
        let kill_ready = (case == "delayed").then(|| root.join("ready"));
        if case == "sampler" || case.starts_with("sampler-stop") {
            unsafe {
                std::env::set_var(
                    "CAPACITY_SAMPLE_CONTROL",
                    if case == "sampler" { "hung" } else { "stop" },
                );
            }
        }
        let result = bounded_request(command, input, &prefix, bound, kill_ready);
        // This marker must be unreachable after Stop or timeout.
        fs::write(root.join("next-shape"), "started").unwrap();
        if case == "success" {
            assert_eq!(result.0.stdout, b"complete");
        }
        return;
    }
    for case in [
        "terminal-success",
        "terminal-stop-term",
        "terminal-stop-int",
        "success",
        "failed",
        "malformed",
        "delayed",
        "sampler",
        "sampler-stop-term",
        "sampler-stop-int",
        "hung",
        "inherited",
        "stop-term",
        "stop-int",
        "outer-term",
        "outer-int",
        "inspect-stop-term",
        "inspect-stop-int",
        "inspect-before-term",
        "inspect-before-int",
    ] {
        let dir = TempDir::new().unwrap();
        let persistent =
            std::env::var_os("CAPACITY_CONTROL_EVIDENCE").map(|p| PathBuf::from(p).join(case));
        let root = persistent.as_deref().unwrap_or(dir.path());
        if persistent.is_some() {
            fs::create_dir(root).unwrap();
        }
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "capacity::bounded_producer_and_stop_controls",
                "--nocapture",
            ])
            .env("CAPACITY_PROCESS_CONTROL", case)
            .env("CAPACITY_PROCESS_EVIDENCE", root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let start = Instant::now();
        let mut signalled_at = None;
        if case.starts_with("stop")
            || case.starts_with("outer")
            || case.starts_with("sampler-stop")
            || case.starts_with("inspect-stop")
        {
            let marker = if case.starts_with("sampler-stop") {
                "sampling-ready"
            } else {
                "ready"
            };
            while !root.join(marker).exists() {
                assert!(start.elapsed() < Duration::from_secs(5));
                assert!(child.try_wait().unwrap().is_none());
                std::thread::sleep(Duration::from_millis(5));
            }
            let signal = if case.ends_with("term") {
                libc::SIGTERM
            } else {
                libc::SIGINT
            };
            assert_eq!(unsafe { libc::kill(child.id() as i32, signal) }, 0);
            signalled_at = Some(Instant::now());
        }
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if ((case.starts_with("sampler-stop") || case.starts_with("inspect-stop"))
                && signalled_at.is_some_and(|t| t.elapsed() >= Duration::from_secs(1)))
                || start.elapsed() > Duration::from_secs(8)
            {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("control hung: {case}");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        if case.starts_with("terminal") {
            assert!(root.join("pure-return").exists());
            assert_eq!(
                status.code(),
                Some(if case == "terminal-success" {
                    0
                } else if case.ends_with("term") {
                    128 + libc::SIGTERM
                } else {
                    128 + libc::SIGINT
                })
            );
            assert!(!root.join("request.start.json").exists());
            continue;
        }
        if case.starts_with("inspect-before") {
            assert!(!status.success());
            assert!(
                !root.join("ready").exists(),
                "inspection started after Stop"
            );
            assert!(!root.join("000-inspect.start.json").exists());
            assert!(!root.join("next-shape").exists());
            continue;
        }
        let receipt: Value = serde_json::from_slice(
            &fs::read(root.join(if case.starts_with("inspect-stop") {
                "000-inspect.process.json"
            } else {
                "request.process.json"
            }))
            .unwrap(),
        )
        .unwrap();
        eprintln!("{case}: {receipt}");
        assert_eq!(receipt["drained"], true, "{case}");
        assert!(!group_alive(receipt["pgid"].as_u64().unwrap() as u32));
        if case.starts_with("sampler-stop") {
            let pgid: u32 = fs::read_to_string(root.join("sampling-pgid"))
                .unwrap()
                .parse()
                .unwrap();
            assert!(!group_alive(pgid), "sampling group survived Stop: {pgid}");
        }
        if matches!(case, "success" | "failed" | "malformed") {
            assert!(status.success(), "{case}");
            assert_eq!(receipt["runner_timeout"], false);
            assert_eq!(receipt["exit"], if case == "failed" { 1 } else { 0 });
            assert!(root.join("next-shape").exists());
        } else if case == "sampler" {
            assert!(!status.success());
            assert!(
                receipt["sampling_failure"]
                    .as_str()
                    .unwrap()
                    .contains("deadline")
            );
            assert!(!root.join("next-shape").exists());
        } else if case == "delayed" {
            assert!(status.success());
            assert_eq!(receipt["barrier_reached"], true);
            assert_eq!(receipt["signal"], libc::SIGKILL);
            assert_eq!(receipt["runner_timeout"], false);
        } else if matches!(case, "hung" | "inherited") {
            assert!(!status.success());
            assert_eq!(receipt["runner_timeout"], true);
            assert!(!root.join("next-shape").exists());
        } else if case.starts_with("stop")
            || case.starts_with("outer")
            || case.starts_with("sampler-stop")
            || case.starts_with("inspect-stop")
        {
            assert!(!status.success());
            assert_eq!(receipt["runner_timeout"], false);
            assert_eq!(
                receipt["cancelled_signal"],
                if case.ends_with("term") {
                    libc::SIGTERM
                } else {
                    libc::SIGINT
                }
            );
            assert!(!root.join("next-shape").exists());
            if case.starts_with("outer") {
                let outer: Value =
                    serde_json::from_slice(&fs::read(root.join("outer.json")).unwrap()).unwrap();
                assert_eq!(outer["forwarded"], true);
                assert_eq!(outer["signal"], receipt["cancelled_signal"]);
            }
        }
    }
}
