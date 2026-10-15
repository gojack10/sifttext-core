//! Full request controls for the checked validator, not the graph-only drivers.
use sifttext_file_graph::{Policy, invoke};
use std::{fs, path::Path, process::Command};

// Root 1 has children 2,3; node 2 references 3. Payloads/assets are nonzero.
const GRAPH: &str = "99 1 1 3 1 10 2 2 3 0 2 20 0 1 3 1 3 30 0 0";

#[test]
fn validation_and_unchanged_mutations_use_full_runtime_wire() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    Policy::checked(root).unwrap(); // Complete source/executable stamp, no alternate acceptance.
    let executable = root.join("build/graph-policy");
    let cases = [
        (format!("1 0 0 0 {GRAPH}"), "STG1 VALID\n"),
        ("1 0 0 0 99 0 0".into(), "STG1 VALID\n"),
        ("1 0 0 0 99 1 1 1 1 10 0 0".into(), "STG1 VALID\n"),
        // Dangling child, unrooted cycle, duplicate roots, empty reference reason.
        ("1 0 0 0 99 1 1 1 1 10 1 2 0".into(), "STG1 REJECT\n"),
        (
            "1 0 0 0 99 0 2 1 10 1 2 0 2 20 1 1 0".into(),
            "STG1 REJECT\n",
        ),
        ("1 0 0 0 99 2 1 1 1 1 10 0 0".into(), "STG1 REJECT\n"),
        (
            format!("1 0 0 0 {}", GRAPH.replacen("1 3 1", "1 3 0", 1)),
            "STG1 REJECT\n",
        ),
        // Prescribed exact transformed graph, preserving payloads, references/assets.
        (
            format!("1 1 3 2 {GRAPH}"),
            "STG1 OK 99 1 1 3 1 10 1 2 0 2 20 1 3 1 3 1 3 30 0 0\n",
        ),
        (
            format!("1 2 2 3 {GRAPH}"),
            "STG1 OK 99 1 1 3 1 10 2 3 2 0 2 20 0 1 3 1 3 30 0 0\n",
        ),
        (
            format!("1 3 2 77 {GRAPH}"),
            "STG1 OK 99 1 1 3 1 10 2 2 3 0 2 77 0 1 3 1 3 30 0 0\n",
        ),
        (format!("1 1 1 2 {GRAPH}"), "STG1 REJECT\n"),
        (format!("1 2 3 99 {GRAPH}"), "STG1 REJECT\n"),
        (format!("1 2 3 0 {GRAPH}"), "STG1 REJECT\n"),
        (format!("1 3 9 77 {GRAPH}"), "STG1 REJECT\n"),
    ];
    for (wire, expected) in cases {
        assert_eq!(
            invoke(&executable, wire.as_bytes()).unwrap(),
            expected,
            "{wire}"
        );
        // Optional independent frozen executable supplied by isolated qualification.
        if let Some(original) = std::env::var_os("SIFTTEXT_ORIGINAL_POLICY") {
            assert_eq!(
                invoke(Path::new(&original), wire.as_bytes()).unwrap(),
                expected,
                "frozen: {wire}"
            );
        }
    }
    for (wire, diagnostic) in [
        ("".into(), "bad wire"),
        ("1 0 0".into(), "bad wire"),
        (format!("2 0 0 0 {GRAPH}"), "bad wire"),
        (format!("1 0 0 0 {GRAPH} nope"), "bad wire"),
        ("1 0 0 0 99 1 1 1 1 10 0".into(), "bad graph"),
        (format!("1 0 0 0 {GRAPH} 99"), "bad graph"),
        (format!("1 9 0 0 {GRAPH}"), "bad operation"),
    ] {
        // The public invoker intentionally hides helper stderr on nonzero exit.
        assert!(invoke(&executable, wire.as_bytes()).is_err(), "{wire}");
        let input = tempfile::NamedTempFile::new().unwrap();
        fs::write(input.path(), &wire).unwrap();
        let mut executables = vec![executable.clone()];
        if let Some(original) = std::env::var_os("SIFTTEXT_ORIGINAL_POLICY") {
            executables.push(original.into());
        }
        for helper in executables {
            let output = Command::new(&helper)
                .env("SIFTTEXT_WIRE", input.path())
                .args(["--threads", "1", "--gpu", "off"])
                .output()
                .unwrap();
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(!output.status.success(), "{helper:?}: {wire}");
            assert!(text.contains(diagnostic), "{helper:?}: {wire}: {text}");
            assert!(
                !text.contains("STG1"),
                "refusal advertised a verdict: {text}"
            );
        }
    }
}

#[test]
fn wrong_rooted_computation_is_rejected_by_equivalence_proof() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("sh")
        .arg(root.join("scripts/bend-path.sh"))
        .output()
        .unwrap();
    assert!(out.status.success());
    let bend = String::from_utf8(out.stdout).unwrap();
    let fixture = tempfile::tempdir().unwrap();
    let laws = fixture.path().join("laws");
    let validation = laws.join("successor/validation");
    fs::create_dir_all(&validation).unwrap();
    fs::copy(root.join("laws/Spec.bend"), laws.join("Spec.bend")).unwrap();
    for file in ["Graph.bend", "Rooted.bend", "Equivalence.bend"] {
        fs::copy(
            root.join("laws/successor/validation").join(file),
            validation.join(file),
        )
        .unwrap();
    }
    let check = || {
        Command::new(bend.trim())
            .arg(validation.join("Graph.bend"))
            .arg("--check-only")
            .output()
            .unwrap()
    };
    let baseline = check();
    assert!(
        baseline.status.success(),
        "{}",
        String::from_utf8_lossy(&baseline.stderr)
    );
    let path = validation.join("Rooted.bend");
    let source = fs::read_to_string(&path).unwrap();
    assert_eq!(source.matches("S.member(roots, x)").count(), 2);
    fs::write(path, source.replacen("S.member(roots, x)", "False{}", 1)).unwrap();
    let mutant = check();
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&mutant.stdout),
        String::from_utf8_lossy(&mutant.stderr)
    );
    assert!(!mutant.status.success(), "wrong-rooted computation passed");
    assert!(diagnostic.contains("rooted_equivalent"), "{diagnostic}");
}
