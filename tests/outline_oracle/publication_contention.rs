// Process-level outline publication controls retained from outlines.mjs.
// This slice does not retire the legacy runner's other scenario families.
mod publication_contention {
    use super::*;

    fn start(request: Value) -> capacity::ObserverJob {
        let bin = std::env::var_os("CORE_OBSERVE_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/core-observe")
            });
        assert!(bin.is_file(), "build core-observe first: {}", bin.display());
        let mut command = Command::new(&bin);
        command.current_dir(bin.parent().unwrap().parent().unwrap().parent().unwrap());
        capacity::ObserverJob::start(command, request.to_string().into_bytes())
    }

    fn current(job: capacity::ObserverJob, label: &str) -> Value {
        let out = job.finish();
        let v = dogfood_outcome(out.status, &out.stdout, "current").unwrap_or_else(|e| {
            panic!(
                "{label}: {e}; stderr {}",
                String::from_utf8_lossy(&out.stderr)
            )
        });
        assert!(v["version"].as_str().is_some_and(|v| !v.is_empty()));
        eprintln!("OUTLINE_CONTENTION {label}: current {}", v["version"]);
        v
    }

    fn assert_files(store: &Path, inspection: &Value, version: &str, bpe: &tiktoken_rs::CoreBPE) {
        assert_eq!(inspection["version"], version);
        assert_eq!(
            inspection["files"].as_array().unwrap().len(),
            authored_records(store).len() * 4
        );
        inspected_bytes(inspection); // Every returned byte/hash matches the actual generated set.
        check_native(store, version, bpe); // Authored rows, pinned LEAN and independent costs.
    }

    #[test]
    fn locked_reader_and_simultaneous_publishers_return_complete_current_outlines() {
        let bpe = ORACLE_BPE.clone();
        let dir = TempDir::new().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let store = root.join("store");
        materialize(&store, &specimen());
        let initial = refresh(&store, "full");
        change_field(
            &store,
            specimen()[0]["id"].as_str().unwrap(),
            "name",
            json!("New locked publication Ω"),
        );
        let authored = authored_snapshot(&store);
        let barriers = root.join("barriers");
        fs::create_dir(&barriers).unwrap();
        let ready = barriers.join("ready");
        let release = barriers.join("release");
        let mut publisher = start(json!({"op":"index", "store":store, "mode":"incremental",
            "pause_before_accept":{"ready":ready,"release":release}}));
        let began = Instant::now();
        while !ready.exists() {
            assert!(publisher.running(), "publisher exited before barrier");
            assert!(
                began.elapsed() < Duration::from_secs(10),
                "publisher barrier expired"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut reader = start(json!({"op":"outline_inspect", "store":store}));
        // Bounded observation, not a claim about arbitrary scheduler interleavings.
        std::thread::sleep(Duration::from_millis(50));
        assert!(
            reader.running(),
            "reader returned before publication release"
        );
        publisher.check_active();
        fs::write(&release, "go").unwrap();
        let published = current(publisher, "locked-publisher");
        let version = published["version"].as_str().unwrap();
        assert_ne!(version, initial);
        assert_files(&store, &current(reader, "locked-reader"), version, &bpe);
        assert_eq!(authored_snapshot(&store), authored);
        let generated = generated_snapshot(&store);

        // All three children are launched before any is awaited, as in the legacy control.
        let jobs = ["index", "outline_inspect", "index"].map(|op| {
            let mut request = json!({"op":op,"store":store});
            if op == "index" {
                request["mode"] = json!("incremental");
            }
            (op, start(request))
        });
        for (op, job) in jobs {
            let v = current(job, op);
            assert_eq!(v["version"], version);
            if op == "outline_inspect" {
                assert_files(&store, &v, version, &bpe);
            }
        }
        assert_eq!(authored_snapshot(&store), authored);
        assert_eq!(generated_snapshot(&store), generated);
        assert!(!store.join(".sifttext/outline-stage").exists());
        check_native(&store, version, &bpe);
    }
    include!("lifetime_controls.rs");
}
