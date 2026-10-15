// Deep48/wide180 observations retained from incremental_outlines.mjs.
// ps/descriptor sampling is deliberately observational: it can miss short peaks
// and adds wall time; neither 250ms nor 250MB is an acceptance threshold.
#[path = "../common/native_resources.rs"]
#[expect(
    dead_code,
    reason = "This harness uses cancellable sampling; other harnesses use sample."
)]
mod native_resources;
use native_resources::{native_rss, resource_tree};
fn resource_call(store: &Path, label: &str, evidence: &Path) -> (Value, Value) {
    let bin = std::env::var_os("CORE_OBSERVE_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/core-observe"));
    let stdout = evidence.join(format!("{label}.stdout.json"));
    let stderr = evidence.join(format!("{label}.stderr.txt"));
    let mut command = Command::new("/usr/bin/time");
    #[cfg(target_os = "macos")]
    command.arg("-l");
    #[cfg(target_os = "linux")]
    command.arg("-v");
    command
        .arg(&bin)
        .current_dir(bin.parent().unwrap().parent().unwrap().parent().unwrap())
        .env("SIFTTEXT_RETRIEVAL_TRACE", "1");
    let request = json!({"op":"index","store":store,"mode":"incremental"});
    let start = Instant::now();
    let (output, transport) = capacity::bounded_sampled_request_without_install(
        command,
        request.to_string().into_bytes(),
        &evidence.join(format!("{label}-process")),
        Duration::from_secs(900),
        None,
        Duration::from_millis(20),
    );
    fs::write(stdout, &output.stdout).unwrap();
    fs::write(stderr, &output.stderr).unwrap();
    let status = output.status;
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(status.success(), "{label}: {status}; {stderr}");
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["status"], "current");
    let work: Value = serde_json::from_str(
        stderr
            .lines()
            .find_map(|l| l.strip_prefix("retrieval-work "))
            .expect("missing trace"),
    )
    .unwrap();
    let native_rss_bytes = native_rss(&stderr);
    let peak_rss = transport["sampled_helper_inclusive_rss_bytes"]
        .as_u64()
        .unwrap();
    let peak_fds = transport["sampled_numeric_fd_peak"].as_u64().unwrap();
    let samples = transport["samples"].as_array().unwrap().len();
    assert!(
        native_rss_bytes > 0 && peak_rss > 0 && peak_fds > 0,
        "{label}: missing resource observation"
    );
    let receipt = json!({"label":label,"request":request,"exit":status.code(),"ms":start.elapsed().as_secs_f64()*1000.,
        "native_rss_bytes":native_rss_bytes,"sampled_helper_inclusive_rss_bytes":peak_rss,"sampled_tree_fds":peak_fds,"samples":samples,"work":work,"process_receipt":evidence.join(format!("{label}-process.process.json"))});
    fs::write(
        evidence.join(format!("{label}.receipt.json")),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    eprintln!("{receipt}");
    (result, receipt)
}
#[test]
fn resource_request_stop_controls() {
    if let Ok(case) = std::env::var("RESOURCE_STOP_CONTROL") {
        capacity::install_stop();
        if case == "native" {
            incremental_deep_wide_complete_output_identity_and_resource_observations();
            return;
        }
        let root = PathBuf::from(std::env::var_os("RESOURCE_STOP_EVIDENCE").unwrap());
        if case.starts_with("before") {
            unsafe {
                libc::raise(if case.ends_with("term") {
                    libc::SIGTERM
                } else {
                    libc::SIGINT
                });
            }
        }
        resource_call(&root, "control", &root);
        fs::write(root.join("next-call"), "started").unwrap();
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    for case in [
        "success",
        "before-term",
        "before-int",
        "during-term",
        "during-int",
    ] {
        let dir = TempDir::new().unwrap();
        let persistent =
            std::env::var_os("RESOURCE_STOP_EVIDENCE").map(|p| PathBuf::from(p).join(case));
        let root = persistent.as_deref().unwrap_or(dir.path());
        if persistent.is_some() {
            fs::create_dir(root).unwrap();
        }
        let bin = root.join("target/debug/core-observe");
        fs::create_dir_all(bin.parent().unwrap()).unwrap();
        fs::write(&bin, if case == "success" {
            "#!/bin/sh\ncat >/dev/null\nsleep 0.2\nprintf '{\"status\":\"current\"}'\nprintf 'retrieval-work {}\\n' >&2\n"
        } else {
            "#!/bin/sh\nprintf ready > ready\nexec sleep 30\n"
        }).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o700)).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "resource_request_stop_controls", "--nocapture"])
            .env("RESOURCE_STOP_CONTROL", case)
            .env("RESOURCE_STOP_EVIDENCE", root)
            .env("CORE_OBSERVE_BIN", &bin)
            .stdout(fs::File::create(root.join("control.stdout")).unwrap())
            .stderr(fs::File::create(root.join("control.stderr")).unwrap())
            .spawn()
            .unwrap();
        let start = Instant::now();
        let mut signalled_at = None;
        if case.starts_with("during") {
            while !root.join("ready").exists() {
                assert!(start.elapsed() < Duration::from_secs(5));
                assert!(child.try_wait().unwrap().is_none());
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(
                unsafe {
                    libc::kill(
                        child.id() as i32,
                        if case.ends_with("term") {
                            libc::SIGTERM
                        } else {
                            libc::SIGINT
                        },
                    )
                },
                0
            );
            signalled_at = Some(Instant::now());
        }
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if signalled_at.is_some_and(|t| t.elapsed() >= Duration::from_secs(1))
                || start.elapsed() >= Duration::from_secs(8)
            {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("resource control failed to drain: {case}");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(status.success(), case == "success", "{case}: {status}");
        assert_eq!(root.join("next-call").exists(), case == "success", "{case}");
        if case.starts_with("before") {
            assert!(!root.join("control-process.start.json").exists());
            assert!(!root.join("ready").exists());
        } else {
            let transport: Value = serde_json::from_slice(
                &fs::read(root.join("control-process.process.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(transport["drained"], true);
            assert_eq!(transport["runner_timeout"], false);
            let pgid = transport["pgid"].as_i64().unwrap() as i32;
            assert_eq!(unsafe { libc::kill(-pgid, 0) }, -1);
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ESRCH)
            );
            assert_eq!(
                transport["cancelled_signal"],
                if case == "success" {
                    0
                } else if case.ends_with("term") {
                    libc::SIGTERM
                } else {
                    libc::SIGINT
                }
            );
            if case == "success" {
                let receipt: Value =
                    serde_json::from_slice(&fs::read(root.join("control.receipt.json")).unwrap())
                        .unwrap();
                assert!(receipt["samples"].as_u64().unwrap() > 0);
                assert!(receipt["native_rss_bytes"].as_u64().unwrap() > 0);
                assert!(
                    receipt["sampled_helper_inclusive_rss_bytes"]
                        .as_u64()
                        .unwrap()
                        > 0
                );
                assert!(receipt["sampled_tree_fds"].as_u64().unwrap() > 0);
            }
        }
    }
}
#[test]
fn incremental_deep_wide_complete_output_identity_and_resource_observations() {
    // A descendant can precede its parent in ps; exclude unrelated processes.
    assert_eq!(
        resource_tree(&[(3, 2, 30), (4, 1, 40), (2, 1, 20), (9, 8, 90)], 1),
        BTreeSet::from([1, 2, 3, 4])
    );
    let dir = TempDir::new().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let evidence = std::env::var_os("OUTLINE_RESOURCE_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("resources"));
    fs::create_dir(&evidence).unwrap();
    let bpe = ORACLE_BPE.clone();
    let mut capacity = vec![];
    for (shape, size) in [("deep", 48), ("wide", 180)] {
        let ident = |i| format!("72000000-0000-4000-8000-{:012}", i + 1);
        let ns: Vec<_> = (0..size).map(|i| {
            let cs = if shape == "deep" && i + 1 < size { vec![ident(i+1)] }
                else if shape == "wide" && i == 0 { (1..size).map(ident).collect() } else { vec![] };
            json!({"id":ident(i),"name":format!("{shape} {i}"),"micro_summary":format!("Complete branch {i} Ω"),
                "status":"in_progress","content":"Synthetic body.\n","see_also":[],"children":cs})
        }).collect();
        let store = root.join(shape);
        materialize(&store, &ns);
        let authored = authored_snapshot(&store);
        let (cold, _) = resource_call(&store, &format!("{shape}-cold"), &evidence);
        assert_eq!(cold["nodes"].as_array().unwrap().len(), size);
        assert_eq!(authored_snapshot(&store), authored);
        let baseline = outline_identity(&store);
        let (warm, _) = resource_call(&store, &format!("{shape}-warm"), &evidence);
        assert_eq!(warm, cold);
        assert_eq!(outline_identity(&store), baseline);
        let p = store.join("nodes").join(ident(size - 1)).join("node.md");
        fs::write(
            &p,
            fs::read_to_string(&p).unwrap().replacen(
                "---\n",
                "---\nscope: \"Only body/scope affects SQL.\"\n",
                1,
            ),
        )
        .unwrap();
        let authored = authored_snapshot(&store);
        let (body, receipt) = resource_call(&store, &format!("{shape}-body"), &evidence);
        assert_ne!(body["version"], cold["version"]);
        assert_eq!(outline_identity(&store), baseline);
        assert_eq!(receipt["work"]["outline_files_staged"], 0);
        let db = rusqlite::Connection::open(store.join(".sifttext/index.sqlite")).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT scope FROM nodes WHERE id=?",
                [ident(size - 1)],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "Only body/scope affects SQL."
        );
        drop(db);
        assert_eq!(authored_snapshot(&store), authored);
        change_field(
            &store,
            &ident(size - 1),
            "micro_summary",
            json!("Changed final displayed summary."),
        );
        let authored = authored_snapshot(&store);
        let (summary, _) = resource_call(&store, &format!("{shape}-summary"), &evidence);
        assert_ne!(summary["version"], body["version"]);
        let after = outline_identity(&store);
        let writes: Vec<_> = after
            .iter()
            .filter(|(p, v)| baseline.get(*p) != Some(*v))
            .map(|(p, _)| p)
            .collect();
        assert!(!writes.is_empty());
        assert_dependency_equality(&store, &summary, &bpe);
        assert_eq!(authored_snapshot(&store), authored);
        // check_native already compared every emitted row/byte/cost against the
        // authored oracle. Recompute only root geometry, not all token costs again.
        let by = authored_records(&store)
            .into_iter()
            .map(|n| (n["id"].as_str().unwrap().to_owned(), n))
            .collect();
        let mut rows = vec![];
        walk(&by, &ident(0), 0, &mut rows, &mut BTreeSet::new()).unwrap();
        assert_eq!(rows.len(), size);
        assert_eq!(
            rows.last().unwrap().1,
            if shape == "deep" { size - 1 } else { 1 }
        );
        let bytes = |p: &Path| fs::metadata(store.join(p)).unwrap().len();
        capacity.push(json!({"shape":shape,"size":size,"files":after.len(),"disk_bytes":after.keys().map(|p| bytes(p)).sum::<u64>(),
            "summary_files_replaced":writes.len(),"summary_bytes_replaced":writes.iter().map(|p| bytes(p)).sum::<u64>()}));
    }
    fs::write(
        evidence.join("capacity.json"),
        serde_json::to_vec_pretty(&capacity).unwrap(),
    )
    .unwrap();
    fs::write(evidence.join("limits.txt"), "Synthetic deep48/wide180 only. Native RSS is time's measurement; helper-inclusive RSS/FD sampling has a nominal20ms sleep plus ps/lsof cost, can miss short peaks, and inflates wall time. No universal250ms/250MB claim.\n").unwrap();
}
