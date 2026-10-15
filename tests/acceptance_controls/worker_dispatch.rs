//! Offline private recipe check: native commands only replay consumed receipts.
use std::{fs, path::PathBuf, process::Command};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).map(PathBuf::from).collect();
    assert_eq!(
        args.len(),
        5,
        "SOURCE WORKER CARGO_LOG PORTABLE_LOG EVIDENCE_DIR"
    );
    let source = &args[0];
    let evidence = &args[4];
    let root = PathBuf::from(format!(
        "/tmp/sifttext-worker-dispatch-rust-{}",
        std::process::id()
    ));
    fs::create_dir(&root).unwrap();
    for file in [
        "scripts/check-acceptance-controls.sh",
        "scripts/acceptance-controls.rs",
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
        fs::copy(source.join(file), destination).unwrap();
    }
    for dir in ["build", "target"] {
        fs::create_dir(root.join(dir)).unwrap();
    }
    fs::copy(&args[2], root.join("tests.log")).unwrap();
    fs::copy(&args[3], root.join("package.log")).unwrap();
    fs::write(root.join("scripts/build.sh"), ":\n").unwrap();
    fs::write(root.join("install-local.sh"), "mkdir -p \"$1\" \"$2\"\n").unwrap();
    fs::write(
        root.join("completion.sh"),
        "printf DISPATCH_COMPLETED > \"$PWD/completed\"\n",
    )
    .unwrap();
    fs::write(root.join("scripts/with-tools.sh"), r#"#!/bin/sh
case "$*" in
  'cargo test --locked -- --nocapture') cat "$PWD/tests.log" ;;
  'cargo test --locked --test portable_runtime -- --ignored --exact relocated_runtime_refuses_missing_and_tampered_assets --nocapture') cat "$PWD/package.log" ;;
  *) echo 'Unexpected offline tool dispatch' >&2; exit 1 ;;
esac
"#).unwrap();
    fs::copy(&args[1], root.join("worker.sh")).unwrap();
    let run = |name: &str| {
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
        fs::write(evidence.join(format!("rust-dispatch-{name}.log")), &text).unwrap();
        (output.status.success(), text)
    };
    let (ok, text) = run("accepted");
    assert!(ok && root.join("completed").is_file(), "{text}");
    for (name, file, old, new) in [
        (
            "wrong-cargo-namespace",
            "tests.log",
            "capacity::process_output_self_controls",
            "unrelated::process_output_self_controls",
        ),
        (
            "wrong-package-control",
            "package.log",
            "relocated_runtime_refuses_missing_and_tampered_assets",
            "unrelated_portable_control",
        ),
    ] {
        fs::copy(&args[2], root.join("tests.log")).unwrap();
        fs::copy(&args[3], root.join("package.log")).unwrap();
        let path = root.join(file);
        let receipt = fs::read_to_string(&path).unwrap();
        assert!(receipt.contains(old));
        fs::write(path, receipt.replace(old, new)).unwrap();
        if root.join("completed").exists() {
            fs::remove_file(root.join("completed")).unwrap();
        }
        let (ok, text) = run(name);
        assert!(
            !ok && !root.join("completed").exists()
                && text.contains("Required control did not execute once"),
            "{name}: {text}"
        );
    }
    // The wrong package remains; removing both hooks must expose continuation.
    let worker = fs::read_to_string(root.join("worker.sh")).unwrap();
    let bypass = worker
        .lines()
        .filter(|line| !line.starts_with("sh scripts/check-acceptance-controls.sh"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_ne!(worker, bypass);
    fs::write(root.join("worker.sh"), bypass).unwrap();
    let (ok, text) = run("bypassed-guards");
    assert!(ok && root.join("completed").is_file(), "{text}");
    fs::remove_dir_all(root).unwrap();
    println!(
        "PASS exact worker dispatch, both stage refusals, and guard-bypass mutation (offline only)"
    );
}
