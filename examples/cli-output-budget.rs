//! Safe serializer samples; never opens a store, writes files or contacts services.
//! sh scripts/with-tools.sh cargo run --locked --example cli-output-budget [OUTPUT_FILE ...]
//! Optional files are counted as exact bytes; samples are not worst-case bounds.
use serde_json::{Value, json};
use sifttext_file_graph::{
    candidate_validation::{GraphCheck, Report, Row},
    checked_publication, lean_render,
    search::Outcome,
    status::{Acceptance, Change, Status},
};
use std::{collections::BTreeMap, fs};

fn id(n: usize) -> String {
    let h = sifttext_file_graph::sha256(format!("output-budget-{n}").as_bytes());
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}
fn tokens(text: &str) -> usize {
    tiktoken_rs::o200k_base_singleton()
        .lock()
        .encode_ordinary(text)
        .len()
}
fn count(label: &str, text: &str) -> usize {
    let n = tokens(text);
    println!("{label}\t{}\t{n}", text.len());
    n
}
fn lean(value: &Value) -> String {
    lean_render::render(value).unwrap() + "\n"
}
fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap() + "\n"
}
fn report(n: usize) -> Report {
    Report {
        status: "current",
        version: Some(sifttext_file_graph::sha256(b"output-budget")),
        before_version: None,
        nodes: n,
        rows: (0..n)
            .map(|i| Row {
                id: Some(id(i)),
                check: "content_tokens",
                rule: "302b4549-ef27-4ab7-a9a7-7f240ffe058e",
                tokens: Some(5000),
                source: None,
                target: None,
                law: None,
            })
            .collect(),
        graph: GraphCheck {
            mode: "full_law",
            reason: "sample",
            base_version: None,
            candidate_version: None,
        },
        failure: None,
        problem: None,
    }
}
fn main() {
    println!("sample\tbytes\to200k_tokens");
    for n in [1, 100, 904] {
        let mut r = report(n);
        count(
            &format!("check rich diagnostic N={n}"),
            &(r.lean().unwrap() + "\n"),
        );
        count(&format!("check CLI N={n}"), &(r.cli_lean().unwrap() + "\n"));
        assert!(!r.cli_lean().unwrap().contains("content_tokens"));
        let mut receipt = json!({"status":"current", "check":"accepted_checked_revision",
            "candidate":sifttext_file_graph::sha256(b"commit")[..40], "version":r.version,
            "view":"/tmp/example/checked-view", "validation":r,
            "published":false, "tree":"internal", "elapsed_ms":100});
        count(&format!("accepted rich sample N={n}"), &pretty(&receipt));
        count(
            &format!("accepted CLI N={n}"),
            &(checked_publication::cli_json(&receipt).unwrap() + "\n"),
        );
        receipt["home"] = json!("/tmp/example/home");
        receipt["worktree_removed"] = json!(true);
        receipt["check"] = json!("published_checked_candidate");
        let text = checked_publication::cli_json(&receipt).unwrap() + "\n";
        assert!(!text.contains("validation"));
        count(&format!("submit CLI N={n}"), &text);
        r.status = "rejected";
        r.rows.push(Row {
            id: Some(id(0)),
            check: "content_over_limit",
            rule: "302b4549-ef27-4ab7-a9a7-7f240ffe058e",
            tokens: Some(5001),
            source: None,
            target: None,
            law: None,
        });
        count(
            &format!("check single real failure N={n}"),
            &format!("rejected: {}\n", r.cli_lean().unwrap()),
        );
        for changed in [0, 1, n] {
            let a = Acceptance {
                status: "current",
                normalized_ids: vec![],
                reopened_ids: vec![],
                nodes: BTreeMap::new(),
                changes: (0..changed)
                    .map(|i| Change {
                        id: id(i),
                        from: Status::NotExamined,
                        to: Status::InProgress,
                        removed_reason_fields: vec![],
                    })
                    .collect(),
            };
            let text = pretty(&a.cli_json());
            if changed == 0 {
                assert_eq!(a.cli_json(), json!({"status":"unchanged"}));
            }
            count(
                &format!("normalize CLI changes={changed} home_nodes={n}"),
                &text,
            );
        }
    }
    // Same twenty deepest hits: old presentation repeats each root-to-hit chain.
    let mut rows = Vec::new();
    for hit in 1..=20 {
        let leaf = 904 - 21 + hit;
        for depth in 0..=leaf {
            let mut row = json!({"hit":hit,"depth":depth,"id":id(depth),"name":"Synthetic node",
                "status":"in_progress","micro_summary":"A short synthetic summary for output budgeting."});
            if depth == leaf {
                row["match"] = json!(true);
                row["path"] = json!(format!("/tmp/example/home/nodes/{}/node.md", id(depth)));
                row["matching_excerpt"] = json!("Synthetic matching passage.");
            }
            rows.push(row);
        }
    }
    assert_eq!(rows.len(), 17_890);
    let out = Outcome {
        refusal: None,
        status: "current",
        version: report(0).version,
        total_matches: 904,
        path_template: "/tmp/example/home/nodes/{id}/node.md".into(),
        hits: vec![],
        cards: vec![],
        rows,
    };
    let old = lean(&json!({"status":out.status,"version":out.version,
        "version_scope":"authored_store","total_matches":out.total_matches,"rows":out.rows}));
    let new_value = out.cli_value();
    assert_eq!(new_value["rows"].as_array().unwrap().len(), 20);
    let new = lean(&new_value);
    assert!(tokens(&new) < tokens(&old));
    count("search legacy chains same 20 hits N=904", &old);
    count("search CLI same 20 hits N=904", &new);
    // Actual SQL and outline checks use temporary stores in sql_output_cli,
    // outline_output_cli and retrieval_output_budget; don't duplicate their projections here.
    for path in std::env::args().skip(1) {
        let text = fs::read_to_string(&path).expect("read UTF-8 output file");
        count(&format!("file {path}"), &text);
    }
}
