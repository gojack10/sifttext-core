#[path = "native_resources.rs"]
mod native_resources;

fn disk_bytes(path: &Path) -> u64 {
    let meta = fs::symlink_metadata(path).unwrap();
    if meta.is_file() {
        meta.len()
    } else if meta.is_dir() {
        fs::read_dir(path)
            .unwrap()
            .map(|e| disk_bytes(&e.unwrap().path()))
            .sum()
    } else {
        0
    }
}
struct ResourceChild(Option<std::process::Child>);
impl Drop for ResourceChild {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl Native {
    fn measured_prepared(&mut self, p: &Path, label: &str) -> Value {
        let request = json!({"op":"index_prepared","store":p,"accepted_main":self.main,"prepared_worktree":self.work});
        let stdout = self.evidence.join(format!("{label}.stdout.json"));
        let stderr = self.evidence.join(format!("{label}.stderr.txt"));
        let mut command = Command::new("/usr/bin/time");
        #[cfg(target_os = "macos")]
        command.arg("-l");
        #[cfg(target_os = "linux")]
        command.arg("-v");
        let start = Instant::now();
        let child = command
            .arg(&self.bin)
            .stdin(Stdio::piped())
            .stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap();
        let mut guard = ResourceChild(Some(child));
        let pid = guard.0.as_ref().unwrap().id();
        guard
            .0
            .as_mut()
            .unwrap()
            .stdin
            .take()
            .unwrap()
            .write_all(request.to_string().as_bytes())
            .unwrap();
        let mut samples = vec![];
        let (mut rss_peak, mut fd_peak) = (0, 0);
        let status = loop {
            let (rss, fds, mut raw) = native_resources::sample(pid);
            raw["elapsed_ms"] = json!(start.elapsed().as_secs_f64() * 1000.0);
            samples.push(raw);
            rss_peak = rss_peak.max(rss);
            fd_peak = fd_peak.max(fds);
            if let Some(status) = guard.0.as_mut().unwrap().try_wait().unwrap() {
                break status;
            }
            thread::sleep(Duration::from_millis(5));
        };
        guard.0.take();
        let stderr = fs::read_to_string(stderr).unwrap();
        let result: Value = serde_json::from_slice(&fs::read(stdout).unwrap()).unwrap();
        let rss = native_resources::native_rss(&stderr);
        let receipt = json!({"label":label,"request":request,"exit":status.code(),"ms":start.elapsed().as_secs_f64()*1000.0,
            "native_rss_bytes":rss,"sampled_helper_inclusive_rss_bytes":rss_peak,"sampled_tree_fds":fd_peak,
            "samples":samples,"disk_bytes":disk_bytes(p),"result":result,"stderr":stderr});
        fs::write(
            self.evidence.join(format!("{label}.receipt.json")),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
        eprintln!("prepared resource {label}: {receipt}");
        assert!(status.success(), "{receipt}");
        assert_eq!(result["view"]["status"], "current", "{receipt}");
        assert!(
            rss > 0 && rss_peak > 0 && fd_peak > 0,
            "missing honest observation: {receipt}"
        );
        result
    }
}
#[test]
fn native_prepared_cold_warm_changed_resources() {
    use std::collections::BTreeSet;
    assert_eq!(
        native_resources::resource_tree(&[(3, 2, 30), (4, 1, 40), (2, 1, 20), (9, 8, 90)], 1),
        BTreeSet::from([1, 2, 3, 4])
    );
    let temp = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("SIFTTEXT_TEST_EVIDENCE")
        .map(|p| PathBuf::from(format!("{}-resources", PathBuf::from(p).display())))
        .unwrap_or_else(|| temp.path().join("resources"));
    fs::create_dir(&evidence).unwrap();
    let initial = evidence.join("initial");
    fixture(&initial);
    let main = evidence.join("main");
    copy(&initial, &main);
    let work = evidence.join("work");
    copy(&initial, &work);
    field(&main, 3, "name", json!("Current Y"));
    field(&work, 2, "name", json!("Prepared X"));
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .map(|p| p.join("core-observe"))
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_core-observe")));
    let mut native = Native {
        bin,
        evidence,
        serial: 0,
        main: main.clone(),
        work: work.clone(),
    };
    native.search(&main, "login");
    native.search(&work, "login");
    let donor_bytes = [fs::read(db(&main)).unwrap(), fs::read(db(&work)).unwrap()];
    let combined = native.clone_store(&main, "combined");
    field(&combined, 2, "name", json!("Prepared X"));
    let cold = native.measured_prepared(&combined, "cold");
    for (key, n) in [
        ("accepted_main", 6),
        ("prepared_worktree", 1),
        ("computed", 0),
        ("changed_rows", 1),
        ("search_bodies_reused", 7),
    ] {
        assert_eq!(cold["reuse"][key], n);
    }
    let bytes = fs::read(db(&combined)).unwrap();
    for n in 0..3 {
        let warm = native.measured_prepared(&combined, &format!("warm-{n}"));
        assert_eq!(warm["view"], cold["view"]);
        assert_eq!(warm["reuse"]["changed_rows"], 0);
        assert_eq!(fs::read(db(&combined)).unwrap(), bytes);
    }
    field(&combined, 4, "name", json!("Changed unique"));
    let changed = native.measured_prepared(&combined, "changed");
    assert_ne!(changed["view"]["version"], cold["view"]["version"]);
    assert_eq!(changed["reuse"]["computed"], 1);
    assert_eq!(changed["reuse"]["changed_rows"], 1);
    assert_eq!(
        native.search(&combined, "unique")["results"][0]["id"],
        id(4)
    );
    assert_eq!(
        [fs::read(db(&main)).unwrap(), fs::read(db(&work)).unwrap()],
        donor_bytes
    );
    native.equality(&combined);
    fs::write(native.evidence.join("limits.txt"),"Seven synthetic nodes, debug binary; not universal capacity or speed evidence. Cold/warm use fresh processes; OS page cache not dropped. Native time RSS excludes simultaneous helpers. Process-tree ps/FD samples sleep5ms plus acquisition overhead, can miss peaks and inflate latency; retain raw samples. Disk sums regular file logical lengths (including cache), not allocated blocks, ignoring symlinks. Prepared donor identities remain reverified, Bend/full-source/outline checks remain active.\n").unwrap();
}
