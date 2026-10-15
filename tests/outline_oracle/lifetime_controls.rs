#[test]
fn overlap_and_barrier_stop_controls() {
    use std::{
        io::Read,
        os::unix::{fs::PermissionsExt, process::ExitStatusExt},
    };
    const TEST: &str = "publication_contention::overlap_and_barrier_stop_controls";
    if let Ok(case) = std::env::var("LIFETIME_CONTROL") {
        let root = PathBuf::from(std::env::var_os("SHARED_OBSERVER_EVIDENCE").unwrap());
        if std::env::var_os("LIFETIME_FAKE").is_some() {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input).unwrap();
            let request: Value = serde_json::from_str(&input).unwrap();
            fs::write(
                root.join(format!("spawned-{}", std::process::id())),
                "ready",
            )
            .unwrap();
            if case.starts_with("barrier") && (case.contains("success") || case.contains("action"))
            {
                let barrier = &request["pause_before_accept"];
                fs::write(barrier["ready"].as_str().unwrap(), "ready").unwrap();
                while !Path::new(barrier["release"].as_str().unwrap()).exists() {
                    std::thread::sleep(Duration::from_millis(5));
                }
                let bytes = br#"{"status":"stale","version":null,"nodes":[],"links":[]}"#;
                assert_eq!(
                    unsafe { libc::write(3, bytes.as_ptr().cast(), bytes.len()) },
                    bytes.len() as isize
                );
                std::process::exit(1);
            }
            if case.starts_with("overlap") && case.contains("success") {
                let began = Instant::now();
                while fs::read_dir(&root)
                    .unwrap()
                    .filter_map(Result::ok)
                    .filter(|e| e.file_name().to_string_lossy().starts_with("spawned-"))
                    .count()
                    < 3
                {
                    assert!(began.elapsed() < Duration::from_secs(5));
                    std::thread::sleep(Duration::from_millis(5));
                }
                let bytes = br#"{"status":"current","version":"synthetic"}"#;
                assert_eq!(
                    unsafe { libc::write(3, bytes.as_ptr().cast(), bytes.len()) },
                    bytes.len() as isize
                );
                std::process::exit(0);
            }
            loop {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let signal = if case.ends_with("int") {
            libc::SIGINT
        } else {
            libc::SIGTERM
        };
        let dispositions = || {
            [libc::SIGTERM, libc::SIGINT].map(|s| unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                assert_eq!(libc::sigaction(s, std::ptr::null(), &mut action), 0);
                action.sa_sigaction
            })
        };
        let before = dispositions();
        if !case.contains("original") && !case.contains("late") {
            capacity::install_stop();
        }
        if case.contains("before") {
            assert_eq!(unsafe { libc::raise(signal) }, 0);
        }
        if case.contains("late") {
            let markers = root.clone();
            std::thread::spawn(move || {
                let began = Instant::now();
                while !fs::read_dir(&markers)
                    .unwrap()
                    .filter_map(Result::ok)
                    .any(|e| e.file_name().to_string_lossy().starts_with("spawned-"))
                {
                    assert!(began.elapsed() < Duration::from_secs(5));
                    std::thread::sleep(Duration::from_millis(5));
                }
                capacity::install_stop();
                assert_eq!(unsafe { libc::raise(signal) }, 0);
            });
        }
        if case.starts_with("overlap") {
            let jobs = ["index", "outline_inspect", "index"].map(|op| start(json!({"op":op})));
            for job in jobs {
                assert_eq!(current(job, "overlap control")["status"], "current");
            }
        } else {
            paused_index(
                &root,
                "source-replacement",
                || {
                    fs::write(root.join("acted"), "mutation").unwrap();
                    if case.contains("action") {
                        assert_eq!(unsafe { libc::raise(signal) }, 0);
                    }
                },
                false,
            );
        }
        if case.contains("original") {
            assert_eq!(before, dispositions());
        }
        fs::write(root.join("next-call"), "continued").unwrap();
        return;
    }
    for family in ["overlap", "barrier"] {
        for phase in [
            "success-original",
            "success-installed",
            "before-term",
            "before-int",
            "during-term",
            "during-int",
            "late-term",
            "late-int",
            "action-term",
            "action-int",
        ] {
            if family == "overlap" && phase.starts_with("action") {
                continue;
            }
            let case = format!("{family}-{phase}");
            let temp = TempDir::new().unwrap();
            let persistent =
                std::env::var_os("LIFETIME_CONTROL_EVIDENCE").map(|p| PathBuf::from(p).join(&case));
            let root = persistent.as_deref().unwrap_or(temp.path());
            if persistent.is_some() {
                fs::create_dir(root).unwrap();
            }
            let bin = root.join("build/debug/core-observe");
            fs::create_dir_all(bin.parent().unwrap()).unwrap();
            fs::write(&bin,format!("#!/bin/sh\nexec 3>&1\nLIFETIME_FAKE=1 exec \"$LIFETIME_EXE\" --exact {TEST} >/dev/null\n")).unwrap();
            fs::set_permissions(&bin, fs::Permissions::from_mode(0o700)).unwrap();
            let exe = std::env::current_exe().unwrap();
            let mut child = Command::new(&exe)
                .args(["--exact", TEST, "--nocapture"])
                .env("LIFETIME_CONTROL", &case)
                .env("SHARED_OBSERVER_EVIDENCE", root)
                .env("CORE_OBSERVE_BIN", &bin)
                .env("LIFETIME_EXE", &exe)
                .env("CAPACITY_PROCESS_CONTROL", "lifetime")
                .stdout(fs::File::create(root.join("output.log")).unwrap())
                .stderr(fs::File::create(root.join("error.log")).unwrap())
                .spawn()
                .unwrap();
            let began = Instant::now();
            let mut signalled = None;
            let status = loop {
                if phase.starts_with("during") && signalled.is_none() {
                    let count = fs::read_dir(root)
                        .unwrap()
                        .filter_map(Result::ok)
                        .filter(|e| e.file_name().to_string_lossy().starts_with("spawned-"))
                        .count();
                    if count == if family == "overlap" { 3 } else { 1 } {
                        assert_eq!(
                            unsafe {
                                libc::kill(
                                    child.id() as i32,
                                    if phase.ends_with("int") {
                                        libc::SIGINT
                                    } else {
                                        libc::SIGTERM
                                    },
                                )
                            },
                            0
                        );
                        signalled = Some(Instant::now());
                    }
                }
                if let Some(status) = child.try_wait().unwrap() {
                    break status;
                }
                if began.elapsed() > Duration::from_secs(6)
                    || signalled.is_some_and(|t| t.elapsed() > Duration::from_secs(1))
                {
                    for entry in fs::read_dir(root).unwrap().filter_map(Result::ok) {
                        if let Ok(bytes) = fs::read(entry.path().join("request.start.json")) {
                            let started: Value = serde_json::from_slice(&bytes).unwrap();
                            unsafe {
                                libc::kill(
                                    -(started["pgid"].as_i64().unwrap() as i32),
                                    libc::SIGKILL,
                                );
                            }
                        }
                    }
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("lifetime control hung: {case}");
                }
                std::thread::sleep(Duration::from_millis(5));
            };
            assert_eq!(
                status.signal(),
                None,
                "{case}: killed harness instead of refusal"
            );
            let success = phase.starts_with("success");
            assert_eq!(
                status.success(),
                success,
                "{case}: {}",
                fs::read_to_string(root.join("error.log")).unwrap()
            );
            assert_eq!(
                root.join("next-call").exists(),
                success,
                "{case}: later continuation"
            );
            if family == "barrier" {
                assert_eq!(
                    root.join("acted").exists(),
                    success || phase.starts_with("action"),
                    "{case}"
                );
            }
            let directories: Vec<_> = fs::read_dir(root)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().starts_with("lifetime-"))
                .collect();
            if phase.starts_with("before") {
                assert!(
                    !fs::read_dir(root)
                        .unwrap()
                        .filter_map(Result::ok)
                        .any(|e| e.file_name().to_string_lossy().starts_with("spawned-"))
                );
            } else if !success {
                assert!(
                    !directories.is_empty(),
                    "{case}: missing cancellation evidence"
                );
                for directory in directories {
                    let receipt: Value = serde_json::from_slice(
                        &fs::read(directory.path().join("request.receipt.json")).unwrap(),
                    )
                    .unwrap();
                    assert_eq!(receipt["drained"], true, "{case}: {receipt}");
                    let pgid = receipt["pgid"].as_i64().unwrap() as i32;
                    assert_eq!(
                        unsafe { libc::kill(-pgid, 0) },
                        -1,
                        "{case}: surviving request group"
                    );
                    assert_eq!(
                        std::io::Error::last_os_error().raw_os_error(),
                        Some(libc::ESRCH)
                    );
                }
            }
            eprintln!("LIFETIME_CONTROL_CHECKED {case}");
        }
    }
}
