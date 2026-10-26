const LOGO: &str = "░▒▓█████████████████\n\n    ░▒▓█████████████\n\n        ░▒▓█████████\n\n            ░▒▓█████\n\n                ░▒▓█\n";

//! Rust replacement for the retired Node release-bootstrap check. Exercises the public
//! agent-facing `install.sh` release path only: the official release record, the published
//! digest binding, and the refusals that must leave no partial application behind.
//! Build checked assets once with scripts/build.sh, then run
//! `cargo test --locked --test release_bootstrap`.
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
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

    let shell_home = root.join("shell-home");
    fs::create_dir(&shell_home).unwrap();
    let home = root.join("knowledge");
    fs::create_dir(&home).unwrap();
    fs::write(home.join("customer.txt"), "preserve").unwrap();
    let path = |name: &str| PathBuf::from(root).join(name);
    let install = |prefix: &Path, bad_package: bool, fail_runtime: bool, shell: &str| {
        let output = Command::new("sh")
            .arg("install.sh")
            .arg(prefix)
            .arg(&home)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", commands.display()),
            )
            .env("HOME", &shell_home)
            .env("SHELL", shell)
            .env("FIXTURE", &fixture)
            .env("BAD_PACKAGE", if bad_package { "1" } else { "0" })
            .env("FAIL_RUNTIME", if fail_runtime { "1" } else { "0" })
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        if output.status.success() {
            assert_eq!(
                stdout,
                format!(
                    "{}Login with sifttext login.\nTell your agent to run sifttext instructions.\n",
                    LOGO
                )
            );
        } else {
            assert!(stdout.is_empty(), "failure printed success output: {stdout}");
        }
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };

    let prefix = path("application");
    let (status, stderr) = install(&prefix, false, false, "/bin/sh");
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
    assert!(
        Command::new("sh")
            .arg("-c")
            .arg(". \"$HOME/.profile\"; sifttext runtime-check")
            .env("HOME", &shell_home)
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
            .status()
            .unwrap()
            .success(),
        "new shell cannot find sifttext"
    );
    let profile = fs::read_to_string(shell_home.join(".profile")).unwrap();
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

    let (status, stderr) = install(&prefix, false, false, "/bin/sh");
    assert_eq!(status, Some(0), "reinstall refused: {stderr}");
    assert_eq!(
        fs::read_to_string(&requests).unwrap().lines().count(),
        2,
        "reinstall re-downloaded the release"
    );

    assert_eq!(
        fs::read_to_string(shell_home.join(".profile")).unwrap(),
        profile,
        "reinstall duplicated PATH setup"
    );
    // Remove only our command to let distinct application destinations exercise failure gates.
    fs::remove_file(shell_home.join(".local/bin/sifttext")).unwrap();
    let broken = path("digest-refused");
    let (status, stderr) = install(&broken, true, false, "/bin/sh");
    assert_ne!(status, Some(0), "a package failing its digest installed");
    assert!(
        !broken.exists(),
        "digest refusal left a partial application: {stderr}"
    );

    let refused = path("runtime-refused");
    let (status, stderr) = install(&refused, false, true, "/bin/sh");
    assert_ne!(status, Some(0), "a failing runtime check installed");
    assert!(
        !refused.exists(),
        "runtime-check refusal left a partial application: {stderr}"
    );
    for (shell, profile) in [("/bin/bash", ".bash_profile"), ("/bin/zsh", ".zshrc")] {
        fs::write(shell_home.join(profile), "# preserve shell configuration\n").unwrap();
        let shell_prefix = path(profile);
        let (status, stderr) = install(&shell_prefix, false, false, shell);
        assert_eq!(status, Some(0), "shell setup failed: {stderr}");
        let config = fs::read_to_string(shell_home.join(profile)).unwrap();
        assert!(config.starts_with("# preserve shell configuration\n"));
        assert!(config.contains("export PATH=\"$HOME/.local/bin:$PATH\""));
        fs::remove_file(shell_home.join(".local/bin/sifttext")).unwrap();
    }
    assert!(fs::read_to_string(shell_home.join(".bashrc")).unwrap()
        .contains("export PATH=\"$HOME/.local/bin:$PATH\""));
    let decoy = commands.join("sifttext");
    script(&decoy, "#!/bin/sh\nexit 42\n");
    let (status, _) = install(&path("path-collision"), false, false, "/bin/sh");
    assert_ne!(status, Some(0));
    assert!(!path("path-collision").exists());
    assert_eq!(fs::read_to_string(&decoy).unwrap(), "#!/bin/sh\nexit 42\n");
    fs::remove_file(decoy).unwrap();
    let command = shell_home.join(".local/bin/sifttext");
    script(&command, "#!/bin/sh\nprintf unrelated\\n\n");
    let collision = path("collision-refused");
    let (status, _) = install(&collision, false, false, "/bin/sh");
    assert_ne!(status, Some(0));
    assert!(!collision.exists());
    assert_eq!(
        fs::read_to_string(command).unwrap(),
        "#!/bin/sh\nprintf unrelated\\n\n"
    );
    assert_eq!(
        fs::read_to_string(home.join("customer.txt")).unwrap(),
        "preserve"
    );
}

