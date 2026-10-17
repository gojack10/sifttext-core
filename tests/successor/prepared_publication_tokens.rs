use super::{Fixture, commit, git, id, node, precas::fingerprint};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

fn call(binary: &str, request: Value) -> Output {
    let bins = crate::native_bin_dir();
    let mut child = Command::new(bins.join(binary))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("build checked native binaries before testing");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(request.to_string().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn current(binary: &str, request: Value) -> Value {
    let out = call(binary, request);
    assert!(out.status.success(), "{out:?}");
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn native_submission_enforces_body_token_boundary_despite_forged_sidecar() {
    let f = Fixture::new();
    // Independent pinned o200k fixture: N repetitions of " a" encode as N tokens.
    // Check the actual submit boundary, not merely a standalone candidate validator.
    let body = " a".repeat(5000);
    node(&f.a, 2, &body);
    let submitted = commit(&f.a);
    let before = fingerprint(&f.a);
    let b_before = fingerprint(&f.b);
    let reader_before = fingerprint(&f.repo);
    let accepted = current("git-submit-observe", f.request(&f.a));
    assert_eq!(accepted["status"], "current");
    assert_eq!(accepted["published"], true);
    assert_eq!(accepted["draft_commit"], submitted);
    assert!(
        accepted["validation"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == id(2) && row["tokens"] == 5000)
    );
    let view = Path::new(accepted["view"].as_str().unwrap());
    let tip = git(&f.repo, &["rev-parse", "main"]);
    assert_eq!(accepted["candidate"], tip);
    assert_eq!(git(view, &["rev-parse", "HEAD"]), tip);
    assert_eq!(accepted["tree"], git(view, &["rev-parse", "HEAD^{tree}"]));
    let indexed = current(
        "core-observe",
        json!({"op":"index","store":view,"mode":"incremental"}),
    );
    assert_eq!(indexed["status"], "current");
    assert_eq!(indexed["version"], accepted["version"]);
    assert_eq!(indexed["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(
        indexed["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == id(2))
            .unwrap()["content"],
        body
    );
    assert_eq!(
        current("core-observe", json!({"op":"outline_inspect","store":view}))["version"],
        accepted["version"]
    );
    assert_eq!(fingerprint(&f.a), before);
    assert_eq!(fingerprint(&f.b), b_before);
    assert_eq!(fingerprint(&f.repo), reader_before);

    node(&f.a, 2, &" a".repeat(5001));
    commit(&f.a);
    fs::write(f.a.join(format!("nodes/{}/node.meta.json", id(2))), "{}").unwrap();
    let over_before = fingerprint(&f.a);
    let out = call("git-submit-observe", f.request(&f.a));
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let text = std::str::from_utf8(&out.stdout).unwrap();
    assert!(text.lines().any(|line| line == "status:rejected"), "{text}");
    assert!(text.lines().any(|line| line == "ceiling:5000"), "{text}");
    // Fixed scalar-only pinned LEAN schema; exact row checks rule identity and count.
    assert!(
        text.lines().any(|line| line.starts_with("rows[")
            && line.ends_with(":check\tid\tlaw\trule\tsource\ttarget\ttokens")),
        "{text}"
    );
    assert!(text.lines().any(|line| line == format!("  crystallization_over_limit\t{}\t_\t302b4549-ef27-4ab7-a9a7-7f240ffe058e\t_\t_\t5001", id(2))), "{text}");
    super::submission_parity::assert_rule_rows(text);
    assert!(
        !text.contains(f._dir.path().to_str().unwrap()),
        "private fixture path: {text}"
    );
    assert!(!text.contains(" a a a"), "authored body leaked: {text}");
    assert_eq!(git(&f.repo, &["rev-parse", "main"]), tip);
    assert_eq!(fingerprint(&f.a), over_before);
    assert_eq!(fingerprint(&f.b), b_before);
    assert_eq!(fingerprint(&f.repo), reader_before);
    let recovered = current(
        "git-submit-observe",
        json!({"repo":f.repo,"accepted_ref":"refs/heads/main"}),
    );
    assert_eq!(recovered["status"], "current");
    assert_eq!(recovered["candidate"], tip);
    assert_eq!(recovered["version"], accepted["version"]);
}
