//! Offline dispatch proof: saved package receipts only, no native/guest commands.
use std::{env, fs, path::PathBuf, process::Command};
fn main() {
    let args: Vec<_> = env::args().skip(1).map(PathBuf::from).collect();
    assert_eq!(
        args.len(),
        5,
        "SOURCE RECIPE SAVED_PACKAGE CHECKER EVIDENCE"
    );
    let root = env::temp_dir().join(format!(
        "sifttext-linux-host-dispatch-{}",
        std::process::id()
    ));
    fs::create_dir(&root).unwrap();
    fs::create_dir_all(root.join("target/debug")).unwrap();
    fs::copy(&args[3], root.join("target/debug/acceptance-receipts")).unwrap();
    let source = args[0].to_str().unwrap();
    let rev = Command::new("git")
        .args(["-C", source, "rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(rev.status.success());
    let rev = String::from_utf8(rev.stdout).unwrap();
    for (name, required, expected) in [
        (
            "source-pass",
            "selected_init_identity_is_local_verified_and_preserved",
            true,
        ),
        ("source-refuse", "absent_control", false),
    ] {
        let output = Command::new("sh")
            .arg(args[1].join("host-preflight.sh"))
            .args([
                source,
                rev.trim(),
                "",
                "",
                "global_git_identity",
                required,
                "1800",
                "tests",
            ])
            .output()
            .unwrap();
        fs::write(
            args[4].join(format!("{name}.log")),
            [&output.stdout[..], &output.stderr[..]].concat(),
        )
        .unwrap();
        assert_eq!(output.status.success(), expected);
    }
    let list = fs::read_to_string(args[2].join("package-tests.list")).unwrap();
    let log = fs::read_to_string(args[2].join("package-portable.log")).unwrap();
    let runtime = fs::read_to_string(args[2].join("isolation-before.json")).unwrap();
    let mut dispatch = String::new();
    for file in ["package-stage.sh", "isolation-stage.sh"] {
        let script = fs::read_to_string(args[1].join(file)).unwrap();
        for line in script
            .lines()
            .filter(|s| s.starts_with("\"$SRC/target/debug/acceptance-receipts\" "))
        {
            dispatch.push_str(line);
            dispatch.push('\n');
        }
    }
    assert_eq!(
        dispatch.lines().count(),
        3,
        "missing changed checker dispatch"
    );
    fs::write(root.join("dispatch.sh"), format!("set -eu\n{dispatch}")).unwrap();
    for (name, list, log, runtime, expected) in [
        (
            "saved-pass",
            list.clone(),
            log.clone(),
            runtime.clone(),
            true,
        ),
        (
            "missing-list",
            String::new(),
            log.clone(),
            runtime.clone(),
            false,
        ),
        (
            "missing-refusal",
            list.clone(),
            log.replacen("RUNTIME_ASSET_REFUSAL", "missing", 1),
            runtime.clone(),
            false,
        ),
        (
            "failed-control",
            list.clone(),
            log.replace("\nok\n", "\nFAILED\n"),
            runtime.clone(),
            false,
        ),
        (
            "unfinished-control",
            list.clone(),
            log.replace("\nok\n", "\n"),
            runtime.clone(),
            false,
        ),
        ("missing-runtime", list, log, "{}".into(), false),
    ] {
        fs::write(root.join("package-tests.list"), list).unwrap();
        fs::write(root.join("package-portable.log"), log).unwrap();
        fs::write(root.join("isolation-before.json"), runtime).unwrap();
        let output = Command::new("sh")
            .arg(root.join("dispatch.sh"))
            .env("SRC", &root)
            .env("E", &root)
            .output()
            .unwrap();
        fs::write(
            args[4].join(format!("{name}.log")),
            [&output.stdout[..], &output.stderr[..]].concat(),
        )
        .unwrap();
        assert_eq!(output.status.success(), expected, "{name}");
    }
    fs::remove_dir_all(root).unwrap();
    println!("Linux host/package dispatch controls passed (no native replay)");
}
