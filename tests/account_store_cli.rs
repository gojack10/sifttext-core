use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    process::Command,
};

fn run(home: &std::path::Path, command: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sifttext"))
        .arg(command)
        .env("HOME", home)
        .env("SIFTTEXT_AUTH_ORIGIN", "http://127.0.0.1:1")
        .env("SIFTTEXT_NO_BROWSER", "1")
        .output()
        .unwrap()
}

#[test]
fn git_home_refusal_creates_no_authentication_files() {
    let home = tempfile::tempdir().unwrap();
    fs::create_dir(home.path().join(".git")).unwrap();
    let output = run(home.path(), "login");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Git repository"));
    assert!(!home.path().join(".config").exists());
}

#[test]
fn symlink_credentials_are_not_read_and_logout_only_unlinks_them() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join(".config/sifttext/account");
    fs::create_dir_all(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    let target = home.path().join("knowledge.md");
    fs::write(&target, "preserved knowledge").unwrap();
    let credentials = dir.join("credentials.json");
    symlink(&target, &credentials).unwrap();
    assert!(!run(home.path(), "account").status.success());
    assert!(run(home.path(), "logout").status.success());
    assert!(!credentials.symlink_metadata().is_ok());
    assert_eq!(fs::read_to_string(target).unwrap(), "preserved knowledge");
}

#[test]
fn writable_authentication_ancestors_are_refused() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join(".config");
    fs::create_dir(&config).unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o777)).unwrap();
    let output = run(home.path(), "account");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not writable by others"));
    assert!(!config.join("sifttext").exists());
}
