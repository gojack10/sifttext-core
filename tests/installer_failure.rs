//! Shell lifecycle controls only; the native installer workflow is file_first_kb.rs.
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    process::Command,
};

#[test]
fn failed_install_removes_only_its_new_application_and_can_retry() {
    let temporary = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temporary.path()).unwrap();
    let source = root.join("source");
    fs::create_dir(&source).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("install-local.sh"),
        source.join("install-local.sh"),
    )
    .unwrap();
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "README.md",
        "AGENTS.md",
        ".gitignore",
        "rust-toolchain.toml",
    ] {
        fs::write(source.join(name), "fixture").unwrap();
    }
    for name in [
        "src",
        "laws",
        "scripts",
        "tests",
        "fixtures",
        "nodes",
        "trees",
        "build",
        "target/debug",
    ] {
        fs::create_dir_all(source.join(name)).unwrap();
    }
    // Inject deterministic refusal at the source/installed/initialization boundaries.
    let binary = source.join("target/debug/sifttext");
    fs::write(
        &binary,
        r#"#!/bin/sh
case "$1" in
  runtime-check)
    case "$0" in
      */target/debug/*) [ "$FAIL_AT" != source ] ;;
      *) [ "$FAIL_AT" != installed ] ;;
    esac ;;
  init)
    if [ "$FAIL_AT" = init ]; then
      mkdir -p "$2"
      printf 'retain incomplete knowledge\n' > "$2/partial"
      exit 23
    fi ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let prefix = root.join("application");
    let home = root.join("knowledge");
    let run = |failure: &str| {
        Command::new("sh")
            .arg(source.join("install-local.sh"))
            .arg(&prefix)
            .arg(&home)
            .env("FAIL_AT", failure)
            .output()
            .unwrap()
    };
    for failure in ["source", "copy", "installed", "init"] {
        if failure == "copy" {
            fs::remove_file(source.join("Cargo.lock")).unwrap();
        }
        let result = run(failure);
        assert!(!result.status.success(), "{failure} unexpectedly succeeded");
        assert!(!prefix.exists(), "{failure} left a partial application");
        if failure == "copy" {
            fs::write(source.join("Cargo.lock"), "fixture").unwrap();
        }
    }
    assert_eq!(
        fs::read_to_string(home.join("partial")).unwrap(),
        "retain incomplete knowledge\n"
    );
    // Generated build outputs may live behind a /tmp symlink, but an install
    // must own its copied assets rather than share mutable source outputs.
    let assets = root.join("assets");
    fs::rename(source.join("build"), &assets).unwrap();
    fs::write(assets.join("candidate.sha256"), "checked assets\n").unwrap();
    symlink(&assets, source.join("build")).unwrap();
    assert!(
        run("none").status.success(),
        "clean prefix could not be retried"
    );
    assert!(
        !fs::symlink_metadata(prefix.join("build"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    fs::write(prefix.join("build/candidate.sha256"), "installed change\n").unwrap();
    assert_eq!(
        fs::read_to_string(assets.join("candidate.sha256")).unwrap(),
        "checked assets\n"
    );
    fs::remove_dir_all(&assets).unwrap();
    assert_eq!(
        fs::read_to_string(prefix.join("build/candidate.sha256")).unwrap(),
        "installed change\n"
    );
    let saved = fs::read(prefix.join("sifttext")).unwrap();
    assert!(!run("init").status.success(), "existing prefix accepted");
    assert_eq!(fs::read(prefix.join("sifttext")).unwrap(), saved);
    fs::remove_dir_all(&prefix).unwrap();
    symlink(&home, &prefix).unwrap();
    assert!(!run("init").status.success(), "symlink prefix accepted");
    assert_eq!(fs::read_link(&prefix).unwrap(), home);
    assert_eq!(
        fs::read_to_string(home.join("partial")).unwrap(),
        "retain incomplete knowledge\n"
    );
}
