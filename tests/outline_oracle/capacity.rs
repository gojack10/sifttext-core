// Native capacity consumers retain all seven synthetic legacy shapes.
// Legacy retirement still requires their source-bound resource/refusal evidence.
mod capacity {
    use super::*;
    fn ident(i: usize) -> String {
        format!("82000000-0000-4000-8000-{i:012}")
    }
    fn specimen(n: usize, shape: &str) -> Vec<Value> {
        let mut nodes: Vec<_> = (0..n).map(|i| json!({"id":ident(i),"name":format!("Capacity item {i}"),
            "micro_summary":format!("Summary {i} Ω"),"scope":format!("Scope {i}"),"content":format!("Authored body {i}.\n"),
            "status":"in_progress","children":[],"see_also":[]})).collect();
        if shape != "roots" {
            for i in (1..n).rev() {
                let parent = match shape {
                    "chain" => i - 1,
                    "balanced" => (i - 1) / 8,
                    _ => 0,
                };
                nodes[parent]["children"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(ident(i)));
            }
        }
        if shape == "links" {
            nodes[0]["see_also"] = json!(
                (1..n)
                    .rev()
                    .map(|i| json!({"target":ident(i),
            "reason":format!("Reference to {} Ω\nKeep order.",ident(i))}))
                    .collect::<Vec<_>>()
            );
        }
        for (i, node) in nodes.iter_mut().enumerate() {
            if children(node).is_empty() {
                let status = ["not_examined", "resolved", "stuck", "deferred", "discarded"][i % 5];
                node["status"] = json!(status);
                if let Some(key) = active(status) {
                    node[key] = json!(format!("Reason {i} Ω\nKeep text."));
                }
                if i % 7 == 0 {
                    node["warnings"] = json!("Warning Ω\tquoted \"text\"\nnext line");
                }
            }
        }
        nodes
    }
    // Independent length-delimited authored inventory, including directories and
    // symlink targets. PathBuf ordering compares path components, not flattened bytes.
    pub(super) fn inventory(store: &Path) -> BTreeMap<PathBuf, (u8, Vec<u8>)> {
        fn visit(store: &Path, p: &Path, out: &mut BTreeMap<PathBuf, (u8, Vec<u8>)>) {
            let meta = fs::symlink_metadata(p).unwrap();
            let (kind, bytes) = if meta.is_symlink() {
                (
                    2,
                    fs::read_link(p)
                        .unwrap()
                        .as_os_str()
                        .as_encoded_bytes()
                        .to_vec(),
                )
            } else if meta.is_dir() {
                (0, vec![])
            } else {
                (1, fs::read(p).unwrap())
            };
            out.insert(p.strip_prefix(store).unwrap().to_owned(), (kind, bytes));
            if kind == 0 {
                for entry in fs::read_dir(p).unwrap() {
                    visit(store, &entry.unwrap().path(), out);
                }
            }
        }
        let mut out = BTreeMap::new();
        visit(store, &store.join("trees"), &mut out);
        for entry in fs::read_dir(store.join("nodes")).unwrap() {
            let p = entry.unwrap().path();
            out.insert(p.strip_prefix(store).unwrap().to_owned(), (0, vec![]));
            for name in ["node.md", "children.order", "children"] {
                visit(store, &p.join(name), &mut out);
            }
        }
        out
    }
    pub(super) fn identities(inv: &BTreeMap<PathBuf, (u8, Vec<u8>)>) -> (String, String) {
        let mut global = Sha256::new();
        let mut neighborhood = Sha256::new();
        neighborhood.update(b"sifttext-neighborhood-v1\0");
        for (p, (kind, bytes)) in inv {
            let path = p.as_os_str().as_encoded_bytes();
            global.update((path.len() as u64).to_le_bytes());
            global.update(path);
            global.update([*kind]);
            if *kind != 0 {
                global.update((bytes.len() as u64).to_le_bytes());
                global.update(bytes);
            }
            if p.starts_with("trees") || p.components().count() >= 3 {
                neighborhood.update((path.len() as u64).to_le_bytes());
                neighborhood.update(path);
                neighborhood.update([match kind {
                    0 => 2,
                    1 => 0,
                    2 => 1,
                    _ => unreachable!(),
                }]);
                neighborhood.update((bytes.len() as u64).to_le_bytes());
                neighborhood.update(bytes);
            }
        }
        (
            format!("{:x}", global.finalize()),
            format!("{:x}", neighborhood.finalize()),
        )
    }
    fn derived(store: &Path) -> BTreeMap<PathBuf, String> {
        let mut out = generated_snapshot(store);
        out.insert(
            PathBuf::from(".sifttext/index.sqlite"),
            sha(&fs::read(store.join(".sifttext/index.sqlite")).unwrap()),
        );
        out
    }
    fn check_index(v: &Value, nodes: &[Value], version: &str) -> Result<(), &'static str> {
        if v["status"] != "current" || v["version"] != version {
            return Err("freshness");
        }
        if v["nodes"] != json!(nodes) {
            return Err("complete authored records");
        }
        let links: Vec<_> =
            nodes
                .iter()
                .flat_map(|n| {
                    n["see_also"].as_array().unwrap().iter().map(
                        |r| json!({"source":n["id"],"target":r["target"],"reason":r["reason"]}),
                    )
                })
                .collect();
        if v["links"] != json!(links) {
            return Err("complete ordered references");
        }
        Ok(())
    }
    fn sql(store: &Path, nodes: &[Value], version: &str) {
        let db = rusqlite::Connection::open_with_flags(
            store.join(".sifttext/index.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            db.query_row("select value from metadata where key='version'", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            version
        );
        let mut stmt = db.prepare("select record from nodes order by id").unwrap();
        let actual: Vec<Value> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| serde_json::from_str(&r.unwrap()).unwrap())
            .collect();
        assert_eq!(actual, nodes);
        let mut stmt = db
            .prepare("select source,target,reason from links order by source,target")
            .unwrap();
        let actual: Vec<_> = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let mut expected: Vec<_> = nodes
            .iter()
            .flat_map(|n| {
                n["see_also"].as_array().unwrap().iter().map(|r| {
                    (
                        n["id"].as_str().unwrap().to_owned(),
                        r["target"].as_str().unwrap().to_owned(),
                        r["reason"].as_str().unwrap().to_owned(),
                    )
                })
            })
            .collect();
        expected.sort();
        assert_eq!(actual, expected);
    }
    fn current_output(out: &Output, version: &str) -> Result<Value, &'static str> {
        if !out.status.success() {
            return Err("concealed nonzero or signaled completion");
        }
        let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| "malformed completion")?;
        if v["status"] != "current" || v["version"] != version {
            return Err("completion freshness");
        }
        Ok(v)
    }
    fn unavailable_output(out: &Output, op: &str) -> Result<Value, &'static str> {
        if out.status.code() != Some(1) {
            return Err("not an explicit refusal");
        }
        let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| "malformed refusal")?;
        if v["status"] != "error" || v.get("version") != Some(&Value::Null) {
            return Err("refusal freshness");
        }
        let empty = if op == "index" {
            v["nodes"] == json!([]) && v["links"] == json!([])
        } else {
            v["total_matches"] == 0 && v["results"] == json!([]) && v["other_groups"] == json!([])
        };
        if !empty {
            return Err("partial refused knowledge");
        }
        Ok(v)
    }
    // A later validate call cannot explain why this invocation refused. The
    // diagnostic must travel with the exact nonpartial index/search output.
    fn resource_reason(out: &Output) -> Option<String> {
        if out.status.code() != Some(1) {
            return None;
        }
        String::from_utf8_lossy(&out.stderr)
            .lines()
            .find_map(|line| {
                let diagnostic: Value = serde_json::from_str(line).ok()?;
                (diagnostic["status"] == "error"
                    && matches!(
                        diagnostic["kind"].as_str(),
                        Some("timeout" | "resource_exhausted")
                    ))
                .then(|| line.to_owned())
            })
    }
    include!("capacity_process.rs");
    struct Runner {
        evidence: PathBuf,
        serial: usize,
        fd_limit: usize,
    }
    impl Runner {
        fn check_native(
            &mut self,
            label: &str,
            store: &Path,
            version: &str,
            bpe: &tiktoken_rs::CoreBPE,
        ) {
            check_native_using(store, version, bpe, || {
                self.current(label, json!({"op":"outline_inspect","store":store}), version)
            });
        }
        fn run(
            &mut self,
            label: &str,
            mut request: Value,
            cli_args: Option<Vec<String>>,
        ) -> (Output, Value) {
            if cli_args.is_none() && matches!(request["op"].as_str(), Some("index" | "search")) {
                request["diagnostics"] = json!(true);
            }
            let prefix = self.evidence.join(format!("{:03}-{label}", self.serial));
            self.serial += 1;
            let request_path = prefix.with_extension("request.json");
            fs::write(&request_path, request.to_string()).unwrap();
            let name = if cli_args.is_some() {
                "sifttext-file-graph"
            } else {
                "core-observe"
            };
            let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug"))
                .join(name);
            let mut command = Command::new("/bin/sh");
            #[cfg(target_os = "macos")]
            let flag = "-l";
            #[cfg(target_os = "linux")]
            let flag = "-v";
            let script = format!("ulimit -n {}; exec /usr/bin/time \"$@\"", self.fd_limit);
            command
                .args(["-c", &script, "capacity", flag])
                .arg(bin)
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .env("SIFTTEXT_RETRIEVAL_TRACE", "1");
            let input = if cli_args.is_some() { vec![] } else { request.to_string().into_bytes() };
            if let Some(args) = cli_args {
                for arg in args {
                    command.arg(if arg == "@request" {
                        request_path.to_string_lossy().into_owned()
                    } else {
                        arg
                    });
                }
            }
            let (out, process) = bounded_request(command, input, &prefix, Duration::from_secs(900), None);
            let status = out.status;
            let store = Path::new(request["store"].as_str().unwrap());
            let disk = fs::read_dir(store.join("nodes"))
                .unwrap()
                .flat_map(|e| fs::read_dir(e.unwrap().path()).unwrap())
                .map(|e| e.unwrap().path())
                .filter(|p| fs::symlink_metadata(p).unwrap().is_file())
                .map(|p| fs::metadata(p).unwrap().len())
                .sum::<u64>()
                + fs::metadata(store.join(".sifttext/index.sqlite"))
                    .map(|m| m.len())
                    .unwrap_or(0);
            let receipt = json!({"label":label,"request":request,"exit":status.code(),"signal":status.signal(),
                "process":process,"ms":process["ms"],"native_rss_bytes":native_resources::native_rss(&String::from_utf8_lossy(&out.stderr)),
                "sampled_helper_inclusive_rss_bytes":process["sampled_helper_inclusive_rss_bytes"],"sampled_numeric_fd_peak":process["sampled_numeric_fd_peak"],"samples":process["samples"],
                "fd_limit":self.fd_limit,"output_bytes":out.stdout.len(),"regular_file_disk_bytes":disk});
            fs::write(
                prefix.with_extension("receipt.json"),
                serde_json::to_vec_pretty(&receipt).unwrap(),
            )
            .unwrap();
            let v = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
            (out, v)
        }
        fn killed(&mut self, label: &str, store: &Path) -> Output {
            let barrier = TempDir::new().unwrap();
            let ready = barrier.path().join("ready");
            let request = json!({"op":"index","store":store,"mode":"incremental",
                "pause_before_accept":{"ready":ready,"release":barrier.path().join("release")}});
            let prefix = self.evidence.join(format!("{:03}-{label}", self.serial));
            self.serial += 1;
            fs::write(prefix.with_extension("request.json"), request.to_string()).unwrap();
            let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR").map(PathBuf::from)
                .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug"))
                .join("core-observe");
            let command = Command::new(bin);
            let (out, receipt) = bounded_request(command, request.to_string().into_bytes(), &prefix, Duration::from_secs(900), Some(ready));
            assert_eq!(receipt["barrier_reached"], true, "{label}: no pre-accept readiness");
            assert_eq!(out.status.signal(), Some(libc::SIGKILL));
            assert!(out.stdout.is_empty(), "{label}: killed computation claimed a result");
            out
        }
        fn current(&mut self, label: &str, request: Value, version: &str) -> Value {
            let (out, _) = self.run(label, request, None);
            current_output(&out, version).unwrap_or_else(|e| {
                panic!("{label}: {e}; {}", String::from_utf8_lossy(&out.stderr))
            })
        }
    }
    fn neighborhood(nodes: &[Value], owner: &str, store: &Path, readable: &[String]) -> Value {
        let brief = |id: &str| {
            let n = nodes.iter().find(|n| n["id"] == id).unwrap();
            json!({"id":id,"name":n["name"],"micro_summary":n["micro_summary"],"path":store.join("nodes").join(id).join("node.md")})
        };
        let allowed = |id: &str| readable.iter().any(|i| i == id);
        let n = nodes.iter().find(|n| n["id"] == owner).unwrap();
        let reference = |id: &str, reason: &Value| {
            let mut v = brief(id);
            v["reason"] = reason.clone();
            v
        };
        json!({"parent":nodes.iter().find(|n|children(n).contains(&owner.to_owned())&&allowed(n["id"].as_str().unwrap())).map(|n|brief(n["id"].as_str().unwrap())),
            "children":children(n).iter().filter(|c|allowed(c)).map(|c|brief(c)).collect::<Vec<_>>(),
            "outgoing":n["see_also"].as_array().unwrap().iter().filter(|r|allowed(r["target"].as_str().unwrap())).map(|r|reference(r["target"].as_str().unwrap(),&r["reason"])).collect::<Vec<_>>(),
            "incoming":nodes.iter().filter(|n|allowed(n["id"].as_str().unwrap())).flat_map(|n|n["see_also"].as_array().unwrap().iter().filter(|r|r["target"]==owner).map(|r|reference(n["id"].as_str().unwrap(),&r["reason"]))).collect::<Vec<_>>()})
    }
    fn controls() {
        let nodes = specimen(4, "links");
        let links: Vec<_> = nodes[0]["see_also"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| json!({"source":ident(0),"target":r["target"],"reason":r["reason"]}))
            .collect();
        let valid = json!({"status":"current","version":"test","nodes":nodes,"links":links});
        assert!(check_index(&valid, &nodes, "test").is_ok());
        for case in 0..10 {
            let mut v = valid.clone();
            match case {
                0 => {
                    v["nodes"].as_array_mut().unwrap().pop();
                }
                1 => {
                    v["nodes"].as_array_mut().unwrap().push(nodes[0].clone());
                }
                2 => {
                    v["nodes"][0]["children"].as_array_mut().unwrap().pop();
                }
                3 => v["nodes"][0]["children"].as_array_mut().unwrap().reverse(),
                4 => v["version"] = json!("old"),
                5 => v["nodes"][0]["content"] = json!("lost"),
                6 => {
                    v["nodes"][1]
                        .as_object_mut()
                        .unwrap()
                        .remove("micro_summary");
                }
                7 => {
                    v["links"].as_array_mut().unwrap().pop();
                }
                8 => v["nodes"] = Value::Null,
                _ => v["status"] = json!("stale"),
            }
            assert!(check_index(&v, &nodes, "test").is_err(), "control{case}");
        }
    }
    #[test]
    fn process_output_self_controls() {
        use std::process::ExitStatus;
        let output = |code, text: &str| Output {
            status: ExitStatus::from_raw(code),
            stdout: text.as_bytes().to_vec(),
            stderr: vec![],
        };
        controls();
        let current = r#"{"status":"current","version":"test"}"#;
        assert!(current_output(&output(0, current), "test").is_ok());
        for out in [
            output(1 << 8, current),
            output(9, current),
            output(0, "not JSON"),
            output(0, "{}"),
            output(0, r#"{"status":"current","version":"old"}"#),
        ] {
            assert!(current_output(&out, "test").is_err());
        }
        for kind in ["timeout", "resource_exhausted"] {
            let mut refused = output(1 << 8, "{}");
            refused.stderr = format!("{{\"status\":\"error\",\"kind\":\"{kind}\",\"stage\":\"source_validation\"}}\nresource timing\n").into_bytes();
            assert!(resource_reason(&refused).is_some());
            for status in [0, 9] {
                let mut wrong = refused.clone();
                wrong.status = ExitStatus::from_raw(status);
                assert!(resource_reason(&wrong).is_none());
            }
        }
        for diagnostic in [
            "",
            "not JSON",
            "rejected: policy process timed out",
            r#"{"status":"error","kind":"unavailable"}"#,
            r#"{"status":"error","kind":"helper_nonzero"}"#,
            r#"{"status":"error","kind":"invalid_capture"}"#,
            r#"{"status":"stale","kind":"timeout"}"#,
            r#"{"status":"current","kind":"resource_exhausted"}"#,
        ] {
            let mut refused = output(1 << 8, "{}");
            refused.stderr = diagnostic.as_bytes().to_vec();
            assert!(resource_reason(&refused).is_none(), "{diagnostic}");
        }
        // spawn() errors already abort in Runner before any Output can be credited.
        for (op, refusal, partial) in [
            (
                "index",
                json!({"status":"error","version":null,"nodes":[],"links":[]}),
                "nodes",
            ),
            (
                "search",
                json!({"status":"error","version":null,"total_matches":0,"results":[],"other_groups":[]}),
                "results",
            ),
        ] {
            assert!(unavailable_output(&output(1 << 8, &refusal.to_string()), op).is_ok());
            for out in [
                output(0, &refusal.to_string()),
                output(9, &refusal.to_string()),
                output(1 << 8, "not JSON"),
            ] {
                assert!(unavailable_output(&out, op).is_err());
            }
            for case in 0..4 {
                let mut v = refusal.clone();
                match case {
                    0 => v["version"] = json!("old"),
                    1 => v[partial] = json!([{"id":ident(0)}]),
                    2 => {
                        v.as_object_mut().unwrap().remove("version");
                    }
                    _ => v["status"] = json!("current"),
                }
                assert!(
                    unavailable_output(&output(1 << 8, &v.to_string()), op).is_err(),
                    "{op} control{case}"
                );
            }
        }
    }
    fn gate_evidence(root: Option<PathBuf>, fallback: &Path, label: &str) -> PathBuf {
        if let Some(root) = root {
            let parent = root.join("capacity");
            fs::create_dir_all(&parent).unwrap();
            parent.join(label)
        } else {
            fallback.join("evidence")
        }
    }
    #[test]
    fn gate_evidence_survives_scratch_and_separates_capacity_tests() {
        let gate = TempDir::new().unwrap();
        let scratch = TempDir::new().unwrap();
        let mut paths = BTreeSet::new();
        for label in [
            "low-fd",
            "native257",
            "deep100",
            "native2048",
            "native10000",
            "deep1000",
        ] {
            let path = gate_evidence(Some(gate.path().to_owned()), scratch.path(), label);
            assert!(paths.insert(path.clone()));
            fs::create_dir(&path).unwrap();
            fs::write(path.join("receipt.json"), b"retained").unwrap();
            assert_eq!(
                fs::create_dir(&path).unwrap_err().kind(),
                std::io::ErrorKind::AlreadyExists
            );
        }
        assert_eq!(
            gate_evidence(None, scratch.path(), "native257"),
            scratch.path().join("evidence")
        );
        drop(scratch);
        for path in paths {
            assert_eq!(fs::read(path.join("receipt.json")).unwrap(), b"retained");
        }
    }
    #[test]
    fn low_descriptor_full_and_incremental_publication() {
        let dir = TempDir::new().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let store = root.join("native32");
        let nodes = specimen(32, "wide");
        materialize(&store, &nodes);
        let source = inventory(&store);
        let version = identities(&source).0;
        let evidence = std::env::var_os("CAPACITY_EVIDENCE")
            .map(|p| PathBuf::from(p).with_extension("low-fd"))
            .unwrap_or_else(|| {
                gate_evidence(
                    std::env::var_os("SIFTTEXT_EVIDENCE_DIR").map(PathBuf::from),
                    &root,
                    "low-fd",
                )
            });
        fs::create_dir(&evidence).unwrap();
        let mut runner = Runner {
            evidence,
            serial: 0,
            fd_limit: 64,
        };
        let bpe = ORACLE_BPE.clone();
        let mut baseline = None;
        for mode in ["full", "incremental"] {
            let v = runner.current(
                &format!("native32-{mode}"),
                json!({"op":"index","store":store,"mode":mode}),
                &version,
            );
            check_index(&v, &nodes, &version).unwrap();
            sql(&store, &nodes, &version);
            runner.check_native(&format!("native32-{mode}-inspect"), &store, &version, &bpe);
            let generated = generated_snapshot(&store);
            assert_eq!(generated.len(), 128);
            if let Some(ref before) = baseline {
                assert_eq!(&generated, before);
            }
            baseline = Some(generated);
            assert!(!store.join(".sifttext/outline-stage").exists());
            assert_eq!(inventory(&store), source);
        }
    }
    #[test]
    fn bounded_native_capacity_complete_queries_invalid_and_killed_recovery() {
        complete_shapes(&[
            ("native257", 257, "wide"),
            ("roots257", 257, "roots"),
            ("links258", 258, "links"),
        ]);
    }
    #[test]
    fn deep100_complete_queries_invalid_and_killed_recovery() {
        let nodes = specimen(100, "chain");
        for (i, node) in nodes.iter().enumerate() {
            assert_eq!(
                children(node),
                if i == 99 { vec![] } else { vec![ident(i + 1)] }
            );
        }
        complete_shapes(&[("deep100", 100, "chain")]);
    }
    #[test]
    fn balanced2048_complete_or_exact_resource_refusal_and_restoration() {
        let nodes = specimen(2048, "balanced");
        for (i, node) in nodes.iter().enumerate() {
            assert_eq!(
                children(node),
                ((i * 8 + 1)..(i * 8 + 9).min(2048))
                    .rev()
                    .map(ident)
                    .collect::<Vec<_>>()
            );
        }
        complete_shapes(&[("native2048", 2048, "balanced")]);
    }
    #[test]
    fn wide10000_complete_or_exact_resource_refusal_and_restoration() {
        complete_shapes(&[("native10000", 10000, "wide")]);
    }
    #[test]
    fn deep1000_complete_or_exact_resource_refusal_and_restoration() {
        complete_shapes(&[("deep1000", 1000, "chain")]);
    }
    fn complete_shapes(shapes: &[(&str, usize, &str)]) {
        controls();
        let dir = TempDir::new().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let evidence = std::env::var_os("CAPACITY_EVIDENCE")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                gate_evidence(
                    std::env::var_os("SIFTTEXT_EVIDENCE_DIR").map(PathBuf::from),
                    &root,
                    shapes[0].0,
                )
            });
        fs::create_dir(&evidence).unwrap();
        let mut runner = Runner {
            evidence: evidence.clone(),
            serial: 0,
            fd_limit: 256,
        };
        let bpe = ORACLE_BPE.clone();
        for &(label, size, shape) in shapes {
            let store = root.join(label);
            let backup = root.join(format!("{label}-seed"));
            let seed = specimen(7, "wide");
            let large = size >= 1000;
            let mut seed_version = String::new();
            if large {
                materialize(&store, &seed);
                seed_version = identities(&inventory(&store)).0;
                let v = runner.current(
                    &format!("{label}-seed"),
                    json!({"op":"index","store":store,"mode":"full"}),
                    &seed_version,
                );
                check_index(&v, &seed, &seed_version).unwrap();
                runner.check_native(&format!("{label}-seed-inspect"), &store, &seed_version, &bpe);
                copy_incremental_store(&store, &backup);
                fs::remove_dir_all(store.join("nodes")).unwrap();
                fs::remove_dir_all(store.join("trees")).unwrap();
            }
            let nodes = specimen(size, shape);
            materialize(&store, &nodes);
            if large {
                for n in &seed {
                    for name in [
                        "outline.lean",
                        "outline.full.lean",
                        "outline.lean.meta.json",
                        "outline.full.lean.meta.json",
                    ] {
                        let relative = PathBuf::from("nodes")
                            .join(n["id"].as_str().unwrap())
                            .join(name);
                        fs::copy(backup.join(&relative), store.join(relative)).unwrap();
                    }
                }
            }
            assert_eq!(authored_records(&store), nodes);
            let source = inventory(&store);
            let (version, neighbor_version) = identities(&source);
            fs::write(evidence.join(format!("{label}.source.json")),serde_json::to_vec_pretty(&json!({"nodes":nodes,"version":version,"neighborhood_version":neighbor_version})).unwrap()).unwrap();
            let req = |op: &str| json!({"op":op,"store":store,"mode":"incremental"});
            let (candidate, _) = runner.run(
                &format!("{label}-candidate"),
                json!({"store":store,"expected_version":version}),
                Some(vec!["validate-candidate".into(), "@request".into()]),
            );
            if candidate.status.success() {
                let text = String::from_utf8_lossy(&candidate.stdout);
                assert!(text.lines().any(|l| l == "status:current"));
                assert!(text.lines().any(|l| l == format!("version:{version}")));
                assert_eq!(
                    text.lines()
                        .filter(|l| l.starts_with("  content_tokens\t"))
                        .count(),
                    size
                );
            }
            let accepted = large.then(|| derived(&store));
            let (cold_out, cold) = runner.run(
                &format!("{label}-cold"),
                json!({"op":"index","store":store,"mode":"full"}),
                None,
            );
            if !cold_out.status.success() {
                assert!(large, "bounded shape refused");
                unavailable_output(&cold_out, "index").unwrap();
                assert_eq!(inventory(&store), source);
                assert_eq!(derived(&store), *accepted.as_ref().unwrap());
                let reason = resource_reason(&cold_out).unwrap_or_else(|| {
                    panic!(
                        "{label}: unknown same-call refusal: {}",
                        String::from_utf8_lossy(&cold_out.stderr)
                    )
                });
                let all: Vec<_> = nodes
                    .iter()
                    .map(|n| n["id"].as_str().unwrap().to_owned())
                    .collect();
                let (out, _) = runner.run(&format!("{label}-refused-search"),
                    json!({"op":"search","store":store,"query":"capacity","limit":20,"offset":0,"readable_ids":all}), None);
                unavailable_output(&out, "search").unwrap();
                let (out, _) = runner.run(
                    &format!("{label}-refused-neighborhood"),
                    json!({"op":"neighborhood","store":store,"id":ident(0),"readable_ids":all}),
                    None,
                );
                assert_eq!(out.status.code(), Some(1));
                assert!(out.stdout.is_empty());
                assert_eq!(inventory(&store), source);
                assert_eq!(derived(&store), *accepted.as_ref().unwrap());
                fs::remove_dir_all(store.join("nodes")).unwrap();
                fs::remove_dir_all(store.join("trees")).unwrap();
                copy_incremental_store(&backup.join("nodes"), &store.join("nodes"));
                copy_incremental_store(&backup.join("trees"), &store.join("trees"));
                assert_eq!(identities(&inventory(&store)).0, seed_version);
                let v = runner.current(
                    &format!("{label}-refusal-restore"),
                    req("index"),
                    &seed_version,
                );
                check_index(&v, &seed, &seed_version).unwrap();
                sql(&store, &seed, &seed_version);
                runner.check_native(
                    &format!("{label}-refusal-restore-inspect"),
                    &store,
                    &seed_version,
                    &bpe,
                );
                assert_eq!(derived(&store), derived(&backup));
                assert!(!store.join(".sifttext/outline-stage").exists());
                fs::write(evidence.join(format!("{label}-outcome.json")),
                    json!({"disposition":"RESOURCE_LIMIT","reason":reason,"preservation_and_restore":"checked"}).to_string()).unwrap();
                continue;
            }
            assert!(
                candidate.status.success(),
                "complete index but candidate refused"
            );
            current_output(&cold_out, &version).unwrap();
            check_index(&cold, &nodes, &version).unwrap();
            sql(&store, &nodes, &version);
            runner.check_native(&format!("{label}-cold-inspect"), &store, &version, &bpe);
            let baseline = derived(&store);
            let warm = runner.current(&format!("{label}-warm"), req("index"), &version);
            assert_eq!(warm, cold);
            assert_eq!(derived(&store), baseline);
            let all: Vec<_> = nodes
                .iter()
                .map(|n| n["id"].as_str().unwrap().to_owned())
                .collect();
            for (scope, readable) in [
                (None, all.clone()),
                (Some(ident(0)), all.clone()),
                (Some(ident(size - 1)), vec![ident(size - 1)]),
                (None, vec![ident(size - 1)]),
            ] {
                let expected: Vec<_> = readable
                    .iter()
                    .filter(|i| {
                        scope
                            .as_ref()
                            .is_none_or(|s| s == *i || s == &ident(0) && shape != "roots")
                    })
                    .cloned()
                    .collect();
                let request = json!({"op":"search","store":store,"query":"capacity","under_node_id":scope,"limit":50,"offset":0,"readable_ids":readable});
                let result = runner.current(&format!("{label}-search"), request.clone(), &version);
                assert_eq!(result["total_matches"], expected.len());
                assert_eq!(result["other_groups"], json!([]));
                let actual: BTreeSet<_> = result["results"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| r["id"].as_str().unwrap().to_owned())
                    .collect();
                assert_eq!(actual.len(), 50.min(expected.len()));
                assert!(actual.iter().all(|i| expected.contains(i)));
                let mut wanted = expected.clone();
                wanted.sort();
                wanted.reverse();
                let mut seen = vec![];
                for offset in (0..wanted.len().max(1)).step_by(50) {
                    let mut request = request.clone();
                    request["query"] = json!("");
                    request["offset"] = json!(offset);
                    let page = runner.current(&format!("{label}-browse"), request, &version);
                    assert_eq!(page["total_matches"], wanted.len());
                    assert_eq!(page["other_groups"], json!([]));
                    let ids: Vec<_> = page["results"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|r| r["id"].as_str().unwrap().to_owned())
                        .collect();
                    assert_eq!(ids, wanted[offset..50.min(wanted.len() - offset) + offset]);
                    seen.extend(ids);
                }
                assert_eq!(seen, wanted);
            }
            for (owner, readable) in [
                (ident(0), all.clone()),
                (ident(size - 1), all.clone()),
                (ident(0), vec![ident(0), ident(size - 1)]),
            ] {
                let (out, v) = runner.run(
                    &format!("{label}-neighborhood"),
                    json!({"op":"neighborhood","store":store,"id":owner,"readable_ids":readable}),
                    None,
                );
                assert!(out.status.success());
                assert_eq!(v["version"], neighbor_version);
                assert_eq!(
                    v["neighborhood"],
                    neighborhood(&nodes, &owner, &store, &readable)
                );
            }
            let p = store.join("nodes").join(ident(size - 1)).join("node.md");
            let original = fs::read(&p).unwrap();
            change_field(
                &store,
                &ident(size - 1),
                "micro_summary",
                json!("Changed capacity summary Ω"),
            );
            let changed = identities(&inventory(&store)).0;
            let v = runner.current(&format!("{label}-changed"), req("index"), &changed);
            check_index(&v, &authored_records(&store), &changed).unwrap();
            runner.check_native(&format!("{label}-changed-inspect"), &store, &changed, &bpe);
            fs::write(&p, &original).unwrap();
            let v = runner.current(&format!("{label}-restore"), req("index"), &version);
            check_index(&v, &nodes, &version).unwrap();
            assert_eq!(
                generated_snapshot(&store),
                baseline
                    .iter()
                    .filter(|(p, _)| p.starts_with("nodes"))
                    .map(|(p, v)| (p.clone(), v.clone()))
                    .collect()
            );
            change_field(&store, &ident(size - 1), "micro_summary", json!(" "));
            let invalid = inventory(&store);
            let accepted = derived(&store);
            let (out, _) = runner.run(
                &format!("{label}-invalid-candidate"),
                json!({"store":store}),
                Some(vec!["validate-candidate".into(), "@request".into()]),
            );
            assert!(!out.status.success());
            assert!(
                !String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .any(|l| l == "status:current")
            );
            let (out, _) = runner.run(&format!("{label}-invalid-refresh"), req("index"), None);
            unavailable_output(&out, "index").unwrap();
            assert_eq!(inventory(&store), invalid);
            assert_eq!(derived(&store), accepted);
            fs::write(&p, &original).unwrap();
            let before = derived(&store);
            let killed = runner.killed(&format!("{label}-killed"), &store);
            fs::write(
                evidence.join(format!("{label}-killed.stderr")),
                &killed.stderr,
            )
            .unwrap();
            fs::write(evidence.join(format!("{label}-killed.receipt.json")),json!({"signal":killed.status.signal(),"stdout_bytes":killed.stdout.len(),"boundary":"pause_before_accept"}).to_string()).unwrap();
            assert_eq!(inventory(&store), source);
            assert_eq!(derived(&store), before);
            let v = runner.current(&format!("{label}-killed-recovery"), req("index"), &version);
            check_index(&v, &nodes, &version).unwrap();
            sql(&store, &nodes, &version);
            assert_eq!(derived(&store), before);
            assert!(!store.join(".sifttext/outline-stage").exists());
            assert_eq!(inventory(&store), source);
            fs::write(
                evidence.join(format!("{label}-outcome.json")),
                json!({"disposition":"COMPLETE","invalid_and_killed_recovery":"checked"})
                    .to_string(),
            )
            .unwrap();
        }
        fs::write(evidence.join("limits.txt"),"Only the named source manifests were exercised; remaining legacy shapes are not retired. Resource samples nominal50ms plus ps/lsof cost can miss short peaks and inflate wall time. RSS and regular-file byte totals are observations, not universal250ms/250MB thresholds. Pre-accept SIGKILL is not power-loss proof. Native per-file identity/freshness only feeds the independent authored row/byte/token oracle.\n").unwrap();
    }
}
