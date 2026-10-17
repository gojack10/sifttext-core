use sifttext_file_graph::local;
use std::fs;

#[test]
fn session_retention_defaults_off_without_config() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(
        local::session_retention_config(home.path()).unwrap(),
        serde_json::json!({"enabled": false, "archive_root": null})
    );
}

#[test]
fn session_retention_requires_opt_in_and_absolute_archive() {
    let home = tempfile::tempdir().unwrap();
    fs::create_dir_all(home.path().join(".sifttext")).unwrap();
    fs::write(
        home.path().join(".sifttext/config.json"),
        r#"{"session_retention":{"enabled":true,"archive_root":"/private/sessions"}}"#,
    )
    .unwrap();
    assert_eq!(
        local::session_retention_config(home.path()).unwrap(),
        serde_json::json!({"enabled": true, "archive_root": "/private/sessions"})
    );
}

#[test]
fn invalid_enabled_retention_config_is_refused() {
    let home = tempfile::tempdir().unwrap();
    fs::create_dir_all(home.path().join(".sifttext")).unwrap();
    fs::write(
        home.path().join(".sifttext/config.json"),
        r#"{"session_retention":{"enabled":true,"archive_root":"relative"}}"#,
    )
    .unwrap();
    assert!(local::session_retention_config(home.path()).is_err());
}
