use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

fn helper(body: &str) -> (tempfile::TempDir, Policy) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("build")).unwrap();
    let executable = dir.path().join("build/graph-policy");
    fs::write(&executable, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let mut stamp = String::new();
    for file in ["Cargo.toml", "Cargo.lock", "README.md", ".gitignore"] {
        fs::write(dir.path().join(file), "synthetic helper fixture\n").unwrap();
        stamp.push_str(&format!(
            "{}  {file}\n",
            sha256(b"synthetic helper fixture\n")
        ));
    }
    fs::write(dir.path().join("build/candidate.sha256"), stamp).unwrap();
    fs::write(
        dir.path().join("build/graph-policy.sha256"),
        format!(
            "{}  build/graph-policy\n",
            sha256(&fs::read(executable).unwrap())
        ),
    )
    .unwrap();
    let policy = Policy::checked(dir.path()).unwrap();
    (dir, policy)
}

fn graph(refs: usize) -> Graph {
    Graph {
        assets: 1,
        roots: vec![1],
        nodes: vec![WireNode {
            id: 1,
            payload: 2,
            children: vec![],
            refs: vec![(1, 3); refs],
        }],
    }
}

fn wire(graph: &Graph) -> String {
    let mut words = vec![1, 3, 1, 2];
    words.extend(graph_words(graph).unwrap());
    wire_text(&words).unwrap()
}

#[test]
fn larger_rust_framing_has_complete_request_sized_output_not_backend_credit() {
    // All integers are small, and the complete request fits the frozen reader.
    // A lossless zero-padded decimal reply exercises the existing Rust numeric
    // grammar beyond 1 MiB. The compiled backend itself emits minimal decimals:
    // this helper test proves framing/decoding, NOT large Bend graph acceptance.
    let graph = graph(100_000);
    let input = wire(&graph);
    assert!(input.len() < BACKEND_INPUT_BYTES);
    let reply = graph_words(&graph)
        .unwrap()
        .iter()
        .map(|word| format!("{word:06}"))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(reply.len() > LIMIT);
    assert_eq!(parse_graph(&reply, &graph).unwrap(), graph);
    let expected = format!("STG1 OK {reply}\n");
    let reply_file = tempfile::NamedTempFile::new().unwrap();
    fs::write(reply_file.path(), &expected).unwrap();
    let (_dir, policy) = helper(&format!("cat '{}'", reply_file.path().display()));
    let budget = decimal_budget(
        graph_word_count(&graph).unwrap(),
        graph_integer_envelope(&graph).unwrap(),
    )
    .unwrap()
        + 9;
    let start = Instant::now();
    assert_eq!(policy.call(3, 1, 2, &graph).unwrap(), expected);
    assert_eq!(
        invoke(&policy.executable, input.as_bytes()).unwrap(),
        expected
    );
    assert!(invoke_bounded(&policy.executable, input.as_bytes(), expected.len() - 1).is_err());
    // The real stamped call's exact cardinality/type boundary still rejects a
    // graph-shaped reply where a VALID/REJECT result was requested.
    assert!(policy.call(0, 0, 0, &graph).is_err());
    eprintln!(
        "Rust framing input={} output={} budget={budget} elapsed_ms={}",
        input.len(),
        expected.len(),
        start.elapsed().as_millis()
    );
}

#[test]
fn frozen_input_boundary_rejects_suffix_before_spawning_or_truncating() {
    let (dir, policy) = helper("touch \"$SIFTTEXT_WIRE\".spawned; printf 'STG1 VALID\\n'");
    let missing = dir.path().join("not-an-executable");
    let mut input = b"1 0 0 0 1 0 0".to_vec();
    input.resize(BACKEND_INPUT_BYTES + 1, b' ');
    assert_eq!(
        invoke(&missing, &input).unwrap_err().to_string(),
        "wire exceeds frozen backend input limit"
    );
    assert_eq!(
        invoke(&policy.executable, &input).unwrap_err().to_string(),
        "wire exceeds frozen backend input limit"
    );
    input.pop();
    assert_eq!(invoke(&policy.executable, &input).unwrap(), "STG1 VALID\n");
    assert_eq!(
        invoke(
            &Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR")))
                .unwrap()
                .executable,
            &input
        )
        .unwrap(),
        "STG1 VALID\n"
    );
    assert!(wire_text(&vec![usize::MAX; BACKEND_INPUT_BYTES / 10]).is_err());
}

#[test]
fn checked_budgets_allocation_and_hostile_count_edges() {
    assert!(decimal_budget(usize::MAX, usize::MAX).is_err());
    assert!(decimal_budget(0, 0).is_err());
    assert!(reserved::<u8>(usize::MAX).is_err());
    assert!(process_output(&b""[..], usize::MAX).is_err());
    assert_eq!(process_output(&b"123"[..], 3).unwrap(), b"123");
    assert!(process_output(&b"1234"[..], 3).is_err());
    let mut graph = graph(3);
    graph.assets = usize::MAX;
    assert!(graph_integer_envelope(&graph).is_err());
    graph.assets = 1;
    let text = wire(&graph).strip_prefix("1 3 1 2 ").unwrap().to_owned();
    for reply in [
        format!("{text} 0"),
        text.rsplit_once(' ').unwrap().0.into(),
        text.replacen("1 ", &format!("{} ", usize::MAX), 1),
        "0".repeat(2_000_000),
    ] {
        assert!(parse_graph(&reply, &graph).is_err());
    }
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let id = "70000000-0000-4000-8000-000000000001";
    let node = dir.path().join("nodes").join(id);
    fs::create_dir_all(node.join("children")).unwrap();
    fs::create_dir(dir.path().join("trees")).unwrap();
    fs::write(node.join("children.order"), "").unwrap();
    fs::write(node.join("node.md"), format!("---\nid: {id:?}\nname: \"Synthetic\"\nmicro_summary: \"Public-safe fixture\"\nstatus: \"in_progress\"\nsee_also: []\n---\nBody\n")).unwrap();
    symlink(format!("../nodes/{id}"), dir.path().join("trees").join(id)).unwrap();
    dir
}

