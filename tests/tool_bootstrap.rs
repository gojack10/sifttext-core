use std::{fs, path::Path, process::Command};
use std::os::unix::fs::PermissionsExt;

fn executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn bend_resolver_returns_exact_verified_selection() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    fs::create_dir(root.join("scripts")).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    fs::copy(source.join("scripts/bend-path.sh"), root.join("scripts/bend-path.sh")).unwrap();
    // Preserve the production digest function; only the sandbox platform pin differs.
    let pins = fs::read_to_string(source.join("scripts/tool-pins.sh")).unwrap();
    let hash_function = pins.split_once("platform_pins()").unwrap().0;
    let selected = root.join("selected tools/bend");
    fs::create_dir(selected.parent().unwrap()).unwrap();
    let pin = |body: &str| {
        executable(&selected, body);
        let digest = Command::new("shasum").args(["-a", "256"]).arg(&selected).output().unwrap();
        assert!(digest.status.success());
        let digest = String::from_utf8(digest.stdout).unwrap();
        let digest = digest.split_whitespace().next().unwrap();
        fs::write(root.join("scripts/tool-pins.sh"), format!(
            "{hash_function}platform_pins() {{ PLATFORM=test; BEND_SHA256={digest}; }}\n"
        )).unwrap();
    };
    let resolve = |selection: Option<&Path>| {
        let mut command = Command::new("sh");
        command.arg(root.join("scripts/bend-path.sh")).env_remove("BEND_BIN");
        if let Some(path) = selection { command.env("BEND_BIN", path); }
        command.output().unwrap()
    };
    pin("#!/bin/sh\necho 'bend 2.0.27'\n");
    for selection in [&selected, Path::new("selected tools/bend")] {
        let out = resolve(Some(selection));
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), selected.to_str().unwrap());
    }
    let default = root.join(".tools/bend-test/bend/bin/bend");
    fs::create_dir_all(default.parent().unwrap()).unwrap();
    fs::copy(&selected, &default).unwrap();
    let out = resolve(None);
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), default.to_str().unwrap());

    executable(&selected, "#!/bin/sh\necho 'bend 2.0.28'\n");
    let tampered = resolve(Some(&selected));
    assert!(!tampered.status.success());
    assert!(tampered.stdout.is_empty());
    assert!(String::from_utf8_lossy(&tampered.stderr).contains("digest mismatch"));
    pin("#!/bin/sh\necho 'bend 2.0.28'\n");
    let wrong_version = resolve(Some(&selected));
    assert!(!wrong_version.status.success());
    assert!(wrong_version.stdout.is_empty());
    assert!(String::from_utf8_lossy(&wrong_version.stderr).contains("version mismatch"));
}

#[test]
fn bootstrap_refuses_before_executing_untrusted_tools_or_overwriting_existing_tools() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    fs::create_dir(root.join("scripts")).unwrap();
    for file in ["bend-path.sh", "tool-pins.sh", "tool-env.sh", "setup-tools.sh"] {
        fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts").join(file), root.join("scripts").join(file)).unwrap();
    }
    let bin = root.join("bin");
    fs::create_dir(&bin).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let run = |script: &str, bend: &Path| {
        Command::new("sh").arg(root.join("scripts").join(script))
            .env("PATH", &path).env("BEND_BIN", bend).output().unwrap()
    };
    let forged = root.join("forged-bend");
    let marker = root.join("executed");
    executable(&forged, &format!("#!/bin/sh\ntouch '{}'\necho 'bend 2.0.27'\n", marker.display()));
    executable(&bin.join("uname"), "#!/bin/sh\ncase \"$1\" in -s) echo Plan9;; -m) echo x86_64;; esac\n");
    let unsupported = run("bend-path.sh", &forged);
    assert!(!unsupported.status.success());
    assert!(String::from_utf8_lossy(&unsupported.stderr).contains("Unsupported OS"));
    executable(&bin.join("uname"), "#!/bin/sh\ncase \"$1\" in -s) echo Linux;; -m) echo riscv64;; esac\n");
    let unsupported_cpu = run("setup-tools.sh", &forged);
    assert!(!unsupported_cpu.status.success());
    assert!(String::from_utf8_lossy(&unsupported_cpu.stderr).contains("Unsupported CPU"));
    assert!(!root.join(".tools").exists(), "unsupported targets must refuse before acquisition");
    executable(&bin.join("uname"), "#!/bin/sh\ncase \"$1\" in -s) echo Darwin;; -m) echo arm64;; esac\n");
    let untrusted = run("bend-path.sh", &forged);
    assert!(!untrusted.status.success());
    assert!(String::from_utf8_lossy(&untrusted.stderr).contains("digest mismatch"));
    assert!(!marker.exists(), "digest check must precede execution, not follow version output");
    fs::create_dir_all(root.join(".tools/bend-darwin-arm64/bend/bin")).unwrap();
    let existing = root.join(".tools/bend-darwin-arm64/bend/bin/bend");
    executable(&existing, "#!/bin/sh\necho old\n");
    let before = fs::read(&existing).unwrap();
    executable(&bin.join("curl"), "#!/bin/sh\nexit 99\n");
    let preservation = run("setup-tools.sh", &existing);
    assert!(!preservation.status.success());
    assert_eq!(fs::read(&existing).unwrap(), before);
    assert!(!marker.exists());
    fs::remove_dir_all(root.join(".tools/bend-darwin-arm64")).unwrap();
    let offline = run("setup-tools.sh", &forged);
    assert!(!offline.status.success());
    assert!(!root.join(".tools/bend-darwin-arm64").exists());
    assert_eq!(fs::read_dir(root.join(".tools")).unwrap().count(), 0, "failed download must clean staging, not leave a false install");
    executable(&bin.join("curl"), "#!/bin/sh\nfor arg do output=$arg; done\nprintf corrupt > \"$output\"\n");
    let corrupt_archive = run("setup-tools.sh", &forged);
    assert!(!corrupt_archive.status.success());
    assert!(String::from_utf8_lossy(&corrupt_archive.stderr).contains("Bend archive digest mismatch"));
    assert!(!marker.exists(), "untrusted Bend must never execute");
    assert_eq!(fs::read_dir(root.join(".tools")).unwrap().count(), 0, "bad digest must clean staging, not leave a false install");
}