#[test]
fn installation_guide_pins_latest_and_refuses_invalid_release_headers() {
    let guide = include_str!("../INSTALL.md");
    let snippet = guide.split_once("```sh\nreview=").unwrap().1
        .split_once("\n```").unwrap().0;
    let root = tempfile::tempdir().unwrap();
    let commands = root.path().join("commands");
    fs::create_dir(&commands).unwrap();
    script(&commands.join("curl"), r#"#!/bin/sh
set -eu
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) out=$2; shift 2 ;;
    https://*) url=$1; shift ;;
    *) shift ;;
  esac
done
case "$url" in
  */releases/latest/download/SHA256SUMS) cp "$FIXTURE/header" "$out" ;;
  */releases/download/v9.8.7/install.sh) touch "$FIXTURE/downloaded" "$out" ;;
  *) exit 1 ;;
esac
"#);
    let revision = "a".repeat(40);
    for (header, valid) in [
        (format!("# SiftText release v9.8.7 source {revision}\n"), true),
        (String::new(), false),
        (format!("# Other release v9.8.7 source {revision}\n"), false),
        (format!("# SiftText release ../../escape source {revision}\n"), false),
        ("# SiftText release v9.8.7 source short\n".into(), false),
        (format!("# SiftText release v9.8.7 source {}\n", "g".repeat(40)), false),
    ] {
        fs::write(root.path().join("header"), header).unwrap();
        let _ = fs::remove_file(root.path().join("downloaded"));
        let output = Command::new("sh").args(["-eu", "-c"])
            .arg(format!("review={snippet}\n[ \"$release\" = v9.8.7 ]\n[ \"$revision\" = {revision} ]"))
            .env("PATH", format!("{}:/usr/bin:/bin", commands.display()))
            .env("FIXTURE", root.path())
            .env("TMPDIR", root.path())
            .output().unwrap();
        assert_eq!(output.status.success(), valid, "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(root.path().join("downloaded").exists(), valid);
    }
    assert!(guide.contains("SIFTTEXT_RELEASE=\"$release\" sh \"$review/install.sh\""));
    assert!(guide.contains("sifttext-core/$revision\""));
}

#[test]
fn banner_colors_only_commands_on_a_terminal_and_respects_no_color() {
    let root = tempfile::tempdir().unwrap();
    let installer = include_str!("../install.sh");
    let banner = installer
        .split_once("success() {\n")
        .unwrap().1
        .split_once("\n}\n")
        .unwrap().0;
    let path = root.path().join("banner.sh");
    script(&path, &format!("#!/bin/sh\n{banner}\n"));
    for (term, no_color, colored) in [
        ("xterm-256color", None, true),
        ("xterm", Some(""), false),
        ("xterm", Some("1"), false),
        ("dumb", None, false),
    ] {
        let mut command = Command::new("script");
        if cfg!(target_os = "macos") {
            command.args(["-q", "/dev/null", "sh"]).arg(&path);
        } else {
            command.args(["-q", "-c"])
                .arg(format!("sh {}", path.display()))
                .arg("/dev/null");
        }
        command.env("TERM", term).env_remove("NO_COLOR");
        if let Some(value) = no_color {
            command.env("NO_COLOR", value);
        }
        // Keep stdin open until the command exits: macOS script otherwise echoes an EOF (^D).
        let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let output = child.wait_with_output().unwrap();
        drop(stdin);
        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap().replace("\r\n", "\n");
        let (start, end) = if colored {
            ("\u{1b}[1;36m", "\u{1b}[0m")
        } else {
            ("", "")
        };
        assert_eq!(
            stdout,
            format!(
                "{}Login with {start}sifttext login{end}.\nTell your agent to run {start}sifttext instructions{end}.\n",
                LOGO
            )
        );
    }
}
