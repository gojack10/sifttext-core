use super::*;
fn shell(script: &str) -> Command {
    let mut c = Command::new("sh");
    c.args(["-c", script]);
    c
}

#[test]
fn helper_timeout_kills_real_child_and_grandchild_before_custody_releases() {
    let t = tempfile::tempdir().unwrap();
    let late = t.path().join("late");
    let ready = t.path().join("ready");
    let gate = fs::File::create(t.path().join("gate")).unwrap();
    gate.lock().unwrap();
    let witness = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(t.path().join("gate"))
        .unwrap();
    let custody = Custody::new(&[&gate]).unwrap();
    let script = format!(
        "sh -c 'echo ready > {}; sleep 0.6; echo unsafe > {}' & wait",
        ready.display(),
        late.display()
    );
    let e = run_bounded(shell(&script), Duration::from_millis(150)).unwrap_err();
    assert!(e.to_string().contains("timeout"), "{e}");
    assert!(ready.exists());
    drop(custody);
    drop(gate);
    let handoff = Instant::now();
    while witness.try_lock().is_err() {
        assert!(handoff.elapsed() < Duration::from_secs(1));
        thread::sleep(Duration::from_millis(2));
    }
    thread::sleep(Duration::from_millis(650));
    assert!(
        !late.exists(),
        "descendant survived confirmed timeout cleanup"
    );
}
#[test]
fn helper_nonzero_and_output_limit_remain_bounded_and_drained() {
    assert_eq!(
        run_bounded(shell("exit 7"), Duration::from_secs(2))
            .unwrap()
            .status
            .code(),
        Some(7)
    );
    let t = tempfile::tempdir().unwrap();
    let late = t.path().join("late");
    let c = shell(&format!(
        "sh -c 'sleep 0.5; echo unsafe > {}' & yes x",
        late.display()
    ));
    assert!(
        run_bounded(c, Duration::from_secs(2))
            .unwrap_err()
            .to_string()
            .contains("resource limit")
    );
    thread::sleep(Duration::from_millis(550));
    assert!(!late.exists());
}
#[test]
fn pipe_read_failure_stops_group_and_keeps_initiating_error() {
    let t = tempfile::tempdir().unwrap();
    let late = t.path().join("late");
    PIPE_ERROR.with(|v| v.set(true));
    let e = run_bounded(
        shell(&format!(
            "sh -c 'sleep 0.3; echo unsafe > {}' & wait",
            late.display()
        )),
        Duration::from_secs(2),
    )
    .unwrap_err();
    assert!(e.to_string().contains("pipe read failed"), "{e}");
    thread::sleep(Duration::from_millis(350));
    assert!(!late.exists());
}
#[test]
fn descendant_closing_output_still_holds_inherited_custody() {
    let t = tempfile::tempdir().unwrap();
    let done = t.path().join("done");
    let start = Instant::now();
    let script = format!(
        "sh -c 'exec >/dev/null 2>&1; sleep 0.15; echo done > {}' &",
        done.display()
    );
    assert!(
        run_bounded(shell(&script), Duration::from_secs(2))
            .unwrap()
            .status
            .success()
    );
    assert!(done.exists());
    assert!(start.elapsed() >= Duration::from_millis(140));
}
