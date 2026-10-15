// Isolate sticky signals in subprocesses; exercise the actual noncapacity adapter.
#[test]
fn shared_observer_after_capacity_stop_controls() {
    use std::os::unix::{fs::PermissionsExt, process::ExitStatusExt};
    use std::time::{Duration, Instant};
    if let Ok(case) = std::env::var("SHARED_OBSERVER_CONTROL") {
        let root = PathBuf::from(std::env::var_os("SHARED_OBSERVER_EVIDENCE").unwrap());
        if case != "original" && !case.starts_with("late") {
            capacity::install_stop();
        }
        if case.starts_with("late") {
            let ready = root.join("ready");
            let signal = if case.ends_with("term") {
                libc::SIGTERM
            } else {
                libc::SIGINT
            };
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
        if case.starts_with("before") {
            let signal = if case.ends_with("term") {
                libc::SIGTERM
            } else {
                libc::SIGINT
            };
            assert_eq!(unsafe { libc::raise(signal) }, 0);
        }
        let dispositions = || {
            [libc::SIGTERM, libc::SIGINT].map(|signal| unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                assert_eq!(libc::sigaction(signal, std::ptr::null(), &mut action), 0);
                action.sa_sigaction
            })
        };
        let before = dispositions();
        let (ok, value) = if std::env::var("SHARED_OBSERVER_CALLER").as_deref() == Ok("dogfood") {
            (
                true,
                dogfood_expected_request(&root, json!({"op":"outline_inspect"}), "current"),
            )
        } else {
            native_request(json!({"op":"outline_inspect"}))
        };
        if case == "original" {
            assert_eq!(dispositions(), before);
        }
        assert!(ok);
        assert_eq!(value, json!({"status":"current","files":[]}));
        fs::write(root.join("next-call"), "started").unwrap();
        return;
    }
    for (case, caller) in [
        "original",
        "success",
        "before-term",
        "before-int",
        "during-term",
        "during-int",
        "late-term",
        "late-int",
    ]
    .into_iter()
    .flat_map(|case| ["shared", "dogfood"].map(|caller| (case, caller)))
    {
        let dir = TempDir::new().unwrap();
        let persistent = std::env::var_os("SHARED_OBSERVER_CONTROL_EVIDENCE")
            .map(|path| PathBuf::from(path).join(format!("{caller}-{case}")));
        let root = persistent.as_deref().unwrap_or(dir.path());
        if persistent.is_some() {
            fs::create_dir(root).unwrap();
        }
        let bin = root.join("build/debug/core-observe");
        fs::create_dir_all(bin.parent().unwrap()).unwrap();
        // A no-descendant delayed observer lets group absence be checked independently.
        fs::write(&bin, if case.starts_with("during") || case.starts_with("late") {
            "#!/bin/sh\nprintf ready > \"$SHARED_OBSERVER_EVIDENCE/ready\"\nexec sleep 30\n"
        } else {
            "#!/bin/sh\nprintf spawned > \"$SHARED_OBSERVER_EVIDENCE/spawned\"\ncat >/dev/null\nprintf '{\"status\":\"current\",\"files\":[]}'\n"
        }).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o700)).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "shared_observer_after_capacity_stop_controls",
                "--nocapture",
            ])
            .env("SHARED_OBSERVER_CONTROL", case)
            .env("SHARED_OBSERVER_CALLER", caller)
            .env("SHARED_OBSERVER_EVIDENCE", root)
            .env("CORE_OBSERVE_BIN", &bin)
            .env("CAPACITY_PROCESS_CONTROL", "shared-observer")
            .stdout(fs::File::create(root.join("output.log")).unwrap())
            .stderr(Stdio::from(
                fs::File::create(root.join("error.log")).unwrap(),
            ))
            .spawn()
            .unwrap();
        let start = Instant::now();
        if case.starts_with("during") {
            while !root.join("ready").exists() {
                assert!(start.elapsed() < Duration::from_secs(5));
                assert!(child.try_wait().unwrap().is_none());
                std::thread::sleep(Duration::from_millis(5));
            }
            let signal = if case.ends_with("term") {
                libc::SIGTERM
            } else {
                libc::SIGINT
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
                panic!("shared observer Stop hung: {case}");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(
            status.signal(),
            None,
            "must refuse rather than merely kill harness: {case}"
        );
        if case == "original" || case == "success" {
            assert!(
                status.success(),
                "{case}: {}",
                fs::read_to_string(root.join("error.log")).unwrap()
            );
            assert!(root.join("next-call").exists());
        } else {
            assert!(!status.success(), "{case}: concealed Stop");
            assert!(!root.join("next-call").exists());
        }
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
                if case.starts_with("during") || case.starts_with("late") {
                    if case.ends_with("term") {
                        libc::SIGTERM
                    } else {
                        libc::SIGINT
                    }
                } else {
                    0
                }
            );
        }
    }
}
