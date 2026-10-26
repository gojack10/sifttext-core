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
                "[15001 tokens suppressed] Re-run this sifttext sql command with -y to print them.\n"
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
            "[{count} tokens suppressed] Re-run this sifttext sql command with -y to print them.\n"
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
fn sql_bounds_native_allocations_and_aggregate_escaped_output_even_with_y() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap().join("home");
    let home = home.to_str().unwrap();
    assert!(cli(&["init", home, "--empty"]).status.success());
    for (query, reason) in [
        ("SELECT hex(zeroblob(5 * 1024 * 1024))", "too big"),
        ("SELECT printf('%.*c', 5 * 1024 * 1024, 'x')", "4 MiB"),
        ("SELECT printf('%.*c', 3 * 1024 * 1024, 'x') UNION ALL SELECT printf('%.*c', 3 * 1024 * 1024, 'x')", "4 MiB"),
        ("SELECT replace(hex(zeroblob(1024 * 1024)), '00', char(1))", "4 MiB"),
    ] {
        let out = cli(&["sql", home, query, "-y"]);
        assert!(!out.status.success(), "{query}: {out:?}");
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).contains(reason), "{query}: {out:?}");
    }
    assert!(cli(&["sql", home, "SELECT 1", "-y"]).status.success());
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
    assert!(value.get("column_count").is_none());
    assert!(value.get("version_scope").is_none());
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
    assert_eq!(value["c0_type"], "integer");
    assert_eq!(value["c1_type"], "integer");
    assert_eq!(value["c2_type"], "real");
    assert_eq!(value["c3_type"], "real");
    assert_eq!(value["c4_type"], "real");
    assert!(row.get("c0_type").is_none());
    assert!(row.get("c2_type").is_none());
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
    let homogeneous = run(&thousand);
    assert_eq!(homogeneous["c0_type"], "integer");
    assert_eq!(homogeneous["rows"].as_array().unwrap().len(), 1000);
    let mut old = homogeneous.clone();
    old["version_scope"] = serde_json::json!("authored_store");
    old["column_count"] = serde_json::json!(1);
    old.as_object_mut().unwrap().remove("c0_type");
    for row in old["rows"].as_array_mut().unwrap() { row["c0_type"] = serde_json::json!("integer"); }
    let old = sifttext_file_graph::lean_render::render(&old).unwrap() + "\n";
    let new = sifttext_file_graph::lean_render::render(&homogeneous).unwrap() + "\n";
    println!("sql homogeneous rows=1000 old_bytes={} old_tokens={} new_bytes={} new_tokens={}", old.len(), tokens(old.as_bytes()), new.len(), tokens(new.as_bytes()));
    assert!(homogeneous["rows"].as_array().unwrap().iter().all(|row| row.get("c0_type").is_none()));
    let nullable = run("SELECT -0.0 AS x UNION ALL SELECT NULL UNION ALL SELECT 1e-300");
    assert_eq!(nullable["c0_type"], "real");
    assert_eq!(nullable["rows"][1]["c0"], serde_json::Value::Null);
    assert_eq!(nullable["rows"][0]["c0"].as_str().unwrap().parse::<f64>().unwrap().to_bits(), (-0.0f64).to_bits());
    assert_eq!(nullable["rows"][2]["c0"].as_str().unwrap().parse::<f64>().unwrap(), 1e-300);
    assert!(run("SELECT NULL AS x").get("c0_type").is_none());
    assert!(empty.get("c0_type").is_none());
    let index = cli(&["index", home]);
    assert!(index.status.success(), "{index:?}");
    let index = sifttext_file_graph::outlines::decode_lean(std::str::from_utf8(&index.stdout).unwrap()).unwrap();
    assert_eq!(index.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(), ["nodes", "status", "version"]);
    let mut old = index.clone();
    old["version_scope"] = serde_json::json!("authored_store");
    old["links"] = serde_json::json!(0);
    let old = sifttext_file_graph::lean_render::render(&old).unwrap() + "\n";
    let new = sifttext_file_graph::lean_render::render(&index).unwrap() + "\n";
    println!("index empty old_bytes={} old_tokens={} new_bytes={} new_tokens={}", old.len(), tokens(old.as_bytes()), new.len(), tokens(new.as_bytes()));
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
