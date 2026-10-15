//! Contributor probes must not trigger Rustup installation in existing user homes.
use std::{fs, path::Path, process::Command};

#[test]
fn clang_prerequisite_requires_version_14_or_newer() {
    let fixture = tempfile::tempdir().unwrap();
    let bin = fixture.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let clang = bin.join("clang");
    fs::write(&clang, "#!/bin/sh\nprintf '%s\\n' 'clang version 14.0.0'\n").unwrap();
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&clang, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let check = |expected| {
        Command::new("sh")
            .args(["-c", "ROOT=$PWD; . scripts/tool-env.sh; tool_env_verify_clang"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("PATH", &path)
            .status().unwrap().success() == expected
    };
    assert!(check(true));
    fs::write(&clang, "#!/bin/sh\nprintf '%s\\n' 'clang version 13.9.0'\n").unwrap();
    assert!(check(false));
}

#[test]
fn shared_tool_environment_disables_implicit_rustup_installation() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    fs::create_dir(root.join("scripts")).unwrap();
    for script in ["tool-env.sh", "with-tools.sh"] {
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts").join(script),
            root.join("scripts").join(script),
        ).unwrap();
    }
    for command in [
        "ROOT=$PWD; . scripts/tool-env.sh; tool_env_apply; test \"$RUSTUP_AUTO_INSTALL\" = 0",
        "sh scripts/with-tools.sh sh -c 'test \"$RUSTUP_AUTO_INSTALL\" = 0'",
    ] {
        let result = Command::new("sh")
            .args(["-c", command])
            .current_dir(root)
            .env("RUSTUP_AUTO_INSTALL", "1")
            .env("RUSTUP_HOME", root.join("user-rustup"))
            .env("CARGO_HOME", root.join("user-cargo"))
            .output().unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        assert!(!root.join("user-rustup").exists());
        assert!(!root.join("user-cargo").exists());
    }
}
