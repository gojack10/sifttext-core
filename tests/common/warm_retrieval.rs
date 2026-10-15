// Remaining process observations from incremental_retrieval.mjs. Product stays unchanged.
#[path = "directory_events.rs"]
mod directory_events;
#[path = "native_resources.rs"]
mod native_resources;
use std::os::unix::fs::MetadataExt;

fn identities(p: &Path) -> Value {
    let rows: BTreeMap<_, _> = artifacts(p).into_iter().map(|(path, bytes)| {
        let m = fs::metadata(p.join(&path)).unwrap();
        (path.to_str().unwrap().to_owned(), json!({"hash":format!("{:x}",Sha256::digest(&bytes)),
            "bytes":m.len(),"ino":m.ino(),"mtime":[m.mtime(),m.mtime_nsec()],"ctime":[m.ctime(),m.ctime_nsec()]}))
    }).collect();
    json!(rows)
}
fn authored_hashes(p: &Path) -> Value {
    let rows: BTreeMap<_, _> = (1..=7)
        .flat_map(|n| ["node.md", "children.order"].map(move |f| (n, f)))
        .map(|(n, f)| {
            (
                format!("{}/{f}", id(n)),
                format!(
                    "{:x}",
                    Sha256::digest(fs::read(node(p, n).join(f)).unwrap())
                ),
            )
        })
        .collect();
    json!(rows)
}
impl Native {
    fn observed(&mut self, request: Value, label: &str) -> (Value, Value) {
        let start = Instant::now();
        let stdout = self
            .evidence
            .join(format!("{}-resource.stdout", self.serial));
        let stderr = self
            .evidence
            .join(format!("{}-resource.stderr", self.serial));
        let mut command = Command::new("/usr/bin/time");
        #[cfg(target_os = "macos")]
        command.arg("-l");
        #[cfg(target_os = "linux")]
        command.arg("-v");
        if label.starts_with("baseline-") {
            command.current_dir(
                std::env::var_os("INCREMENTAL_BASELINE_CWD").expect("baseline cwd required"),
            );
        }
        let child = command
            .arg(&self.bin)
            .env("SIFTTEXT_RETRIEVAL_TRACE", "1")
            .stdin(Stdio::piped())
            .stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap();
        let mut guard = Process(Some(child));
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
        let (mut peak, mut fds, mut samples) = (0, 0, vec![]);
        let status = loop {
            let (rss, fd, sample) = native_resources::sample(pid);
            peak = peak.max(rss);
            fds = fds.max(fd);
            samples.push(sample);
            if let Some(status) = guard.0.as_mut().unwrap().try_wait().unwrap() {
                break status;
            }
            thread::sleep(Duration::from_millis(5));
        };
        guard.0.take();
        let err = fs::read_to_string(stderr).unwrap();
        let result = serde_json::from_slice::<Value>(&fs::read(stdout).unwrap());
        let traces: BTreeMap<_, Value> = err
            .lines()
            .filter(|s| s.starts_with("retrieval-"))
            .map(|s| {
                let (k, v) = s.split_once(' ').unwrap();
                (k, serde_json::from_str(v).unwrap())
            })
            .collect();
        let native_rss = native_resources::native_rss(&err);
        let receipt = json!({"label":label,"request":request,"exit":status.code(),"signal":status.signal(),
            "result":result.as_ref().ok(),"json_error":result.as_ref().err().map(ToString::to_string),
            "ms":start.elapsed().as_secs_f64()*1000.,"native_rss_bytes":native_rss,
            "sampled_helper_inclusive_rss_bytes":peak,"sampled_tree_fds":fds,"samples":samples,
            "work":traces,"stderr":err});
        fs::write(
            self.evidence.join(format!("{}-resource.json", self.serial)),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
        self.serial += 1;
        assert!(status.success(), "{receipt:#}");
        let result = result.unwrap_or_else(|_| panic!("invalid resource response: {receipt:#}"));
        assert_eq!(result["status"], "current", "{receipt:#}");
        assert!(
            native_rss > 0 && peak > 0 && fds > 0,
            "missing resource observation: {receipt:#}"
        );
        (result, receipt)
    }
    fn warm_benchmark(&mut self, label: &str, candidate: bool) -> Value {
        let p = self.evidence.join(label);
        fixture(&p);
        let source = authored_hashes(&p);
        let (cold, cold_receipt) = self.observed(Self::request(&p), &format!("{label}-cold"));
        assert_eq!(authored_hashes(&p), source);
        let initial = identities(&p);
        let cache = p.join(".sifttext");
        let stage = cache.join("outline-stage");
        // Delivery control preserves the transient-stage check, not just final absence.
        let control = directory_events::Watch::new(&cache);
        fs::create_dir(&stage).unwrap();
        fs::remove_dir(&stage).unwrap();
        assert!(control.finish().contains(&stage));
        let watch = directory_events::Watch::new(&cache);
        let mut warm = vec![];
        for _ in 0..8 {
            let (output, receipt) = self.observed(Self::request(&p), &format!("{label}-warm"));
            assert_eq!(output, cold);
            if candidate {
                assert_eq!(receipt["work"]["retrieval-postings"]["updated"], 0);
                assert_eq!(receipt["work"]["retrieval-postings"]["removed"], 0);
                assert_eq!(
                    receipt["work"]["retrieval-work"]["node_link_rows_changed"],
                    0
                );
                assert_eq!(receipt["work"]["retrieval-work"]["outlines_reused"], true);
            }
            warm.push(receipt);
        }
        let events = watch.finish();
        let reused = identities(&p);
        if candidate {
            assert_eq!(reused, initial);
            assert!(!events.contains(&stage));
        }
        assert!(!stage.exists());
        assert_eq!(authored_hashes(&p), source);
        field(&p, 4, "name", json!("Changed unique login"));
        let changed_source = authored_hashes(&p);
        let (changed, changed_receipt) =
            self.observed(Self::request(&p), &format!("{label}-changed"));
        assert_ne!(cold["version"], changed["version"]);
        if candidate {
            assert_eq!(changed_receipt["work"]["retrieval-postings"]["updated"], 1);
        }
        assert_eq!(authored_hashes(&p), changed_source);
        let after = identities(&p);
        let writes = |a: &Value, b: &Value| {
            b.as_object()
                .unwrap()
                .iter()
                .filter(|(k, v)| a.get(k.as_str()) != Some(*v))
                .map(|(k, v)| json!({"path":k,"bytes":v["bytes"]}))
                .collect::<Vec<_>>()
        };
        let report = json!({"cold":cold,"changed":changed,"cold_files":initial,"warm_files":reused,"changed_files":after,
            "source_before":source,"source_after":changed_source,"warm_writes":writes(&initial,&reused),
            "changed_writes":writes(&reused,&after),"disk_bytes":after.as_object().unwrap().values().map(|v|v["bytes"].as_u64().unwrap()).sum::<u64>(),
            "stage_events":events,"cold_resource":cold_receipt,"warm_resources":warm,"changed_resource":changed_receipt});
        fs::write(
            self.evidence.join(format!("{label}-benchmark.json")),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        report
    }
}
#[test]
fn resource_response_failures_retain_exit_stderr_and_parse_evidence() {
    use std::os::unix::fs::PermissionsExt;
    for (output, exit) in [("", 7), ("not-json", 0)] {
        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("helper");
        fs::write(&bin, format!("#!/bin/sh\ncat >/dev/null\nprintf '%s' '{output}'\nprintf 'helper diagnostic\\n' >&2\nexit {exit}\n")).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o700)).unwrap();
        let mut native = Native {
            bin,
            evidence: temp.path().to_owned(),
            serial: 0,
        };
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                native.observed(json!({}), "failed-response")
            }))
            .is_err()
        );
        let receipt: Value =
            serde_json::from_slice(&fs::read(temp.path().join("0-resource.json")).unwrap())
                .unwrap();
        assert_eq!(receipt["exit"], exit);
        assert_eq!(receipt["signal"], Value::Null);
        assert_eq!(receipt["result"], Value::Null);
        assert!(
            receipt["json_error"]
                .as_str()
                .unwrap()
                .contains(if output.is_empty() { "EOF" } else { "expected" })
        );
        assert!(
            receipt["stderr"]
                .as_str()
                .unwrap()
                .contains("helper diagnostic")
        );
        assert_eq!(
            fs::read(temp.path().join("0-resource.stdout")).unwrap(),
            output.as_bytes()
        );
    }
}
#[test]
fn native_warm_identity_settings_and_resource_observations() {
    let temp = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("WARM_RETRIEVAL_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("warm-evidence"));
    assert!(evidence.is_absolute());
    fs::create_dir(&evidence).unwrap();
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .map(|p| p.join("core-observe"))
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_core-observe")));
    let mut native = Native {
        bin,
        evidence: evidence.clone(),
        serial: 0,
    };
    assert_eq!(
        native_resources::resource_tree(&[(3, 2, 30), (4, 1, 40), (2, 1, 20), (9, 8, 90)], 1),
        std::collections::BTreeSet::from([1, 2, 3, 4])
    );
    let candidate = native.warm_benchmark("candidate", true);
    if let Some(bin) = std::env::var_os("INCREMENTAL_BASELINE_BIN") {
        let cwd = std::env::var_os("INCREMENTAL_BASELINE_CWD").expect("baseline cwd required");
        assert!(Path::new(&bin).is_absolute() && Path::new(&cwd).is_absolute());
        native.bin = bin.into();
        let baseline = native.warm_benchmark("baseline", false);
        assert_eq!(baseline["cold"], candidate["cold"]);
        assert_eq!(baseline["changed"], candidate["changed"]);
        assert!(
            baseline["warm_writes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["path"] == ".sifttext/index.sqlite")
        );
        native.bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
            .map(PathBuf::from)
            .map(|p| p.join("core-observe"))
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_core-observe")));
    }
    let store = evidence.join("candidate");
    for query in [
        "login reset",
        "\"login reset\"",
        "login -reset",
        "resetting",
        "the and",
        "",
        "-login",
    ] {
        for scope in [None, Some(1), Some(2), Some(4), Some(5)] {
            let before = identities(&store);
            let mut request = Native::request(&store);
            request["query"] = json!(query);
            request["under_node_id"] = json!(scope.map(id));
            request["offset"] = json!(1);
            request["ranking"] = json!({"title_weight":3,"boosts":{id(3):2.5}});
            let initial = native.call(request, "current");
            let allowed: Vec<_> = match scope {
                Some(4) => vec![id(4)],
                Some(2) => vec![id(2), id(4)],
                Some(1) => (1..=4).map(id).collect(),
                Some(5) => (5..=7).map(id).collect(),
                None => (1..=7).map(id).collect(),
                _ => unreachable!(),
            };
            for hit in initial["results"].as_array().unwrap() {
                assert!(allowed.contains(&hit["id"].as_str().unwrap().to_owned()));
            }
            assert_eq!(initial["other_groups"], json!([]));
            assert_eq!(identities(&store), before);
            let mut restricted = Native::request(&store);
            restricted["query"] = json!(query);
            restricted["readable_ids"] = json!([id(3)]);
            restricted["limit"] = json!(1);
            let read = native.call(restricted, "current");
            for hit in read["results"].as_array().unwrap() {
                assert_eq!(hit["id"], id(3));
            }
            assert_eq!(read["other_groups"], json!([]));
            assert_eq!(identities(&store), before);
        }
    }
    fs::write(evidence.join("warm-passed.json"),serde_json::to_vec_pretty(&json!({"requests":native.serial,"matrix": "7 queries x 5 scopes x unrestricted/restricted",
        "limits":"7 synthetic nodes only; nominal5ms sampling plus ps/lsof overhead can miss short helper peaks and adds wall cost. No latency/capacity threshold or speed claim. Optional baseline runs only when explicit binary/cwd are supplied; exact-score tie order stays undefined."})).unwrap()).unwrap();
}
