use std::{
    fs,
    os::unix::fs::symlink,
    process::{Command, Output, Stdio},
};

const ROOT: &str = "00000000-0000-4000-8000-000000000001";
const CHILD: &str = "00000000-0000-4000-8000-000000000002";

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sifttext"))
        .args(args)
        .env("SIFTTEXT_NO_UPDATE", "1")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn full_outline_suppresses_large_returns_and_y_prints_exact_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap().join("home");
    let home_text = home.to_str().unwrap();
    let initialized = cli(&["init", home_text, "--empty"]);
    assert!(initialized.status.success(), "{initialized:?}");

    let root = home.join("nodes").join(ROOT);
    let child = home.join("nodes").join(CHILD);
    fs::create_dir_all(root.join("children")).unwrap();
    fs::create_dir_all(child.join("children")).unwrap();
    fs::write(root.join("children.order"), format!("{CHILD}\n")).unwrap();
    fs::write(child.join("children.order"), "").unwrap();
    fs::write(
        root.join("node.md"),
        format!("---\nid: \"{ROOT}\"\nname: \"Root\"\nmicro_summary: \"Synthetic root\"\nstatus: \"not_examined\"\nsee_also: []\n---\nRoot.\n"),
    )
    .unwrap();
    fs::write(
        child.join("node.md"),
        format!("---\nid: \"{CHILD}\"\nname: \"Large child\"\nmicro_summary: \"{}\"\nstatus: \"not_examined\"\nsee_also: []\n---\nChild.\n", " ordinary".repeat(20_000)),
    )
    .unwrap();
    symlink(format!("../../{CHILD}"), root.join("children").join(CHILD)).unwrap();
    symlink(format!("../nodes/{ROOT}"), home.join("trees").join(ROOT)).unwrap();

    let full = cli(&["outline", home_text, ROOT, "full", "-y"]);
    assert!(full.status.success(), "{full:?}");
    assert!(full.stderr.is_empty(), "{full:?}");
    let tokens = tiktoken_rs::o200k_base_singleton()
        .lock()
        .encode_ordinary(std::str::from_utf8(&full.stdout).unwrap())
        .len();
    assert!(tokens > 15_000, "expected oversized outline, got {tokens}");

    let guarded = cli(&["outline", home_text, ROOT, "full"]);
    assert_eq!(guarded.status.code(), Some(1), "{guarded:?}");
    assert!(guarded.stderr.is_empty(), "{guarded:?}");
    assert_eq!(
        guarded.stdout,
        format!("[{tokens} tokens suppressed] Re-run with -y to print them, read sql schema if needed.\n").as_bytes()
    );
}
