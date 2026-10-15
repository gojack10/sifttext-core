// Remaining dogfood_local.mjs outcome and re-sealed freshness controls.
fn dogfood_outcome(
    status: std::process::ExitStatus,
    bytes: &[u8],
    expected: &str,
) -> Result<Value, String> {
    let code = status.code().ok_or("native operation was signaled")?;
    if !(0..=255).contains(&code) || (code == 0) != (expected == "current") {
        return Err("unexpected native exit code".into());
    }
    let v: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if v["status"] != expected {
        return Err("unexpected native status".into());
    }
    Ok(v)
}
fn dogfood_empty_unavailable(v: &Value, op: &str) {
    assert_eq!(v.get("version"), Some(&Value::Null));
    match op {
        "index" => {
            assert_eq!(v["nodes"], json!([]));
            assert_eq!(v["links"], json!([]));
        }
        "outline_inspect" => assert_eq!(v["files"], json!([])),
        "search" => {
            assert_eq!(v["results"], json!([]));
            assert_eq!(v["total_matches"], 0);
            assert_eq!(v["other_groups"], json!([]));
        }
        _ => panic!("unsupported unavailable operation"),
    }
}
#[test]
fn dogfood_outcome_validator_rejects_false_success_and_invalid_process_results() {
    use std::os::unix::process::ExitStatusExt;
    let exited = |code| std::process::ExitStatus::from_raw(code << 8);
    assert!(dogfood_outcome(exited(1), br#"{"status":"error"}"#, "error").is_ok());
    assert!(dogfood_outcome(exited(0), br#"{"status":"current"}"#, "current").is_ok());
    for (status, bytes, expected) in [
        (exited(0), br#"{"status":"error"}"#.as_slice(), "error"),
        (exited(1), br#"{"status":"current"}"#.as_slice(), "error"),
        (exited(1), br#"{"status":"unknown"}"#.as_slice(), "error"),
        (exited(1), b"not JSON".as_slice(), "error"),
        (
            std::process::ExitStatus::from_raw(15),
            br#"{"status":"error"}"#.as_slice(),
            "error",
        ),
        (exited(1), br#"{"status":"current"}"#.as_slice(), "current"),
    ] {
        assert!(dogfood_outcome(status, bytes, expected).is_err());
    }
}
#[test]
fn dogfood_resealed_freshness_and_unavailable_search_preserve_authored_bytes() {
    let dir = TempDir::new().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let store = root.join("store");
    let forged = root.join("resealed-identity");
    let nodes = dogfood_nodes();
    materialize(&store, &nodes);
    let before = authored_snapshot(&store);
    let bpe = ORACLE_BPE.clone();
    let version = refresh(&store, "full");
    check_native(&store, &version, &bpe);
    assert!(versions(&store, "planted-global-v9").is_err());
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(&store)
            .arg(&forged)
            .status()
            .unwrap()
            .success()
    );
    let owner = nodes[0]["id"].as_str().unwrap();
    reseal_file_identity(&forged, owner, &bpe);
    let stale = dogfood_expected_request(&forged, json!({"op":"outline_inspect"}), "stale");
    dogfood_empty_unavailable(&stale, "outline_inspect");
    assert!(versions(&forged, &version).is_err());
    assert_eq!(authored_records(&forged), authored_records(&store));
    assert_eq!(authored_snapshot(&forged), before);
    assert_eq!(authored_snapshot(&store), before);
    // Consumer-specific unavailable search must neither return old hits nor repair input.
    change_field(
        &store,
        nodes[1]["id"].as_str().unwrap(),
        "micro_summary",
        json!("   "),
    );
    let invalid = authored_snapshot(&store);
    let all = nodes.iter().map(|n| n["id"].clone()).collect::<Vec<_>>();
    for request in [
        json!({"op":"index","mode":"full"}),
        json!({"op":"outline_inspect"}),
        json!({"op":"search","query":"alpha","under_node_id":null,
            "limit":6,"offset":0,"readable_ids":all}),
    ] {
        let op = request["op"].as_str().unwrap();
        let v = dogfood_expected_request(&store, request.clone(), "error");
        dogfood_empty_unavailable(&v, op);
        assert_eq!(
            authored_snapshot(&store),
            invalid,
            "refusal repaired authored bytes"
        );
    }
    eprintln!("DOGFOOD_RUST_RESEALED_AND_UNAVAILABLE_CHECKED");
}
