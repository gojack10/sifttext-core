use serde_json::json;
use std::{
    fs,
    os::unix::fs::symlink,
    process::{Command, Output, Stdio},
};

const ID: &str = "00000000-0000-4000-8000-000000000001";

#[test]
fn large_unavailable_attachment_receipt_is_checked_before_persisting() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let home = root.join("store");
    let descriptor = root.join("descriptor.json");
    let git_config = root.join("gitconfig");
    fs::write(&git_config, "[user]\nname = Synthetic Fixture\nemail = fixture@example.invalid\n[commit]\ngpgSign = false\n[core]\nhooksPath = /dev/null\n").unwrap();
    let run = |args: &[&str]| -> Output {
        Command::new(env!("CARGO_BIN_EXE_sifttext"))
            .args(args)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("HOME", &root)
            .env("XDG_CONFIG_HOME", root.join(".config"))
            .env("SIFTTEXT_NO_UPDATE", "1")
            .env("GIT_CONFIG_GLOBAL", &git_config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Synthetic Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Synthetic Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };

    let initialized = run(&["init", home.to_str().unwrap(), "--empty"]);
    assert!(initialized.status.success(), "{initialized:?}");
    let node = home.join("nodes").join(ID);
    fs::create_dir_all(node.join("children")).unwrap();
    fs::write(node.join("children.order"), "").unwrap();
    fs::write(
        node.join("node.md"),
        format!("---\nid: \"{ID}\"\nname: \"Root\"\nmicro_summary: \"Root node\"\nstatus: \"in_progress\"\nsee_also: []\n---\nRoot body\n"),
    )
    .unwrap();
    symlink(format!("../nodes/{ID}"), home.join("trees").join(ID)).unwrap();

    let topic = "topic";
    let descriptor_value = json!({
        "schema_version": 1,
        "harness": "pi",
        "session_id": "00000000-0000-4000-8000-000000000002",
        "availability": "unavailable",
        "archive_key": format!("sessions/{}.jsonl", (0..10_000).map(|n| format!("{n:05}")).collect::<String>())
    });
    fs::write(&descriptor, serde_json::to_vec(&descriptor_value).unwrap()).unwrap();
    let attachment_name = format!("pi-{topic}.jsonl");
    let receipt = json!({
        "status": "source_unavailable",
        "node_id": ID,
        "attachment_name": attachment_name,
        "archive_key": descriptor_value["archive_key"]
    });
    let rendered = format!("{}\n", serde_json::to_string_pretty(&receipt).unwrap());
    let tokens = tiktoken_rs::o200k_base_singleton()
        .lock()
        .encode_ordinary(&rendered)
        .len();
    assert!(tokens > 15_000);

    let args = [
        "session-attach",
        home.to_str().unwrap(),
        ID,
        descriptor.to_str().unwrap(),
        &topic,
        root.to_str().unwrap(),
    ];
    let suppressed = run(&args);
    assert_eq!(suppressed.status.code(), Some(1));
    assert_eq!(suppressed.stderr, b"");
    assert_eq!(
        suppressed.stdout,
        format!("[{tokens} tokens suppressed] Re-run with -y to print them, read sql schema if needed.\n").as_bytes()
    );
    assert!(!node.join("session-sources.json").exists());

    let mut forced_args = args.to_vec();
    forced_args.push("-y");
    let forced = run(&forced_args);
    assert_eq!(forced.status.code(), Some(0), "{forced:?}");
    assert_eq!(forced.stderr, b"");
    assert_eq!(forced.stdout, rendered.as_bytes());
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(node.join("session-sources.json")).unwrap()).unwrap();
    assert_eq!(saved.as_array().unwrap().len(), 1);
    assert_eq!(saved[0]["attachment_name"], attachment_name);
}
