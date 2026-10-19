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
    // Each appended " x" is one ordinary token; measure the complete LEAN wrapper too.
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
        let value =
            sifttext_file_graph::outlines::decode_lean(std::str::from_utf8(&full.stdout).unwrap())
                .unwrap();
        assert_eq!(value["column_0"], "payload");
        assert_eq!(value["rows"][0]["c0"], " x".repeat(repeats));
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
    let value =
        sifttext_file_graph::outlines::decode_lean(std::str::from_utf8(&full.stdout).unwrap())
            .unwrap();
    assert_eq!(value["rows"][0]["c0"], text);
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

#[test]
fn sql_lean_keeps_column_positions_storage_classes_and_empty_schema() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap().join("home");
    let home = home.to_str().unwrap();
    assert!(cli(&["init", home, "--empty"]).status.success());
    let query = "SELECT 9223372036854775807 AS \"same\", -9223372036854775808 AS \"same\", \
        1.0 AS \"\", -0.0 AS \"a.b\", 1e999 AS \"Ω\nkey\", NULL AS \"~\", \
        '001' AS \"a:b\", 'Ω\t界\ncarriage\rreturn\\\"' AS \"two words\"";
    let run = |query: &str| {
        let out = cli(&["sql", home, query, "-y"]);
        assert!(out.status.success(), "{out:?}");
        assert!(out.stderr.is_empty());
        sifttext_file_graph::outlines::decode_lean(std::str::from_utf8(&out.stdout).unwrap())
            .unwrap()
    };
    let value = run(query);
    assert_eq!(value["column_count"], 8);
    for (i, name) in ["same", "same", "", "a.b", "Ω\nkey", "~", "a:b", "two words"]
        .iter()
        .enumerate()
    {
        assert_eq!(value[format!("column_{i}")], *name);
    }
    let row = &value["rows"][0];
    assert_eq!(
        row["c0"].as_str().unwrap().parse::<i64>().unwrap(),
        i64::MAX
    );
    assert_eq!(
        row["c1"].as_str().unwrap().parse::<i64>().unwrap(),
        i64::MIN
    );
    assert_eq!(row["c0_type"], "integer");
    assert_eq!(row["c1_type"], "integer");
    assert_eq!(row["c2_type"], "real");
    assert_eq!(row["c2"].as_str().unwrap().parse::<f64>().unwrap(), 1.0);
    assert_eq!(
        row["c3"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(
        row["c4"].as_str().unwrap().parse::<f64>().unwrap(),
        f64::INFINITY
    );
    assert!(row["c5"].is_null());
    assert_eq!(row["c6"], "001");
    assert!(row.get("c6_type").is_none());
    assert_eq!(row["c7"], "Ω\t界\ncarriage\rreturn\\\"");
    let empty = run(&(query.to_owned() + " WHERE 0"));
    assert_eq!(empty["rows"], serde_json::json!([]));
    for i in 0..8 {
        assert_eq!(empty[format!("column_{i}")], value[format!("column_{i}")]);
    }
    let mixed =
        run("SELECT 1 AS x UNION ALL SELECT '1' UNION ALL SELECT NULL UNION ALL SELECT 1.0");
    assert_eq!(
        mixed["rows"],
        serde_json::json!([
            {"c0":"1","c0_type":"integer"}, {"c0":"1"}, {"c0":null}, {"c0":"1","c0_type":"real"}
        ])
    );
    let thousand = format!(
        "SELECT value FROM json_each('{}')",
        serde_json::to_string(&vec![1; 1000]).unwrap()
    );
    assert_eq!(run(&thousand)["rows"].as_array().unwrap().len(), 1000);
    for query in [
        "SELECT 1; SELECT 2".to_owned(),
        "SELECT 1; DELETE FROM nodes".to_owned(),
        "DELETE FROM nodes".to_owned(),
        "PRAGMA user_version".to_owned(),
        "SELECT x'ff'".to_owned(),
        format!(
            "SELECT value FROM json_each('{}')",
            serde_json::to_string(&vec![1; 1001]).unwrap()
        ),
    ] {
        let out = cli(&["sql", home, &query, "-y"]);
        assert!(!out.status.success(), "{query}: {out:?}");
        assert!(out.stdout.is_empty(), "{query}: {out:?}");
    }
}
