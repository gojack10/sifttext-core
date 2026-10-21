//! Reproducible serializer comparison for the audit's deep repeated-ancestor case.
use serde_json::json;
use sifttext_file_graph::{lean_render, search::Outcome};

fn id(n: usize) -> String {
    let hex = sifttext_file_graph::sha256(format!("output-budget-{n}").as_bytes());
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}
fn tokens(text: &str) -> usize {
    tiktoken_rs::o200k_base_singleton().lock().encode_ordinary(text).len()
}
#[test]
fn twenty_deep_hits_do_not_repeat_ancestors_or_warnings() {
    let mut rows = Vec::new();
    for hit in 1..=20 {
        let leaf_depth = 904 - 21 + hit;
        for depth in 0..=leaf_depth {
            let mut row = json!({"hit":hit,"depth":depth,"id":id(depth),
                "name":"Synthetic node","micro_summary":"A short synthetic summary for output budgeting.",
                "status":"in_progress"});
            if depth == leaf_depth {
                row["match"] = json!(true);
                row["path"] = json!(format!("/tmp/example/home/nodes/{}/node.md", id(depth)));
                row["matching_excerpt"] = json!("Synthetic matching passage.");
            }
            rows.push(row);
        }
    }
    assert_eq!(rows.len(), 17_890);
    let old = lean_render::render(&json!({"status":"current","version":"a".repeat(64),
        "version_scope":"authored_store","total_matches":904,"rows":rows})).unwrap() + "\n";
    let mut outcome = Outcome { refusal:None, status:"current", version:Some("a".repeat(64)),
        total_matches:904, rows, hits:vec![], cards:vec![],
        path_template:"/tmp/example/home/nodes/{id}/node.md".into() };
    let value = outcome.cli_value();
    let new = lean_render::render(&value).unwrap() + "\n";
    assert_eq!(value["total_matches"], 904);
    assert_eq!(value["rows"].as_array().unwrap().len(), 20);
    for (i, row) in value["rows"].as_array().unwrap().iter().enumerate() {
        assert_eq!(row["id"], id(884 + i));
        assert_eq!(row["parent"], id(883 + i));
        assert_eq!(row["root"], id(0));
        assert_eq!(row["name"], "Synthetic node");
        assert_eq!(row["micro_summary"], "A short synthetic summary for output budgeting.");
    }
    println!("search deep N=904 hits=20 old_rows=17890 new_rows=20 old_bytes={} old_tokens={} new_bytes={} new_tokens={}", old.len(), tokens(&old), new.len(), tokens(&new));
    assert!(tokens(&new) * 100 < tokens(&old));
    // Applicable ancestor warnings are emitted once, despite twenty shared chains.
    for row in &mut outcome.rows {
        if row["depth"] == 0 { row["warnings"] = json!("Exact warning Ω\nRead the source."); }
    }
    let warnings = outcome.cli_value();
    assert_eq!(warnings[format!("warnings_{}", id(0))], "Exact warning Ω\nRead the source.");
    assert_eq!(warnings.as_object().unwrap().keys().filter(|key| key.starts_with("warnings_")).count(), 1);
    // An excerpt that only repeats the displayed summary adds no query-specific evidence.
    let hit = outcome.rows.iter_mut().find(|row| row["match"] == true).unwrap();
    hit["matching_excerpt"] = hit["micro_summary"].clone();
    assert!(outcome.cli_value()["rows"][0].get("matching_excerpt").is_none());
}
