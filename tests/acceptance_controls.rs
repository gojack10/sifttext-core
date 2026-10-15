//! Offline source/receipt controls; no Cargo, Bend or native helpers are launched.
use std::{fs, path::Path, process::Command};
#[path = "../scripts/linux-source-controls.rs"]
#[allow(dead_code)]
mod linux_source_controls;

fn reset(root: &Path) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    for file in [
        "scripts/build.sh",
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
        fs::copy(source.join(file), destination).unwrap();
    }
    fs::write(
        root.join("scripts/tool-env.sh"),
        "echo TOOL_SETUP_REACHED >&2\nexit 93\n",
    )
    .unwrap();
}

fn change(root: &Path, file: &str, old: &str, new: &str) {
    let path = root.join(file);
    let text = fs::read_to_string(&path).unwrap();
    assert!(
        text.contains(old),
        "mutation no longer applies: {file} {old}"
    );
    fs::write(path, text.replacen(old, new, 1)).unwrap();
}

fn run(root: &Path, args: &[&str]) -> (bool, String) {
    let output = Command::new("sh")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    (
        output.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )
}

fn refuse_build(root: &Path, name: &str, reason: &str) {
    let (ok, text) = run(root, &["scripts/build.sh"]);
    assert!(
        !ok && !text.contains("TOOL_SETUP_REACHED") && text.contains(reason),
        "{name}: {text}"
    );
}

