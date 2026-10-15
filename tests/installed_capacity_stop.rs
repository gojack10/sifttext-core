//! Opt-in installed-job cancellation check. The outside operator launches the
//! source-bound Cargo transport first, then supplies its job directory and
//! private CAPACITY_PROCESS_EVIDENCE directory. Never run from a floor worker.
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn absent_error(errno: Option<i32>) -> bool {
    // Darwin can return EPERM while a killed group is still disappearing.
    // Permission denial is never absence: keep observing within the same bound.
    if errno == Some(libc::EPERM) {
        return false;
    }
    assert_eq!(errno, Some(libc::ESRCH));
    true
}
fn absent(pgid: i32) -> bool {
    assert!(pgid > 1, "invalid process-group identity");
    if unsafe { libc::kill(-pgid, 0) } == 0 {
        return false;
    }
    absent_error(std::io::Error::last_os_error().raw_os_error())
}
fn receipt_valid(terminal: &Value, receipt: &Value, key: &str) -> bool {
    terminal["id"] == key
        && terminal["state"] == "cancelled"
        && terminal["drained"] == true
        && terminal["drainScope"] == "process-group"
        && receipt["drained"] == true
        && receipt["runner_timeout"] == false
        && receipt["cancelled_signal"] == libc::SIGTERM
}
#[test]
fn receipt_contract_rejects_unproven_cancellation() {
    assert!(!absent_error(Some(libc::EPERM)));
    assert!(absent_error(Some(libc::ESRCH)));
    assert!(std::panic::catch_unwind(|| absent_error(Some(libc::EINVAL))).is_err());
    let terminal = json!({"id":"fixture", "state":"cancelled", "drained":true,
        "drainScope":"process-group"});
    let receipt = json!({"drained":true,"runner_timeout":false,"cancelled_signal":libc::SIGTERM});
    assert!(receipt_valid(&terminal, &receipt, "fixture"));
    for (field, value) in [
        ("id", json!("other")),
        ("state", json!("exited")),
        ("drained", json!(false)),
        ("drainScope", json!("unknown")),
    ] {
        let mut bad = terminal.clone();
        bad[field] = value;
        assert!(!receipt_valid(&bad, &receipt, "fixture"), "{field}");
    }
    for (field, value) in [
        ("drained", json!(false)),
        ("runner_timeout", json!(true)),
        ("cancelled_signal", json!(0)),
    ] {
        let mut bad = receipt.clone();
        bad[field] = value;
        assert!(!receipt_valid(&terminal, &bad, "fixture"), "{field}");
    }
    assert!(!receipt_valid(&json!({}), &receipt, "fixture"));
    assert!(!receipt_valid(&terminal, &json!({}), "fixture"));
}

#[test]
#[ignore = "requires explicit outside-operator authority to cancel the supplied disposable job"]
fn cancel_installed_job_after_sampler_admission() {
    for name in [
        "FACTORY_SESSION",
        "FACTORY_SENDER",
        "BF2_BOARD_SESSION",
        "BF2_BOARD_SENDER",
        "BF2_SWARM_WORKER",
    ] {
        assert!(
            std::env::var_os(name).is_none(),
            "outside operator required: {name}"
        );
    }
    let job = std::path::PathBuf::from(std::env::var_os("STOP_JOB_DIR").unwrap());
    let root = std::path::PathBuf::from(std::env::var_os("CAPACITY_PROCESS_EVIDENCE").unwrap());
    let floor = std::env::var("STOP_FLOOR").unwrap();
    let key = job.file_name().unwrap().to_str().unwrap();
    let manifest = read(&job.join("manifest.json"));
    assert_eq!(manifest["id"], key);
    assert_eq!(manifest["owner"], std::env::var("STOP_OWNER").unwrap());
    assert!(
        manifest["command"]
            .as_str()
            .unwrap()
            .contains(root.to_str().unwrap()),
        "evidence directory is not bound to this job command"
    );
    assert!(
        !job.join("terminal.json").exists(),
        "job already terminal; never recancel"
    );
    assert!(
        !root.join("request.receipt.json").exists(),
        "stale evidence"
    );
    let start = Instant::now();
    while !root.join("sampling-ready").exists() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "sampler admission deadline"
        );
        assert!(
            !job.join("terminal.json").exists(),
            "job ended before admission"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let request = read(&root.join("request.start.json"))["pgid"]
        .as_i64()
        .unwrap() as i32;
    let sampler: i32 = fs::read_to_string(root.join("sampling-pgid"))
        .unwrap()
        .parse()
        .unwrap();
    assert!(!absent(request), "request absent before cancellation");
    assert!(!absent(sampler), "sampler absent before cancellation");
    let cancel_at = Instant::now();
    let mut cancel = Command::new("factory-job")
        .args(["cancel", "--floor", &floor, "--key", key])
        .stdout(fs::File::create(root.join("cancel.stdout")).unwrap())
        .stderr(fs::File::create(root.join("cancel.stderr")).unwrap())
        .spawn()
        .unwrap();
    // Observe the detached groups independently; the launcher certifies only its
    // own command group. Never signal these PIDs ourselves or clear a failed job.
    let mut result = None;
    while cancel_at.elapsed() < Duration::from_secs(1) {
        if absent(request) && absent(sampler) && root.join("request.receipt.json").exists() {
            result = Some(cancel_at.elapsed());
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    // Drain the CLI before asserting; cancellation itself remains the launcher's
    // identity-bound responsibility. Preserve every failed receipt for diagnosis.
    let wait_at = Instant::now();
    let status = loop {
        if let Some(status) = cancel.try_wait().unwrap() {
            break status;
        }
        if wait_at.elapsed() >= Duration::from_secs(10) {
            cancel.kill().unwrap();
            cancel.wait().unwrap();
            panic!("cancellation CLI did not settle; inspect retained job");
        }
        thread::sleep(Duration::from_millis(5));
    };
    assert!(
        status.success(),
        "installed cancellation failed; inspect cancel.stderr"
    );
    let elapsed = result.expect("request/sampler group or receipt exceeded one-second grace");
    let terminal = read(&job.join("terminal.json"));
    let receipt = read(&root.join("request.receipt.json"));
    assert!(
        receipt_valid(&terminal, &receipt, key),
        "{terminal}\n{receipt}"
    );
    assert!(
        !root.join("next-shape").exists(),
        "later request after Stop"
    );
    assert!(absent(request) && absent(sampler));
    fs::write(
        root.join("installed-result.json"),
        serde_json::to_vec_pretty(&json!({
            "job":key,"request_pgid":request,"sampling_pgid":sampler,
            "elapsed_ms":elapsed.as_secs_f64()*1000.,"terminal":terminal,
            "request_receipt":receipt,"groups_absent":true,"next_shape_absent":true
        }))
        .unwrap(),
    )
    .unwrap();
}
