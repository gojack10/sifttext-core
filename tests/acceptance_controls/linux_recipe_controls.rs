//! Offline launcher discriminators. Arguments: RECIPE RECEIPT_CHECKER.
//! No Colima, systemd, Cargo, Bend or native Core operation is executed.
use std::{env, fs, path::Path, process::Command};
fn snippet(text: &str, start: &str, end: &str) -> String {
    let start = text.find(start).expect("missing maintained section start");
    let end = text[start..]
        .find(end)
        .expect("missing maintained section end")
        + start;
    text[start..end].into()
}
fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    assert!(
        matches!(args.len(), 2 | 3),
        "RECIPE RECEIPT_CHECKER [probe]"
    );
    let recipe = Path::new(&args[0]);
    let root = env::temp_dir().join(format!(
        "sifttext-linux-recipe-controls-{}",
        std::process::id()
    ));
    fs::create_dir(&root).unwrap();
    let host = fs::read_to_string(recipe.join("host.sh")).unwrap();
    let guest = fs::read_to_string(recipe.join("guest.sh")).unwrap();
    let control = fs::read_to_string(recipe.join("guest-control.sh")).unwrap();
    for entry in fs::read_dir(recipe).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|s| s == "sh") {
            assert!(
                Command::new("sh")
                    .arg("-n")
                    .arg(&path)
                    .status()
                    .unwrap()
                    .success(),
                "{}",
                path.display()
            );
        }
    }
    // Extract the shipped argument guard, not a duplicate implementation of its bounds.
    let bounds = root.join("bounds.sh");
    fs::write(
        &bounds,
        format!(
            "set -eu\n{}\nprintf '%s %s\\n' \"$UNIT_SECONDS\" \"$STAGES\"\n",
            snippet(&host, "UNIT_SECONDS=", "SRC=")
        ),
    )
    .unwrap();
    for seconds in ["1", "900", "7200", "86400"] {
        for stage in ["all", "tests", "package"] {
            let output = Command::new("sh")
                .arg(&bounds)
                .args(["x", "x", "x", "x", "x", "x", seconds, stage])
                .output()
                .unwrap();
            assert!(output.status.success());
            assert_eq!(
                String::from_utf8(output.stdout).unwrap().trim(),
                format!("{seconds} {stage}")
            );
        }
    }
    let default = Command::new("sh")
        .arg(&bounds)
        .args(["x", "x", "x", "x", "x", "x", "900"])
        .output()
        .unwrap();
    assert!(default.status.success());
    assert_eq!(String::from_utf8(default.stdout).unwrap().trim(), "900 all");
    for bad in [
        "",
        "0",
        "00",
        "0900",
        "-1",
        "+1",
        "1.5",
        "900s",
        "infinity",
        "1;exit 0",
        "86401",
        "999999999999999999999999",
    ] {
        let output = Command::new("sh")
            .arg(&bounds)
            .args(["x", "x", "x", "x", "x", "x", bad, "tests"])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "false bound admission: {bad}"
        );
    }
    for bad in ["", "wrong"] {
        assert_eq!(
            Command::new("sh")
                .arg(&bounds)
                .args(["x", "x", "x", "x", "x", "x", "900", bad])
                .output()
                .unwrap()
                .status
                .code(),
            Some(2)
        );
    }
    let line = host
        .lines()
        .find(|s| s.contains("--property=Type=exec "))
        .unwrap()
        .trim_end_matches('\\');
    let props = Command::new("sh")
        .args([
            "-c",
            &format!("UNIT_SECONDS=7200; set -- {line}; printf '%s\\n' \"$@\""),
        ])
        .output()
        .unwrap();
    assert!(props.status.success());
    assert_eq!(
        String::from_utf8(props.stdout).unwrap(),
        "--property=Type=exec\n--property=KillMode=control-group\n--property=TimeoutStopSec=1s\n--property=RuntimeMaxSec=7200s\n"
    );
    // Root custody and transfer remain one contract. Do not silently revive a second root.
    let root_line = guest.lines().find(|s| s.starts_with("Q=")).unwrap();
    assert!(!root_line.trim_start_matches("Q=").is_empty());
    assert!(host.contains("sed -n 's/^Q=//p'"));
    let rust_custody = control.contains("exec ./linux-custody ");
    if rust_custody {
        let check = control
            .find("sha256sum -c linux-custody.executable.sha256")
            .unwrap();
        let dispatch = control
            .find("exec ./linux-custody \"$MODE\" \"$C\" \"$U\"")
            .unwrap();
        assert!(check < dispatch, "unchecked custody executable dispatch");
        assert!(guest.contains("sh \"$C/guest-control.sh\" admit \"$C\" \"$U\""));
        assert!(
            host.contains(
                "sh \"$HERE/../prepare-custody.sh\" \"$SRC\" \"$REV\" \"$E\" \"$P\" \"$C\""
            )
        );
    } else {
        assert!(
            control.contains("sed -n 's/^Q=//p'"),
            "evidence verifier lost guest root derivation"
        );
    }
    // Rust evidence_path behavior is checked by linux-custody.rs's standalone
    // controls; this fixture checks the transport's hashed dispatch boundary.
    for text in [&host, &control] {
        assert!(
            !text
                .lines()
                .any(|s| s.contains("/home/jack.guest/") && s.contains("/identity-resume")),
            "independent hardcoded evidence root"
        );
    }
    assert!(
        host.contains("for F in guest.sh guest-control.sh package-stage.sh isolation-stage.sh; do")
    );
    assert!(
        guest.contains("sh \"$C/package-stage.sh\" \"$PWD\" \"$E\" \"$REV\" \"$E/donor.sha256\"")
    );
    assert!(
        host.contains("FAMILY=${4-"),
        "explicit empty family must not inherit a tests-stage default"
    );
    // Guest stage guard must refuse coverage arguments before control-directory effects.
    for var in [
        "SIFTTEXT_IDENTITY_TEST",
        "SIFTTEXT_FAMILY",
        "SIFTTEXT_TARGETS",
        "SIFTTEXT_REQUIRED",
    ] {
        let mut command = Command::new("sh");
        command.arg(recipe.join("guest.sh"));
        for name in [
            "SIFTTEXT_IDENTITY_TEST",
            "SIFTTEXT_FAMILY",
            "SIFTTEXT_TARGETS",
            "SIFTTEXT_REQUIRED",
        ] {
            command.env_remove(name);
        }
        let output = command
            .env("SIFTTEXT_REV", "1".repeat(40))
            .env("SIFTTEXT_STAGES", "package")
            .env("SIFTTEXT_CONTROL_DIR", root.join("absent"))
            .env("SIFTTEXT_UNIT", "guard-probe")
            .env(var, "tests_stage_control")
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "package stage admitted {var}"
        );
    }
    // Synthetic producers let the maintained selection block run without native/build work.
    let source = root.join("source");
    let fake_control = root.join("control");
    fs::create_dir_all(source.join("scripts")).unwrap();
    fs::create_dir_all(source.join("target/debug")).unwrap();
    fs::create_dir(&fake_control).unwrap();
    fs::copy(&args[1], source.join("target/debug/acceptance-receipts")).unwrap();
    fs::write(source.join("scripts/with-tools.sh"), "printf '%s\\n' \"$*\" >> \"$E/invocations\"\ncase \"$*\" in *--list*) printf 'required_control: test\\n';; *) printf '     Running tests/specimen.rs (synthetic)\\ntest required_control ... ok\\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\\n';; esac\n").unwrap();
    fs::write(
        fake_control.join("package-stage.sh"),
        "touch \"$2/package-ran\"\n",
    )
    .unwrap();
    let selection = root.join("selection.sh");
    fs::write(
        &selection,
        format!("set -eu\n{}", snippet(&guest, "COUNT=0", "sha256sum -c")),
    )
    .unwrap();
    for (stage, required, passes) in [
        ("all", "", true),
        ("tests", "", true),
        ("package", "", true),
        ("package", "required_control", false),
    ] {
        let evidence = root.join(format!("{stage}-{required}"));
        fs::create_dir(&evidence).unwrap();
        let output = Command::new("sh")
            .arg(&selection)
            .current_dir(&source)
            .env("SRC", &source)
            .env("CHECK", source.join("target/debug/acceptance-receipts"))
            .env("C", &fake_control)
            .env("E", &evidence)
            .env("REV", "synthetic")
            .env("STAGES", stage)
            .env("IDENTITY_TEST", "")
            .env("FAMILY", "")
            .env("TARGETS", "specimen")
            .env("REQUIRED", required)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            passes,
            "{stage} {required}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if passes {
            assert_eq!(evidence.join("invocations").exists(), stage != "package");
            assert_eq!(evidence.join("package-ran").exists(), stage != "tests");
        }
    }
    // Execute the actual package stage with generated, non-native producers. Parser
    // unit coverage alone cannot establish install/isolation ordering or command failure.
    let package_source = root.join("package-source");
    let package_stage = root.join("package-stage");
    let bin = root.join("bin");
    for path in [
        package_source.join("scripts"),
        package_source.join("target/debug"),
        package_source.join("build"),
        package_stage.clone(),
        bin.clone(),
    ] {
        fs::create_dir_all(path).unwrap();
    }
    fs::copy(
        &args[1],
        package_source.join("target/debug/acceptance-receipts"),
    )
    .unwrap();
    fs::copy(
        recipe.join("package-stage.sh"),
        package_stage.join("package-stage.sh"),
    )
    .unwrap();
    fs::write(root.join("donor.sha256"), "synthetic donor manifest\n").unwrap();
    fs::write(
        bin.join("git"),
        "#!/bin/sh\ncase \"$*\" in *rev-parse*) printf '%s\\n' \"$REV\";; esac\n",
    )
    .unwrap();
    fs::write(bin.join("shasum"), "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in ["git", "shasum"] {
            fs::set_permissions(bin.join(name), fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    let portable = "relocated_runtime_refuses_missing_and_tampered_assets";
    for mode in [
        "pass",
        "zero",
        "missing",
        "failed",
        "ignored",
        "filtered",
        "artifacts",
        "isolation",
        "absent-build",
    ] {
        let evidence = root.join(format!("package-{mode}"));
        fs::create_dir(&evidence).unwrap();
        let build = if mode == "absent-build" {
            ""
        } else {
            "mkdir \"$1/build\"\n"
        };
        fs::write(package_source.join("install-local.sh"), format!("mkdir -p \"$1\" \"$2\"\n{build}printf '#!/bin/sh\\nprintf runtime-checked\\n' > \"$1/sifttext\"\nchmod +x \"$1/sifttext\"\n")).unwrap();
        fs::write(
            package_stage.join("isolation-stage.sh"),
            format!(
                "printf 'isolation\\n' >> \"$3/order\"\nexit {}\n",
                usize::from(mode == "isolation")
            ),
        )
        .unwrap();
        let list = match mode {
            "zero" => "0 tests, 0 benchmarks\n".into(),
            "missing" => "wrong_name: test\n".into(),
            _ => format!("{portable}: test\n"),
        };
        let workflow = if mode == "artifacts" {
            ""
        } else {
            "PORTABLE_WORKFLOW_CHECKED nodes=78\n"
        };
        let result = match mode {
            "ignored" => "0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out;",
            "filtered" => "1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out;",
            _ => "1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;",
        };
        let log = format!(
            "     Running tests/portable_runtime.rs (synthetic)\ntest {portable} ... RUNTIME_ASSET_REFUSAL missing/tampered first\n{}ok\n{workflow}RELOCATED_RUNTIME_CHECKED package\ntest result: ok. {result} finished in 0.00s\n",
            "RUNTIME_ASSET_REFUSAL missing/tampered next\n".repeat(7)
        );
        fs::write(evidence.join("producer.list"), list).unwrap();
        fs::write(evidence.join("producer.log"), log).unwrap();
        fs::write(package_source.join("scripts/with-tools.sh"), format!("case \"$*\" in *--list*) printf 'list\\n' >> \"$E/order\"; cat \"$E/producer.list\";; *) printf 'portable\\n' >> \"$E/order\"; cat \"$E/producer.log\"; exit {};; esac\n", usize::from(mode == "failed"))).unwrap();
        let output = Command::new("sh")
            .arg(package_stage.join("package-stage.sh"))
            .arg(&package_source)
            .arg(&evidence)
            .arg("1".repeat(40))
            .arg(root.join("donor.sha256"))
            .env(
                "PATH",
                format!("{}:{}", bin.display(), env::var("PATH").unwrap()),
            )
            .env("REV", "1".repeat(40))
            .env("E", &evidence)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            mode == "pass",
            "package {mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if mode == "pass" {
            assert_eq!(
                fs::read_to_string(evidence.join("order")).unwrap(),
                "isolation\nlist\nportable\n"
            );
        }
        if evidence.join("package.path").exists() {
            let path = fs::read_to_string(evidence.join("package.path")).unwrap();
            assert!(path.trim().starts_with("/tmp/sifttext-linux-package."));
            fs::remove_dir_all(path.trim()).unwrap();
        }
    }
    // The private aggregate stage may add a source-mode preflight above these
    // transports. Exercise that exact block: empty scans and unsupported modes
    // must not skip source binding. The fake Git producer supplies only status.
    let stage = recipe.parent().unwrap().join("stage.sh");
    if stage.exists() {
        let stage_text = fs::read_to_string(stage).unwrap();
        let rust_stage = stage_text.contains("RECEIPT_MODES=$(sh");
        let mode_guard = root.join("mode-guard.sh");
        fs::write(
            &mode_guard,
            format!(
                "set -eu\n{}",
                snippet(
                    &stage_text,
                    if rust_stage {
                        "RECEIPT_MODES="
                    } else {
                        "GUEST_MODES="
                    },
                    "if [ \"$ACTION\""
                )
            ),
        )
        .unwrap();
        let mode_dir = root.join("mode-recipe");
        fs::create_dir(&mode_dir).unwrap();
        if rust_stage {
            // Source/mode algorithms run in the exact compiled Rust checker;
            // this stand-in only avoids a second compiler invocation per mutation.
            fs::write(mode_dir.join("host-preflight.sh"), "exec \"$SIFTTEXT_STAGE_CHECKER\" \"$FINAL_SRC\" \"$FINAL_REV\" stage \"$STAGE\" \"$DIR\"\n").unwrap();
        }
        fs::write(
            mode_dir.join("package-stage.sh"),
            "\"$CHECK\" linux-package list log\n",
        )
        .unwrap();
        fs::write(
            mode_dir.join("isolation-stage.sh"),
            "\"$CHECK\" linux-isolation-before a b c d\n",
        )
        .unwrap();
        fs::write(
            bin.join("git"),
            "#!/bin/sh\ncase \"$*\" in *linux-unsupported*) exit 1;; *) exit 0;; esac\n",
        )
        .unwrap();
        for (name, guest_text, expected) in [
            (
                "mode-pass",
                "\"$CHECK\" linux-count\n\"$CHECK\" linux-exact\n\"$CHECK\" linux-target\n\"$CHECK\" linux-required\n",
                true,
            ),
            (
                "mode-zero",
                "old shell assertions without checker dispatch\n",
                false,
            ),
            (
                "mode-comments",
                "# linux-count linux-exact linux-target linux-required\n",
                !rust_stage,
            ),
            (
                "mode-unsupported",
                "\"$CHECK\" linux-count\n\"$CHECK\" linux-exact\n\"$CHECK\" linux-target\n\"$CHECK\" linux-unsupported\n",
                false,
            ),
        ] {
            fs::write(mode_dir.join("guest.sh"), guest_text).unwrap();
            let output = Command::new("sh")
                .arg(&mode_guard)
                .env("DIR", &mode_dir)
                .env("STAGE", "package")
                .env("UNIT_SECS", "900")
                .env("STAGES", "package")
                .env("FINAL_SRC", &package_source)
                .env("FINAL_REV", "1".repeat(40))
                .env(
                    "PATH",
                    format!("{}:{}", bin.display(), env::var("PATH").unwrap()),
                )
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                expected,
                "{name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    if args.len() == 2 {
        for (name, file, old, new) in [
            ("bound-bypass", "host.sh", "''|0*|*[!0-9]*)", "''|*[!0-9]*)"),
            (
                "stop-grace",
                "host.sh",
                "--property=TimeoutStopSec=1s",
                "--property=TimeoutStopSec=9s",
            ),
            (
                "root-custody",
                "guest-control.sh",
                if rust_custody {
                    "exec ./linux-custody "
                } else {
                    "sed -n 's/^Q=//p'"
                },
                if rust_custody {
                    "exec false "
                } else {
                    "sed -n 's/^OTHER=//p'"
                },
            ),
            (
                "stage-guard",
                "guest.sh",
                "package) case",
                "disabled-package) case",
            ),
            (
                "stage-dispatch",
                "guest.sh",
                "if [ \"$STAGES\" != package ]; then",
                "if true; then",
            ),
        ] {
            let mutated = root.join(name);
            fs::create_dir(&mutated).unwrap();
            for entry in fs::read_dir(recipe).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().is_some_and(|s| s == "sh") {
                    fs::copy(&path, mutated.join(path.file_name().unwrap())).unwrap();
                }
            }
            let path = mutated.join(file);
            let text = fs::read_to_string(&path).unwrap();
            assert!(text.contains(old), "mutation no longer applies: {name}");
            fs::write(path, text.replacen(old, new, 1)).unwrap();
            let child = Command::new(env::current_exe().unwrap())
                .arg(&mutated)
                .arg(&args[1])
                .arg("probe")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let id = child.id();
            let output = child.wait_with_output().unwrap();
            fs::write(
                root.join(format!("{name}.log")),
                [&output.stdout[..], &output.stderr[..]].concat(),
            )
            .unwrap();
            assert!(!output.status.success(), "checker admitted bypass: {name}");
            fs::remove_dir_all(
                env::temp_dir().join(format!("sifttext-linux-recipe-controls-{id}")),
            )
            .unwrap();
        }
    }
    fs::remove_dir_all(root).unwrap();
    println!("Linux envelope/stage Rust controls passed (offline; no native qualification)");
}
