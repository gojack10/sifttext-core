//! The rendered response, not its row count, bounds a read-only SQL return.
use sifttext_file_graph::Store;
use std::process::{Command, Output, Stdio};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sifttext"))
        .args(args)
        .env("SIFTTEXT_NO_UPDATE", "1")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn tokens(bytes: &[u8]) -> usize {
    tiktoken_rs::o200k_base_singleton()
        .lock()
        .encode_ordinary(std::str::from_utf8(bytes).unwrap())
        .len()
}

#[test]
fn sql_suppresses_only_oversized_returns_and_y_preserves_the_complete_response() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap().join("home");
    let home = home.to_str().unwrap();
    let initialized = cli(&["init", home, "--empty"]);
    assert!(initialized.status.success(), "{initialized:?}");
    // Each appended " x" is one ordinary token; measure the complete JSON wrapper too.
    let mut repeats = 15_000;
    let mut query = format!("SELECT '{}' AS payload", " x".repeat(repeats));
    let measured = cli(&["sql", home, &query, "-y"]);
    assert!(measured.status.success(), "{measured:?}");
    let authored = Store::read(std::path::Path::new(home)).unwrap().inventory;
    repeats -= tokens(&measured.stdout) - 15_000;
    for target in [15_000, 15_001] {
        query = format!("SELECT '{}' AS payload", " x".repeat(repeats));
        let full = cli(&["sql", home, &query, "-y"]);
        assert!(full.status.success(), "{full:?}");
        assert!(full.stderr.is_empty(), "{full:?}");
        assert_eq!(tokens(&full.stdout), target);
        let value: serde_json::Value = serde_json::from_slice(&full.stdout).unwrap();
        assert_eq!(value["result"]["rows"][0][0], " x".repeat(repeats));
        let guarded = cli(&["sql", home, &query]);
        assert!(guarded.stderr.is_empty(), "{guarded:?}");
        if target == 15_000 {
            assert!(guarded.status.success(), "{guarded:?}");
            assert_eq!(guarded.stdout, full.stdout);
        } else {
            assert_eq!(guarded.status.code(), Some(1));
            assert_eq!(
                String::from_utf8(guarded.stdout).unwrap(),
                "[15001 tokens suppressed] Re-run with -y to print them, read sql schema if needed.\n"
            );
        }
        repeats += 1;
    }

    let text = "界🙂".repeat(8_000);
    let query = format!("SELECT '{text}' AS payload");
    let full = cli(&["sql", home, &query, "-y"]);
    assert!(full.status.success(), "{full:?}");
    let value: serde_json::Value = serde_json::from_slice(&full.stdout).unwrap();
    assert_eq!(value["result"]["rows"][0][0], text);
    let count = tokens(&full.stdout);
    assert!(count > 15_000);
    let guarded = cli(&["sql", home, &query]);
    assert_eq!(guarded.status.code(), Some(1));
    assert!(guarded.stderr.is_empty());
    assert_eq!(
        String::from_utf8(guarded.stdout).unwrap(),
        format!(
            "[{count} tokens suppressed] Re-run with -y to print them, read sql schema if needed.\n"
        )
    );

    let refused = cli(&["sql", home, "DELETE FROM nodes", "-y"]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(refused.stdout.is_empty());
    assert!(String::from_utf8_lossy(&refused.stderr).starts_with("rejected:"));
    assert_eq!(
        Store::read(std::path::Path::new(home)).unwrap().inventory,
        authored
    );
}
