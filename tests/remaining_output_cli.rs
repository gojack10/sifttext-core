//! Small custody receipts must not grow into transcript/configuration dumps.
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

#[test]
fn session_receipts_keep_outcome_and_commit_without_constants() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let home = root.join("home");
    let archive = root.join("archive");
    let remote = root.join("remote");
    let identity = root.join("gitconfig");
    fs::write(&identity, "[user]\nname = Synthetic Fixture\nemail = fixture@example.invalid\n[commit]\ngpgSign = false\n[core]\nhooksPath = /dev/null\n").unwrap();
    let cli = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sifttext"))
            .args(args)
            .env("GIT_CONFIG_GLOBAL", &identity)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("SIFTTEXT_NO_UPDATE", "1")
            .env_remove("SIFTTEXT_INSTALL_ROOT")
            .output().unwrap()
    };
    let initialized = cli(&["init", home.to_str().unwrap(), "--empty"]);
    assert!(initialized.status.success(), "{initialized:?}");
    let config = cli(&["session-retention-config", home.to_str().unwrap()]);
    assert!(config.status.success(), "{config:?}");
    assert_eq!(serde_json::from_slice::<Value>(&config.stdout).unwrap(), json!({
        "enabled":false,"archive_root":null,"backup_enabled":false,"backup_configured":false
    }));
    let descriptor = root.join("descriptor.json");
    fs::write(&descriptor, br#"{"availability":"unavailable"}"#).unwrap();
    let captured = cli(&["session-capture", root.join("absent.jsonl").to_str().unwrap(), descriptor.to_str().unwrap(), archive.to_str().unwrap()]);
    assert!(captured.status.success(), "{captured:?}");
    assert_eq!(serde_json::from_slice::<Value>(&captured.stdout).unwrap(), json!({"status":"source_unavailable"}));
    assert!(!archive.exists());
    let git = |path: &Path, args: &[&str]| {
        let output = Command::new("git").arg("-C").arg(path).args(args)
            .env("GIT_CONFIG_GLOBAL", &identity).env("GIT_CONFIG_NOSYSTEM", "1")
            .output().unwrap();
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    fs::create_dir(&archive).unwrap();
    fs::create_dir(&remote).unwrap();
    git(&archive, &["init", "--quiet", "-b", "main"]);
    fs::write(archive.join("session.jsonl"), "private synthetic session\n").unwrap();
    git(&archive, &["add", "session.jsonl"]);
    git(&archive, &["commit", "--quiet", "-m", "session: synthetic fixture"]);
    git(&remote, &["init", "--bare", "--quiet", "-b", "main"]);
    let refused = cli(&["session-backup", home.to_str().unwrap()]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("not enabled"));
    fs::create_dir_all(home.join(".sifttext")).unwrap();
    fs::write(home.join(".sifttext/config.json"), json!({"session_retention":{
        "enabled":true,"archive_root":archive,"backup_enabled":true,"backup_remote":remote
    }}).to_string()).unwrap();
    let backup = cli(&["session-backup", home.to_str().unwrap()]);
    assert!(backup.status.success(), "{backup:?}");
    let receipt: Value = serde_json::from_slice(&backup.stdout).unwrap();
    assert_eq!(receipt, json!({"status":"backed_up","commit":git(&archive, &["rev-parse", "HEAD"])}));
    assert_eq!(receipt["commit"], git(&remote, &["rev-parse", "refs/heads/main"]));
    let count = |s: &str| tiktoken_rs::o200k_base_singleton().lock().encode_ordinary(s).len();
    let before = json!({"status":"backed_up","commit":receipt["commit"],"destination_configured":true}).to_string();
    eprintln!("session-backup: {} -> {} tokens", count(&before), count(std::str::from_utf8(&backup.stdout).unwrap().trim_end()));
    let before = serde_json::to_string_pretty(&json!({"status":"source_unavailable","commit":null})).unwrap();
    eprintln!("session-capture unavailable: {} -> {} tokens", count(&before), count(std::str::from_utf8(&captured.stdout).unwrap().trim_end()));
    assert!(!String::from_utf8_lossy(&backup.stdout).contains("private synthetic session"));
}
