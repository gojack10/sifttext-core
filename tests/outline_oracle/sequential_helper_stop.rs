// Actual graph/assembler/Git callers; sticky handlers are isolated in child harnesses.
#[test]
fn sequential_graph_and_assembler_stop_controls() {
    use std::os::unix::{fs::PermissionsExt, process::ExitStatusExt};
    use std::time::{Duration, Instant};
    if let Ok(case) = std::env::var("SEQUENTIAL_HELPER_CONTROL") {
        let root = PathBuf::from(std::env::var_os("SHARED_OBSERVER_EVIDENCE").unwrap());
        if case != "original" && !case.starts_with("late") {
            capacity::install_stop();
        }
        let signal = if case.ends_with("int") {
            libc::SIGINT
        } else {
            libc::SIGTERM
        };
        if case.starts_with("before") {
            assert_eq!(unsafe { libc::raise(signal) }, 0);
        }
        if case.starts_with("late") {
            let ready = root.join("ready");
            std::thread::spawn(move || {
                let start = Instant::now();
                while !ready.exists() {
                    assert!(start.elapsed() < Duration::from_secs(5));
                    std::thread::sleep(Duration::from_millis(5));
                }
                capacity::install_stop();
                assert_eq!(unsafe { libc::kill(std::process::id() as i32, signal) }, 0);
            });
        }
        let dispositions = || {
            [libc::SIGTERM, libc::SIGINT].map(|signal| unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                assert_eq!(libc::sigaction(signal, std::ptr::null(), &mut action), 0);
                action.sa_sigaction
            })
        };
        let before = dispositions();
        let caller = std::env::var("SEQUENTIAL_HELPER_CALLER").unwrap();
        if caller == "git" {
            let result =
                std::panic::catch_unwind(|| dogfood_git(&root, &["status", "--porcelain"]));
            if case == "failed" {
                let error = result.unwrap_err();
                assert!(
                    error
                        .downcast_ref::<String>()
                        .unwrap()
                        .contains("helper-err")
                );
            } else {
                assert_eq!(result.unwrap(), "helper-out");
            }
        } else {
            let out = if caller == "graph" {
                dogfood_graph_output(&[Path::new("inspect"), &root])
            } else {
                assembler_output(b"exact assembler input\n")
            };
            assert_eq!(
                out.status.code(),
                Some(if case == "failed" { 1 } else { 0 })
            );
            assert_eq!(out.stdout, b"helper-out");
            assert_eq!(out.stderr, b"helper-err");
        }
        if case == "original" {
            assert_eq!(dispositions(), before);
        }
        fs::write(root.join("next-call"), "started").unwrap();
        return;
    }
    for caller in ["graph", "assembler", "git"] {
        for case in [
            "original",
            "success",
            "failed",
            "before-term",
            "before-int",
            "during-term",
            "during-int",
            "late-term",
            "late-int",
        ] {
            let dir = TempDir::new().unwrap();
            let persistent = std::env::var_os("SEQUENTIAL_HELPER_EVIDENCE")
                .map(|path| PathBuf::from(path).join(format!("{caller}-{case}")));
            let root = persistent.as_deref().unwrap_or(dir.path());
            if persistent.is_some() {
                fs::create_dir(root).unwrap();
            }
            let bin = root.join("git");
            fs::write(&bin, if case.starts_with("during") || case.starts_with("late") {
                "#!/bin/sh\nprintf ready > \"$SHARED_OBSERVER_EVIDENCE/ready\"\nexec sleep 30\n"
            } else {
                "#!/bin/sh\nprintf spawned > \"$SHARED_OBSERVER_EVIDENCE/spawned\"\nprintf '%s\\n' \"$@\" > \"$SHARED_OBSERVER_EVIDENCE/args\"\ncat > \"$SHARED_OBSERVER_EVIDENCE/input\"\nprintf helper-out\nprintf helper-err >&2\n[ \"$SEQUENTIAL_HELPER_CONTROL\" != failed ]\n"
            }).unwrap();
            fs::set_permissions(&bin, fs::Permissions::from_mode(0o700)).unwrap();
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "sequential_graph_and_assembler_stop_controls",
                    "--nocapture",
                ])
                .env(
                    "PATH",
                    std::env::join_paths(
                        std::iter::once(root.to_owned())
                            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
                    )
                    .unwrap(),
                )
                .env("SEQUENTIAL_HELPER_CONTROL", case)
                .env("SEQUENTIAL_HELPER_CALLER", caller)
                .env("SHARED_OBSERVER_EVIDENCE", root)
                .env("CORE_GRAPH_BIN", &bin)
                .env("CORE_OUTLINE_FOREST_BIN", &bin)
                .env("CAPACITY_PROCESS_CONTROL", "sequential-helper")
                .stdout(fs::File::create(root.join("output.log")).unwrap())
                .stderr(fs::File::create(root.join("error.log")).unwrap())
                .spawn()
                .unwrap();
            let start = Instant::now();
            if case.starts_with("during") {
                while !root.join("ready").exists() {
                    assert!(start.elapsed() < Duration::from_secs(5));
                    assert!(child.try_wait().unwrap().is_none());
                    std::thread::sleep(Duration::from_millis(5));
                }
                let signal = if case.ends_with("int") {
                    libc::SIGINT
                } else {
                    libc::SIGTERM
                };
                assert_eq!(unsafe { libc::kill(child.id() as i32, signal) }, 0);
            }
            let status = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break status;
                }
                if start.elapsed() > Duration::from_secs(6) {
                    if let Ok(bytes) = fs::read(root.join("observer.start.json")) {
                        let started: Value = serde_json::from_slice(&bytes).unwrap();
                        unsafe {
                            libc::kill(-(started["pgid"].as_i64().unwrap() as i32), libc::SIGKILL);
                        }
                    }
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("helper Stop hung: {caller}-{case}");
                }
                std::thread::sleep(Duration::from_millis(5));
            };
            assert_eq!(
                status.signal(),
                None,
                "must refuse, not kill harness: {caller}-{case}"
            );
            let normal = matches!(case, "original" | "success" | "failed");
            assert_eq!(
                status.success(),
                normal,
                "{caller}-{case}: {}",
                fs::read_to_string(root.join("error.log")).unwrap()
            );
            assert_eq!(root.join("next-call").exists(), normal);
            if case.starts_with("before") {
                assert!(!root.join("spawned").exists());
                assert!(!root.join("observer.start.json").exists());
            } else {
                let receipt: Value =
                    serde_json::from_slice(&fs::read(root.join("observer.receipt.json")).unwrap())
                        .unwrap();
                assert_eq!(receipt["drained"], true);
                let pgid = receipt["pgid"].as_i64().unwrap() as i32;
                assert_eq!(unsafe { libc::kill(-pgid, 0) }, -1);
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::ESRCH)
                );
                assert_eq!(
                    receipt["cancelled_signal"],
                    if normal {
                        0
                    } else if case.ends_with("int") {
                        libc::SIGINT
                    } else {
                        libc::SIGTERM
                    }
                );
                if normal {
                    assert_eq!(
                        fs::read(root.join("input")).unwrap(),
                        if caller == "graph" || caller == "git" {
                            b"".as_slice()
                        } else {
                            b"exact assembler input\n".as_slice()
                        }
                    );
                    let args = fs::read_to_string(root.join("args")).unwrap();
                    if caller == "graph" {
                        assert_eq!(args, format!("inspect\n{}\n", root.display()));
                    } else if caller == "git" {
                        assert_eq!(
                            args,
                            format!(
                                "-C\n{}\n-c\nuser.name=Local Dogfood\n-c\nuser.email=dogfood@example.invalid\n-c\ncommit.gpgsign=false\n-c\ncore.hooksPath=/dev/null\nstatus\n--porcelain\n",
                                root.display()
                            )
                        );
                    }
                }
            }
        }
    }
}