#[test]
fn source_and_executed_receipts_refuse_missing_or_bypassed_controls() {
    let root = std::path::PathBuf::from(format!(
        "/tmp/sifttext-acceptance-controls-{}",
        std::process::id()
    ));
    fs::create_dir(&root).unwrap();
    reset(&root);
    let (ok, text) = run(&root, &["scripts/check-acceptance-controls.sh"]);
    assert!(
        ok && text.contains("Acceptance source selectors checked: 11"),
        "{text}"
    );
    // A final record without a newline must not disappear from either stage.
    let inventory = fs::read_to_string(root.join("scripts/acceptance-controls.tsv")).unwrap();
    fs::write(
        root.join("scripts/acceptance-controls.tsv"),
        inventory.trim_end_matches('\n'),
    )
    .unwrap();
    let (ok, text) = run(&root, &["scripts/check-acceptance-controls.sh"]);
    assert!(
        ok && text.contains("Acceptance source selectors checked: 11"),
        "unterminated final record: {text}"
    );
    change(
        &root,
        "tests/portable_runtime.rs",
        "fn relocated_runtime_refuses_missing_and_tampered_assets",
        "fn renamed_package_control",
    );
    refuse_build(
        &root,
        "unterminated-missing-control",
        "Missing, duplicate or wrong-stage",
    );
    reset(&root);
    fs::remove_file(root.join("tests/global_git_identity.rs")).unwrap();
    refuse_build(&root, "missing-source", "Missing acceptance source");
    for (name, file, old, new, reason) in [
        (
            "renamed-control",
            "tests/outline_oracle/capacity.rs",
            "fn process_output_self_controls",
            "fn renamed_control",
            "Missing, duplicate or wrong-stage",
        ),
        (
            "ignored-default",
            "tests/outline_oracle/capacity.rs",
            "#[test]\n    fn process_output",
            "#[test]\n    #[ignore]\n    fn process_output",
            "Missing, duplicate or wrong-stage",
        ),
        (
            "unignored-package",
            "tests/portable_runtime.rs",
            "#[ignore =",
            "// removed ignore =",
            "Missing, duplicate or wrong-stage",
        ),
        (
            "unlinked-source",
            "tests/outline_file_oracle.rs",
            "include!(\"outline_oracle/capacity.rs\");",
            "// removed include",
            "Control source is not included",
        ),
        (
            "missing-test",
            "tests/global_git_identity.rs",
            "#[test]\nfn selected_init",
            "// removed test\nfn selected_init",
            "Missing, duplicate or wrong-stage",
        ),
        (
            "wrong-stage",
            "scripts/acceptance-controls.tsv",
            "package portable_runtime",
            "tests portable_runtime",
            "Missing, duplicate or wrong-stage",
        ),
    ] {
        reset(&root);
        change(&root, file, old, new);
        refuse_build(&root, name, reason);
    }
    reset(&root);
    let identity = root.join("tests/global_git_identity.rs");
    let mut text = fs::read_to_string(&identity).unwrap();
    text.push_str("\n#[test]\nfn selected_init_identity_is_local_verified_and_preserved() {\n}\n");
    fs::write(identity, text).unwrap();
    refuse_build(
        &root,
        "duplicate-control",
        "Missing, duplicate or wrong-stage",
    );
    reset(&root);
    fs::write(root.join("scripts/acceptance-controls.tsv"), "").unwrap();
    refuse_build(
        &root,
        "empty-inventory",
        "Empty acceptance control inventory",
    );
    reset(&root);
    fs::remove_file(root.join("tests/global_git_identity.rs")).unwrap();
    change(
        &root,
        "scripts/build.sh",
        "sh \"$ROOT/scripts/check-acceptance-controls.sh\"",
        ": # bypass guard",
    );
    let (_, text) = run(&root, &["scripts/build.sh"]);
    assert!(
        text.contains("TOOL_SETUP_REACHED"),
        "bypass mutation did not expose lost refusal: {text}"
    );
    reset(&root);
    let inventory = fs::read_to_string(root.join("scripts/acceptance-controls.tsv")).unwrap();
    let mut tests = String::new();
    let mut package = String::new();
    for line in inventory.lines().filter(|line| !line.starts_with('#')) {
        let row: Vec<_> = line.split_whitespace().collect();
        assert_eq!(row.len(), 4);
        let selector = if row[2] == "tests/outline_oracle/capacity.rs" {
            format!("capacity::{}", row[3])
        } else {
            row[3].to_owned()
        };
        let receipt = format!(
            "     Running tests/{}.rs (target/debug/deps/example)\ntest {selector} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n",
            row[1]
        );
        match row[0] {
            "tests" => tests.push_str(&receipt),
            "package" => package.push_str(&receipt),
            _ => panic!("unexpected stage"),
        }
    }
    fs::write(
        root.join("scripts/acceptance-controls.tsv"),
        inventory.trim_end_matches('\n'),
    )
    .unwrap();
    for (stage, receipt) in [("tests", &tests), ("package", &package)] {
        fs::write(root.join("receipt.log"), receipt).unwrap();
        let (ok, text) = run(
            &root,
            &["scripts/check-acceptance-controls.sh", stage, "receipt.log"],
        );
        assert!(ok, "{stage}: {text}");
    }
    let split = package.replace(" ... ok\n", " ... fixture output\nok\n");
    fs::write(root.join("receipt.log"), &split).unwrap();
    assert!(
        run(
            &root,
            &[
                "scripts/check-acceptance-controls.sh",
                "package",
                "receipt.log"
            ]
        )
        .0
    );
    for bad in [
        split.replace("\nok\n", "\nFAILED\n"),
        split.replace("\nok\n", "\n"),
        split.clone() + &split,
    ] {
        fs::write(root.join("receipt.log"), bad).unwrap();
        assert!(
            !run(
                &root,
                &[
                    "scripts/check-acceptance-controls.sh",
                    "package",
                    "receipt.log"
                ]
            )
            .0
        );
    }
    let incomplete = package
        .lines()
        .filter(|line| !line.starts_with("test result:"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    for (name, stage, receipt) in [
        (
            "missing-control",
            "tests",
            tests.replace("process_output_self_controls", "renamed_control"),
        ),
        (
            "wrong-target",
            "tests",
            tests.replace(
                "Running tests/outline_file_oracle.rs",
                "Running tests/wrong_target.rs",
            ),
        ),
        (
            "wrong-namespace",
            "tests",
            tests.replace(
                "capacity::process_output_self_controls",
                "unrelated::process_output_self_controls",
            ),
        ),
        (
            "failed-summary",
            "tests",
            tests.replace("1 passed; 0 failed", "0 passed; 1 failed"),
        ),
        (
            "ignored-control",
            "package",
            package.replace(" ... ok", " ... ignored"),
        ),
        (
            "zero-test-replay",
            "package",
            package.clone()
                + &package.replace(
                    "1 passed; 0 failed; 0 ignored",
                    "0 passed; 0 failed; 0 ignored",
                ),
        ),
        ("duplicate-execution", "package", package.clone() + &package),
        ("missing-terminal", "package", incomplete.clone()),
        ("overwritten-terminal", "package", incomplete + &package),
    ] {
        fs::write(root.join("receipt.log"), receipt).unwrap();
        let (ok, text) = run(
            &root,
            &["scripts/check-acceptance-controls.sh", stage, "receipt.log"],
        );
        assert!(
            !ok && text.contains("Required control did not execute once"),
            "{name}: {text}"
        );
    }
    fs::remove_dir_all(root).unwrap();
}
