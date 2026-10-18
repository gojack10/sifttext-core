use std::process::{Command, Stdio};

#[test]
fn closed_stdout_returns_io_error_instead_of_panicking() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sifttext"))
        .arg("-h")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!stderr.contains("panicked"), "{stderr}");
    assert!(stderr.contains("Broken pipe"), "{stderr}");
    assert_eq!(output.status.code(), Some(1));
}
