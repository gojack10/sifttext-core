//! Offline worker integration: replay saved transcripts, never native commands.
use std::{fs, path::PathBuf, process::Command};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).map(PathBuf::from).collect();
    assert_eq!(
        args.len(),
        6,
        "SOURCE WORKER CARGO_LOG PORTABLE_LOG CHECKER EVIDENCE_DIR"
    );
    let root = PathBuf::from(format!(
        "/tmp/sifttext-worker-receipts-{}",
        std::process::id()
    ));
    fs::create_dir(&root).unwrap();
    for file in [
        "scripts/check-acceptance-controls.sh",
        "scripts/acceptance-controls.rs",
        "scripts/cargo-portable-framing.rs",
        "scripts/acceptance-controls.tsv",
        "tests/outline_file_oracle.rs",
        "tests/outline_oracle/capacity.rs",
        "tests/global_git_identity.rs",
        "tests/submission_attribution_native.rs",
        "tests/portable_runtime.rs",
        "tests/tool_consumers.rs",
    ] {
        let destination = root.join(file);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(args[0].join(file), destination).unwrap();
    }
    for dir in ["build", "target"] {
        fs::create_dir(root.join(dir)).unwrap();
    }
    fs::copy(&args[4], root.join("checker")).unwrap();
    fs::write(root.join("scripts/build.sh"), ":\n").unwrap();
    fs::write(root.join("install-local.sh"), "mkdir -p \"$1\" \"$2\"\n").unwrap();
    fs::write(
        root.join("completion.sh"),
        "printf COMPLETED > \"$PWD/completed\"\n",
    )
    .unwrap();
    fs::write(root.join("scripts/with-tools.sh"), r#"#!/bin/sh
case "$*" in
  'rustc --edition 2024 -D warnings src/bin/acceptance-receipts.rs -o '*)
    for output in "$@"; do :; done
    cp "$PWD/checker" "$output" ;;
  'cargo test --locked -- --nocapture') cat "$PWD/tests.log" ;;
  'cargo test --locked --test portable_runtime -- --ignored --exact relocated_runtime_refuses_missing_and_tampered_assets --nocapture') cat "$PWD/package.log" ;;
  *) echo 'Unexpected offline dispatch' >&2; exit 1 ;;
esac
"#).unwrap();
    let worker = fs::read_to_string(&args[1]).unwrap();
    let tests = fs::read_to_string(&args[2]).unwrap();
    let package = fs::read_to_string(&args[3]).unwrap();
    let run = |name: &str, script: &str, cargo: &str, portable: &str, expected: bool| {
        fs::write(root.join("worker.sh"), script).unwrap();
        fs::write(root.join("tests.log"), cargo).unwrap();
        fs::write(root.join("package.log"), portable).unwrap();
        if root.join("completed").exists() {
            fs::remove_file(root.join("completed")).unwrap();
        }
        let output = Command::new("sh")
            .args(["worker.sh", "sh", "completion.sh"])
            .current_dir(&root)
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        fs::write(args[5].join(format!("worker-{name}.log")), &text).unwrap();
        assert_eq!(output.status.success(), expected, "{name}: {text}");
        assert_eq!(
            root.join("completed").exists(),
            expected,
            "{name}: continuation escaped refusal"
        );
    };
    run("accepted", &worker, &tests, &package, true);
    assert!(tests.contains("capacity::process_output_self_controls"));
    run(
        "wrong-selector-namespace",
        &worker,
        &tests.replace(
            "capacity::process_output_self_controls",
            "unrelated::process_output_self_controls",
        ),
        &package,
        false,
    );
    run(
        "wrong-package-selector",
        &worker,
        &tests,
        &package.replace(
            "relocated_runtime_refuses_missing_and_tampered_assets",
            "unrelated_control",
        ),
        false,
    );
    assert!(tests.contains("token_metadata_tests::"));
    let missing_family = tests.replace("token_metadata_tests::", "unrelated::");
    run("missing-family", &worker, &missing_family, &package, false);
    let frontier_call = "\"$RECEIPT_CHECKER\" frontier \"$CARGO_LOG\"";
    assert!(worker.contains(frontier_call));
    run(
        "frontier-bypass",
        &worker.replace(frontier_call, ":"),
        &missing_family,
        &package,
        true,
    );
    let extra_portable = package.replace(
        "test result: ok. 1 passed;",
        "test unrelated ... ok\ntest result: ok. 1 passed;",
    );
    assert_ne!(extra_portable, package);
    run("extra-portable", &worker, &tests, &extra_portable, false);
    let portable_call = "\"$RECEIPT_CHECKER\" portable \"$PORTABLE_LOG\"";
    assert!(worker.contains(portable_call));
    run(
        "portable-bypass",
        &worker.replace(portable_call, ":"),
        &tests,
        &extra_portable,
        true,
    );
    fs::remove_dir_all(root).unwrap();
    println!(
        "PASS saved receipts, four worker refusals and independently exposed receipt dispatch bypasses (offline)"
    );
}
