//! Modeled frame controls through the verified Bend resolver.
//! Run: sh scripts/with-tools.sh cargo test --locked --test tool_consumers
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn bend(root: &Path) -> Result<PathBuf, String> {
    let out = Command::new("sh")
        .arg(root.join("scripts/bend-path.sh"))
        .current_dir(root)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let path = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    if !path.is_absolute() {
        return Err("Bend resolver must return an absolute executable path".into());
    }
    Ok(path)
}

fn check(bend: &Path, source: &Path, log: &Path) -> Output {
    let out = Command::new(bend)
        .arg(source)
        .arg("--check-only")
        .output()
        .unwrap();
    let mut bytes = out.stdout.clone();
    bytes.extend_from_slice(&out.stderr);
    fs::write(log, bytes).unwrap();
    out
}

fn output(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn implementation_law_mutations(root: &Path) -> Result<PathBuf, String> {
    // Resolve before preparing fixtures, so unverified tools cannot reach the laws.
    let bend = bend(root)?;
    let base = std::env::var_os("SIFTTEXT_EVIDENCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let run = tempfile::Builder::new()
        .prefix("implementation-law-mutations-")
        .tempdir_in(base)
        .map_err(|e| e.to_string())?;
    // Preserve both successful and failed checker evidence, as the shell consumer did.
    let path = run.keep();
    for entry in fs::read_dir(root.join("laws")).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().extension().is_some_and(|ext| ext == "bend") {
            fs::copy(entry.path(), path.join(entry.file_name())).map_err(|e| e.to_string())?;
        }
    }
    for (name, outcome) in [
        ("reject-all", "S.Reject{}"),
        ("delete-all", "S.Accept{S.Graph{[], [], 0n}}"),
    ] {
        fs::write(
            path.join("Core.bend"),
            format!("import ./Spec.bend as S\ndef execute(g: S.Graph, r: S.Request) -> S.Outcome:\n  {outcome}\n"),
        )
        .map_err(|e| e.to_string())?;
        let failed = check(
            &bend,
            &path.join("PROOF.bend"),
            &path.join(format!("{name}.log")),
        );
        if failed.status.success() || !output(&failed).contains("Error:") {
            return Err(format!(
                "{name}: expected checked-law rejection; logs: {}\n{}",
                path.display(),
                output(&failed)
            ));
        }
    }
    Ok(path)
}

#[test]
fn implementation_law_mutations_are_rejected() {
    let path = implementation_law_mutations(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    println!(
        "Both deliberate violations rejected by frozen checked laws. Logs: {}",
        path.display()
    );
}

#[test]
fn implementation_law_mutations_refuse_false_rejections() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    fs::create_dir(root.join("scripts")).unwrap();
    fs::create_dir(root.join("laws")).unwrap();
    fs::write(root.join("laws/PROOF.bend"), "unused mock proof").unwrap();
    let tool = root.join("mock-bend");
    fs::write(
        root.join("scripts/bend-path.sh"),
        format!("printf '%s\\n' '{}'\n", tool.display()),
    )
    .unwrap();
    for script in [
        "#!/bin/sh\necho 'Error: synthetic'\nexit 0\n",
        "#!/bin/sh\necho unrelated-failure\nexit 1\n",
    ] {
        fs::write(&tool, script).unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(
            implementation_law_mutations(root)
                .unwrap_err()
                .contains("expected checked-law rejection")
        );
    }
}

#[test]
fn formal_controls_preserve_verified_tool_refusal() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    fs::create_dir(root.join("scripts")).unwrap();
    fs::write(
        root.join("scripts/bend-path.sh"),
        "echo verified-tool-refusal >&2\nexit 17\n",
    )
    .unwrap();
    assert!(bend(root).unwrap_err().contains("verified-tool-refusal"));
    assert!(
        implementation_law_mutations(root)
            .unwrap_err()
            .contains("verified-tool-refusal")
    );
    assert!(
        !root.join("laws").exists(),
        "refusal reached checker preparation"
    );

    fs::write(root.join("scripts/bend-path.sh"), "echo relative-bend\n").unwrap();
    assert!(bend(root).unwrap_err().contains("absolute executable path"));
    assert!(
        implementation_law_mutations(root)
            .unwrap_err()
            .contains("absolute executable path")
    );
}

