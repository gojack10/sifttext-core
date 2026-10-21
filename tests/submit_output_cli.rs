//! Successful publication stays bounded and recoverable without publishing twice.
use sifttext_file_graph::outlines::decode_lean;
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sifttext"))
        .args(args)
        .env("SIFTTEXT_NO_UPDATE", "1")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn large_home_submit_and_accepted_receipts_stay_bounded_and_retry_without_publication() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().canonicalize().unwrap().join("home");
    let init = cli(&["init", text(&home), "--empty"]);
    assert!(init.status.success(), "{init:?}");

    for i in 0..1_200 {
        let id = format!("50000000-0000-4000-8000-{i:012x}");
        let node = home.join("nodes").join(&id);
        fs::create_dir_all(node.join("children")).unwrap();
        fs::write(node.join("children/.keep"), "").unwrap();
        fs::write(node.join("children.order"), "").unwrap();
        fs::write(
            node.join("node.md"),
            format!("---\nid: {id:?}\nname: \"Root {i}\"\nmicro_summary: \"Synthetic root.\"\nstatus: \"not_examined\"\nsee_also: []\nscope: \"Synthetic test root.\"\n---\nBody\n"),
        )
        .unwrap();
        symlink(format!("../nodes/{id}"), home.join("trees").join(id)).unwrap();
    }
    git(&home, &["add", "nodes", "trees"]);
    git(&home, &["commit", "-qm", "seed synthetic roots"]);
    git(&home, &["branch", "-f", "main", "HEAD"]);
    let base = git(&home, &["rev-parse", "HEAD"]);
    let indexed = cli(&["index", text(&home)]);
    assert!(indexed.status.success(), "{indexed:?}");
    let checked_home = cli(&["check", text(&home), "-y"]);
    assert!(checked_home.status.success(), "{checked_home:?}");

    let allocated = cli(&["worktree", text(&home)]);
    assert!(allocated.status.success(), "{allocated:?}");
    let allocation = decode_lean(std::str::from_utf8(&allocated.stdout).unwrap()).unwrap();
    let draft = PathBuf::from(allocation["worktree"].as_str().unwrap());
    let created = cli(&[
        "create",
        text(&draft),
        "Submitted root",
        "A large receipt test.",
    ]);
    assert!(created.status.success(), "{created:?}");
    let checked_draft = cli(&["check", text(&draft), "-y"]);
    assert!(checked_draft.status.success(), "{checked_draft:?}");
    git(&draft, &["add", "nodes", "trees"]);
    git(&draft, &["commit", "-qm", "knowledge: add submitted root"]);
    let commit = git(&draft, &["rev-parse", "HEAD"]);
    let args = ["submit", text(&home), text(&draft), &commit, &base];

    let saved = cli(&args);
    assert!(saved.status.success(), "{saved:?}");
    assert!(saved.stderr.is_empty(), "{saved:?}");
    let receipt: serde_json::Value = serde_json::from_slice(&saved.stdout).unwrap();
    assert_eq!(receipt["status"], "saved");
    assert_eq!(receipt["home"], text(&home));
    assert_eq!(receipt["worktree_removed"], true);
    assert_eq!(receipt.as_object().unwrap().len(), 5);
    let count = tiktoken_rs::o200k_base_singleton()
        .lock()
        .encode_ordinary(std::str::from_utf8(&saved.stdout).unwrap())
        .len();
    assert!(count < 250, "{count}");
    println!(
        "large-home submit: {} bytes, {count} o200k tokens",
        saved.stdout.len()
    );
    let accepted = git(&home, &["rev-parse", "main"]);
    assert_ne!(accepted, base, "first request should publish");
    assert!(!draft.exists(), "completed draft should be removed");

    let mut retry = args.to_vec();
    retry.push("-y");
    let recovered = cli(&retry);
    assert!(recovered.status.success(), "{recovered:?}");
    assert!(recovered.stderr.is_empty(), "{recovered:?}");
    let receipt: serde_json::Value = serde_json::from_slice(&recovered.stdout).unwrap();
    assert_eq!(receipt["status"], "recovered");
    assert_eq!(receipt["accepted"], accepted);
    assert_eq!(git(&home, &["rev-parse", "main"]), accepted);

    let ready = cli(&["accepted", text(&home)]);
    assert!(ready.status.success(), "{ready:?}");
    let receipt: serde_json::Value = serde_json::from_slice(&ready.stdout).unwrap();
    assert_eq!(receipt["status"], "ready");
    assert_eq!(receipt["accepted"], accepted);
    assert_eq!(receipt["home_advanced"], false);
    assert_eq!(receipt.as_object().unwrap().len(), 5);
    let view = PathBuf::from(receipt["view"].as_str().unwrap());
    assert!(view.is_dir());
    assert_eq!(git(&view, &["rev-parse", "HEAD"]), accepted);
    let count = tiktoken_rs::o200k_base_singleton()
        .lock()
        .encode_ordinary(std::str::from_utf8(&ready.stdout).unwrap())
        .len();
    assert!(count < 250, "{count}");
    println!(
        "large-home accepted: {} bytes, {count} o200k tokens",
        ready.stdout.len()
    );
}
