// Build outline-forest-observe, then set CORE_OUTLINE_FOREST_BIN to its path.
// cargo test --test outline_file_oracle -- pure_scaling_cold_warm_bytes_and_independent_costs --ignored --exact
// Explicit pure-observer campaign; native capacity remains separate.
#[test]
#[ignore = "explicit whole-forest scaling campaign; includes quadratic chain1000 output"]
fn pure_scaling_cold_warm_bytes_and_independent_costs() {
    let root = TempDir::new().unwrap();
    let evidence = std::env::var_os("OUTLINE_SCALING_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.path().join("scaling"));
    fs::create_dir(&evidence).unwrap();
    let bin = std::env::var_os("CORE_OUTLINE_FOREST_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/outline-forest-observe")
        });
    assert!(
        bin.is_file(),
        "build the actual outline-forest-observe first"
    );
    let bpe = ORACLE_BPE.clone();
    let mut measurements = vec![];
    for (size, shape) in [
        (14, "two-roots"),
        (100, "chain"),
        (1000, "chain"),
        (1698, "two-roots"),
        (1698, "wide"),
    ] {
        let ident = |i| format!("81000000-0000-4000-8000-{i:012}");
        let nodes: Vec<_> = (0..size).map(|i| {
            let cs: Vec<_> = match shape {
                "chain" => ((i+1)..size.min(i+2)).map(ident).collect(),
                "wide" if i == 0 => (1..size).map(ident).collect(),
                "two-roots" if i < 2 => (2..size).filter(|j| j%2 == i).map(ident).collect(),
                _ => vec![],
            };
            json!({"id":ident(i),"name":format!("Node {i}"),"micro_summary":format!("Summary {i}"),"status":"in_progress","children":cs})
        }).collect();
        let label = format!("{shape}-{size}");
        let request = json!({"source_version":"scaling-v1","nodes":nodes});
        fs::write(
            evidence.join(format!("{label}.input.json")),
            request.to_string(),
        )
        .unwrap();
        let mut cold = None;
        for mode in ["cold", "warm"] {
            let stdout = evidence.join(format!("{label}-{mode}.json"));
            let stderr = evidence.join(format!("{label}-{mode}.stderr"));
            let start = Instant::now();
            let mut command = Command::new("/usr/bin/time");
            #[cfg(target_os = "macos")]
            command.arg("-l");
            #[cfg(target_os = "linux")]
            command.arg("-v");
            let mut child = command
                .arg(&bin)
                .stdin(Stdio::piped())
                .stdout(fs::File::create(&stdout).unwrap())
                .stderr(fs::File::create(&stderr).unwrap())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(request.to_string().as_bytes())
                .unwrap();
            let status = child.wait().unwrap();
            let diagnostic = fs::read_to_string(&stderr).unwrap();
            assert!(status.success(), "{label}-{mode}: {status}; {diagnostic}");
            let bytes = fs::read(&stdout).unwrap();
            let rss = native_resources::native_rss(&diagnostic);
            assert!(rss > 0, "{label}-{mode}: missing resource measurement");
            measurements.push(json!({"label":label,"mode":mode,"ms":start.elapsed().as_secs_f64()*1000.,"output_bytes":bytes.len(),"native_rss_bytes":rss,"exit":status.code()}));
            fs::write(
                evidence.join("measurements.json"),
                serde_json::to_vec_pretty(&measurements).unwrap(),
            )
            .unwrap();
            if let Some(ref original) = cold {
                assert_eq!(&bytes, original, "{label}: cold/warm byte drift");
                continue;
            }
            if let Some(baseline) = std::env::var_os("SIFTTEXT_OUTLINE_BASELINE") {
                assert_eq!(
                    bytes,
                    fs::read(PathBuf::from(baseline).join(format!("{label}-cold.json"))).unwrap(),
                    "{label}: retained baseline drift"
                );
            }
            let actual: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(actual["source_version"], "scaling-v1");
            let expected = expectations(&nodes, "scaling-v1", &bpe).unwrap();
            assert_eq!(
                actual["owners"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect::<BTreeSet<_>>(),
                expected.keys().cloned().collect()
            );
            for (owner, views) in expected {
                for coverage in ["full", "shallow"] {
                    // Whole view equality also rejects unexpected publisher keys.
                    assert_eq!(
                        actual["owners"][&owner][coverage],
                        json!({"text":views[coverage]["text"],"meta":views[coverage]["meta"]}),
                        "{label}: {owner}/{coverage}"
                    );
                }
            }
            cold = Some(bytes);
        }
    }
    fs::write(evidence.join("limits.txt"), "Pure synthetic assembly only, not native import/publication or performance improvement. Fresh processes, not dropped filesystem caches. Chain1000 inherently emits500500 full rows. RSS is measured per observer, not an aggregate250MB guarantee. Native descriptor, invalid-store and recovery controls remain in the separate Rust capacity consumer.\n").unwrap();
}