#[test]
fn modeled_frame_mutations_are_typed_and_rejected_at_their_theorem() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let bend = bend(root).unwrap();
    let evidence = std::env::var_os("SIFTTEXT_EVIDENCE_DIR");
    let run = match evidence.as_ref() {
        Some(path) => {
            fs::create_dir_all(path).unwrap();
            tempfile::Builder::new()
                .prefix("frame-controls-")
                .tempdir_in(path)
        }
        None => tempfile::Builder::new().prefix("frame-controls-").tempdir(),
    }
    .unwrap();
    let path = run.path().to_path_buf();
    // Retain failed checker logs too when an evidence directory was requested.
    let _cleanup = if evidence.is_some() {
        let _ = run.keep();
        None
    } else {
        Some(run)
    };
    let baseline = check(
        &bend,
        &root.join("laws/PROOF.bend"),
        &path.join("baseline.log"),
    );
    assert!(baseline.status.success(), "{}", output(&baseline));
    assert!(output(&baseline).contains("All terms check."));
    let controls = [
        (
            "move-payload",
            "Node{i, p, next, r} <> moved(t, x, dst)",
            "Node{i, 0n, next, r} <> moved(t, x, dst)",
            "move_content_frame",
        ),
        (
            "reorder-other-sequences",
            "next = Bool.pick(List<&2, Nat>, Nat.is_eq(i, par), insert(remove(c, x), x, after), c)",
            "next = {[] : List<&2, Nat>}",
            "reorder_one_sequence",
        ),
        (
            "rename-references",
            "Node{i, Bool.pick(Nat, Nat.is_eq(i, x), payload, p), c, r} <> renamed(t, x, payload)",
            "Node{i, Bool.pick(Nat, Nat.is_eq(i, x), payload, p), c, []} <> renamed(t, x, payload)",
            "rename_one_payload",
        ),
        (
            "rename-other-payloads",
            "Node{i, Bool.pick(Nat, Nat.is_eq(i, x), payload, p), c, r} <> renamed(t, x, payload)",
            "Node{i, payload, c, r} <> renamed(t, x, payload)",
            "rename_one_payload",
        ),
    ];
    for (name, old, new, theorem) in controls {
        let dir = path.join(name);
        fs::create_dir_all(dir.join("next-core")).unwrap();
        for file in ["Spec.bend", "Core.bend", "next-core/Frames.bend"] {
            fs::copy(root.join("laws").join(file), dir.join(file)).unwrap();
        }
        let spec = dir.join("Spec.bend");
        let source = fs::read_to_string(&spec).unwrap();
        assert_eq!(source.matches(old).count(), 1, "{name}: unique mutation");
        fs::write(&spec, source.replacen(old, new, 1)).unwrap();
        let typed = check(&bend, &spec, &dir.join("mutation-typecheck.log"));
        assert!(
            typed.status.success(),
            "{name}: mutation must be well-typed: {}",
            output(&typed)
        );
        let failed = check(
            &bend,
            &dir.join("next-core/Frames.bend"),
            &dir.join("frame-rejection.log"),
        );
        let text = output(&failed);
        assert!(
            !failed.status.success(),
            "{name}: violating mutation passed"
        );
        assert!(text.contains("Error:"), "{name}: {text}");
        assert!(
            text.contains(&format!("Location: {theorem}")),
            "{name}: wrong rejection location: {text}"
        );
        println!("{name}: type-correct mutation rejected at {theorem}");
    }
    if evidence.is_some() {
        println!(
            "Modeled frame controls retained at {}; no Lean-kernel verdict claimed.",
            path.display()
        );
    }
}