#[test]
fn compiled_policy_and_failed_helpers_preserve_source_derived_state_and_recover() {
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let dir = fixture();
    let accepted = index::refresh(&policy, dir.path(), None, index::Mode::Full, None).unwrap();
    // Capture the accepted inventory after its derived-cache directory exists.
    let source = Store::read(dir.path()).unwrap();
    let cache = dir.path().join(".sifttext/index.sqlite");
    let cache_hash = sha256(&fs::read(&cache).unwrap());
    policy.validate(&source).unwrap();
    let mut invalid = source.clone();
    invalid.roots.clear();
    assert_eq!(
        policy.validate(&invalid).unwrap_err().to_string(),
        "checked graph rejected or malformed verdict"
    );
    for (label, script, diagnostic) in [
        (
            "truncated",
            "printf 'STG1 VAL'",
            "malformed checked graph verdict",
        ),
        (
            "excess",
            "printf 'STG1 VALID\\nEXCESS'",
            "malformed checked graph verdict",
        ),
        ("nonzero", "exit 7", "policy process exited nonzero"),
        (
            "stderr",
            "printf diagnostic >&2; exec sleep 30",
            "unexpected policy stderr",
        ),
        ("timeout", "exec sleep 30", "policy process timed out"),
        (
            "hostile streaming",
            "exec yes hostile",
            "malformed checked graph verdict",
        ),
    ] {
        let (_helper, bad) = helper(script);
        let start = Instant::now();
        assert_eq!(
            bad.validate(&source).unwrap_err().to_string(),
            diagnostic,
            "{label}"
        );
        let elapsed = start.elapsed();
        if label == "timeout" {
            assert!(elapsed >= Duration::from_secs(10) && elapsed < Duration::from_secs(15));
        }
        if label == "hostile streaming" {
            assert!(elapsed < Duration::from_secs(5));
        }
        let unavailable =
            index::refresh(&bad, dir.path(), None, index::Mode::Incremental, None).unwrap();
        assert_eq!(unavailable.status, "error");
        assert!(
            unavailable.version.is_none()
                && unavailable.nodes.is_empty()
                && unavailable.links.is_empty()
        );
        assert_eq!(Store::read(dir.path()).unwrap().inventory, source.inventory);
        assert_eq!(sha256(&fs::read(&cache).unwrap()), cache_hash);
        let recovered =
            index::refresh(&policy, dir.path(), None, index::Mode::Incremental, None).unwrap();
        assert_eq!(recovered.version, accepted.version);
        eprintln!(
            "{label} diagnostic={diagnostic:?} elapsed_ms={} source/cache preserved; valid recovery",
            elapsed.as_millis()
        );
    }
    let (_helper, forged) = helper("printf 'STG1 OK 1 1 1 1 2 2 0 0\\n'");
    assert!(
        apply(
            &forged,
            &source,
            &Operation::Rename {
                node: source.roots[0].clone(),
                name: "Forged rename".into(),
            }
        )
        .is_err()
    );
    assert_eq!(Store::read(dir.path()).unwrap().inventory, source.inventory);
    assert_eq!(sha256(&fs::read(&cache).unwrap()), cache_hash);
    let (_helper, stamped) = helper("printf 'STG1 VALID\\n'");
    fs::write(&stamped.executable, "changed executable").unwrap();
    assert!(stamped.validate(&source).is_err());
    assert!(Policy::checked(&stamped.project).is_err());
}

#[test]
fn reader_refusals_preserve_native_diagnostic_identity() {
    for (script, diagnostic) in [
        (
            "printf private >&2; exit 7",
            "policy process exited nonzero",
        ),
        ("printf private >&2", "unexpected policy stderr"),
        ("exec yes hostile", "policy process output exceeds limit"),
    ] {
        let (_dir, helper) = helper(script);
        for _ in 0..20 {
            let error = invoke_bounded(&helper.executable, b"1", 11).unwrap_err();
            assert_eq!(error.to_string(), diagnostic);
            // Native refusal classification treats real IO errors separately
            // from these fixed protocol identities; do not wrap them as IO.
            assert!(error.downcast_ref::<std::io::Error>().is_none());
        }
    }
    let (_dir, helper) = helper("printf '\\377'");
    assert!(
        invoke_bounded(&helper.executable, b"1", 11)
            .unwrap_err()
            .downcast_ref::<std::str::Utf8Error>()
            .is_some()
    );
}

#[test]
fn repeated_process_rejection_does_not_accumulate_descriptors() {
    // Run alone for the descriptor measurement; normal parallel tests may open
    // unrelated files. /dev/fd exists on both supported macOS/Linux platforms.
    let (_dir, helper) = helper("exec yes hostile");
    let before = fs::read_dir("/dev/fd").unwrap().count();
    let start = Instant::now();
    for _ in 0..50 {
        assert!(invoke_bounded(&helper.executable, b"1", 11).is_err());
    }
    thread::sleep(Duration::from_millis(50));
    let after = fs::read_dir("/dev/fd").unwrap().count();
    eprintln!(
        "process rejection repetitions=50 fd_before={before} fd_after={after} elapsed_ms={}",
        start.elapsed().as_millis()
    );
    // Parallel test execution can open descriptors; the serial focused run
    // reports the exact before/after and checks the tighter resource bound.
    if std::env::var_os("SIFTTEXT_SERIAL_PROCESS_TEST").is_some() {
        assert!(after <= before);
    }
}
