//! Rust replacement for the retired Node release-bootstrap check. Exercises the public
//! agent-facing `install.sh` release path only: the official release record, the published
//! digest binding, and the refusals that must leave no partial application behind.
//! Build checked assets once with scripts/build.sh, then run
//! `cargo test --locked --test release_bootstrap`.
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

fn script(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn release_installer_never_invokes_github_cli_and_refuses_an_unverified_package() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let fixture = root.join("fixture");
    let commands = root.join("commands");
    let gh_calls = fixture.join("gh-calls");
    let requests = fixture.join("requests");
    fs::create_dir_all(fixture.join("package")).unwrap();
    fs::create_dir(&commands).unwrap();

    script(
        &fixture.join("package/sifttext"),
        r#"#!/bin/sh
set -eu
case "$1" in
  runtime-check) [ "${FAIL_RUNTIME:-0}" != 1 ] ;;
  install-home)
    if [ ! -f "$2/state.json" ]; then
      printf 'fixture\n' > "$2/state.json"
      mkdir -p "$3"
    fi ;;
  instructions) printf 'fixture instructions\n' ;;
esac
"#,
    );
    assert!(
        Command::new("tar")
            .arg("-czf")
            .arg(fixture.join("archive.tar.gz"))
            .arg("-C")
            .arg(&fixture)
            .arg("package")
            .status()
            .unwrap()
            .success()
    );
    let digest = sifttext_file_graph::sha256(&fs::read(fixture.join("archive.tar.gz")).unwrap());
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        "linux" => "linux",
        other => panic!("no official release fixture for {other}"),
    };
    let cpu = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => panic!("no official release fixture for {other}"),
    };
    fs::write(
        fixture.join("SHA256SUMS"),
        format!(
            "# SiftText release v0.1.0 source {}\n{digest}  sifttext-{os}-{cpu}.tar.gz\n",
            "a".repeat(40)
        ),
    )
    .unwrap();
    script(
        &commands.join("curl"),
        r#"#!/bin/sh
set -eu
out=
url=
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o|--output) out=$2; shift 2 ;;
    https://*) url=$1; shift ;;
    *) shift ;;
  esac
done
printf '%s\n' "$url" >> "$FIXTURE/requests"
case "$url" in
  */SHA256SUMS) cp "$FIXTURE/SHA256SUMS" "$out" ;;
  *.tar.gz)
    if [ "${BAD_PACKAGE:-0}" = 1 ]; then printf damaged > "$out"
    else cp "$FIXTURE/archive.tar.gz" "$out"; fi ;;
  *) exit 25 ;;
esac
"#,
    );
    // A decoy on PATH: any GitHub CLI call by the installer becomes visible evidence here.
    script(
        &commands.join("gh"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$FIXTURE/gh-calls\"\nexit 1\n",
    );

    let home = root.join("knowledge");
    fs::create_dir(&home).unwrap();
    fs::write(home.join("customer.txt"), "preserve").unwrap();
    let path = |name: &str| PathBuf::from(root).join(name);
    let install = |prefix: &Path, bad_package: bool, fail_runtime: bool| {
        let output = Command::new("sh")
            .arg("install.sh")
            .arg(prefix)
            .arg(&home)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env(
                "PATH",
                format!("{}:{}", commands.display(), std::env::var("PATH").unwrap()),
            )
            .env("FIXTURE", &fixture)
            .env("BAD_PACKAGE", if bad_package { "1" } else { "0" })
            .env("FAIL_RUNTIME", if fail_runtime { "1" } else { "0" })
            .output()
            .unwrap();
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };

    let prefix = path("application");
    let (status, stderr) = install(&prefix, false, false);
    assert_eq!(
        status,
        Some(0),
        "installer refused a valid release: {stderr}"
    );
    assert!(
        !gh_calls.exists(),
        "installer required GitHub CLI: {}",
        fs::read_to_string(&gh_calls).unwrap_or_default()
    );
    assert!(prefix.join("current/sifttext").exists());
    assert_eq!(
        fs::read_to_string(home.join("customer.txt")).unwrap(),
        "preserve",
        "installation rewrote user knowledge"
    );
    assert_eq!(
        fs::read_to_string(&requests).unwrap().lines().count(),
        2,
        "installer fetched more than the release record and its package"
    );

    let (status, stderr) = install(&prefix, false, false);
    assert_eq!(status, Some(0), "reinstall refused: {stderr}");
    assert_eq!(
        fs::read_to_string(&requests).unwrap().lines().count(),
        2,
        "reinstall re-downloaded the release"
    );

    let broken = path("digest-refused");
    let (status, stderr) = install(&broken, true, false);
    assert_ne!(status, Some(0), "a package failing its digest installed");
    assert!(
        !broken.exists(),
        "digest refusal left a partial application: {stderr}"
    );

    let refused = path("runtime-refused");
    let (status, stderr) = install(&refused, false, true);
    assert_ne!(status, Some(0), "a failing runtime check installed");
    assert!(
        !refused.exists(),
        "runtime-check refusal left a partial application: {stderr}"
    );
}
