use serde_json::json;
use std::{fs, process::Command};

#[test]
fn update_config_cli_emits_lean_settings() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("install");
    let home = temp.path().join("home");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(home.join(".sifttext")).unwrap();
    fs::write(
        root.join("state.json"),
        serde_json::to_vec(&json!({
            "home": home,
            "application": "releases/v1",
            "application_sha256": "a".repeat(64),
            "knowledge": "b".repeat(64),
            "baseline": null,
            "success": {"wall": 0, "uptime": 0, "boot": "test"},
            "attempt": null,
            "pending": null,
            "previous": null
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        home.join(".sifttext/config.json"),
        r#"{"updates":{"mode":"notify","permission":"standing"}}"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_sifttext"))
        .args(["update", "--config", "-y"])
        .env("SIFTTEXT_INSTALL_ROOT", &root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("mode:notify"), "{stdout}");
    assert!(stdout.contains("permission:standing"), "{stdout}");
    assert!(!stdout.starts_with('{'), "{stdout}");
    let shown = sifttext_file_graph::outlines::decode_lean(&stdout).unwrap();
    assert_eq!(shown["path"], home.join(".sifttext/config.json").to_str().unwrap());
    assert!(shown.get("source").is_none());
    assert!(shown.get("pinned").is_none());
    let before = sifttext_file_graph::lean_render::render(&json!({"mode":"notify", "permission":"standing", "pinned":null, "source":"file", "path":home.join(".sifttext/config.json")})).unwrap();
    let count = |s: &str| tiktoken_rs::o200k_base_singleton().lock().encode_ordinary(s).len();
    eprintln!("update --config: {} -> {} tokens", count(&before), count(stdout.trim_end()));
}
