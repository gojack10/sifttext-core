//! Safe output-size samples: no stores, Git changes, network, login or updates.
//! Run: sh scripts/with-tools.sh cargo run --locked --example cli-output-budget
//! These measure serializer shapes, not reachable global maxima or end-to-end commands.
use serde_json::{Value, json};
use sifttext_file_graph::{
    candidate_validation::{GraphCheck, Report, Row},
    checked_publication, lean_render, outline_artifacts,
};

fn id(n: usize) -> String {
    let hex = sifttext_file_graph::sha256(format!("output-budget-{n}").as_bytes());
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn count(label: &str, output: &str) {
    let tokens = tiktoken_rs::o200k_base_singleton()
        .lock()
        .encode_ordinary(output)
        .len();
    println!("{label}\t{}\t{tokens}", output.len());
}

fn lean(label: &str, value: Value) {
    count(label, &(lean_render::render(&value).unwrap() + "\n"));
}

fn report(n: usize) -> Report {
    Report {
        status: "current",
        version: Some("a".repeat(64)),
        before_version: None,
        nodes: n,
        rows: (0..n)
            .map(|i| Row {
                id: Some(id(i)),
                check: "crystallization_tokens",
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
            &format!("check success N={n}"),
            &(r.cli_lean().unwrap() + "\n"),
        );
        let receipt = checked_publication::Receipt {
            status: "current",
            check: "accepted",
            accepted_before: Some("b".repeat(40)),
            candidate: Some("c".repeat(40)),
            tree: Some("d".repeat(40)),
            draft_commit: None,
            draft_base: None,
            draft_branch: None,
            published: false,
            version: r.version.clone(),
            view: Some("/tmp/example/home".into()),
            validation: Some(report(n)),
            index_reuse: None,
            accepted_index_available: true,
            draft_index_available: false,
            graph_checks: vec![],
            semantic_findings: vec![],
            elapsed_ms: 100,
        };
        count(
            &format!("accepted receipt N={n}"),
            &(checked_publication::cli_json(&receipt).unwrap() + "\n"),
        );
        let mut submitted = serde_json::to_value(&receipt).unwrap();
        submitted["home"] = json!("/tmp/example/home");
        submitted["worktree_removed"] = json!(true);
        submitted["home_advance"] = json!({"status":"current","advanced":true,
            "home":"/tmp/example/home","reader_before":"b".repeat(40),
            "reader":"c".repeat(40),"accepted":"c".repeat(40),"version":"a".repeat(64)});
        count(
            &format!("submit receipt shape N={n}"),
            &(checked_publication::cli_json(&submitted).unwrap() + "\n"),
        );
        r.status = "rejected";
        for i in 0..n {
            r.rows.push(Row {
                id: Some(id(i)),
                check: "status_transition_or_reason",
                rule: "sample-rule",
                tokens: None,
                source: None,
                target: None,
                law: None,
            });
        }
        count(
            &format!("check one error per node N={n}"),
            &format!("rejected: {}\n", r.cli_lean().unwrap()),
        );
        r.rows = (0..n)
            .map(|i| Row {
                id: Some(id(i)),
                check: "placement_count",
                rule: "sample-rule",
                tokens: None,
                source: Some(id(i)),
                target: Some(id(i + 1)),
                law: None,
            })
            .collect();
        count(
            &format!("check structural rows E={n}"),
            &format!("rejected: {}\n", r.cli_lean().unwrap()),
        );
        let nodes: serde_json::Map<String, Value> = (0..n)
            .map(|i| (id(i), json!({"status":"in_progress"})))
            .collect();
        let ids: Vec<_> = (0..n).map(id).collect();
        for changed in [false, true] {
            let changed_ids = if changed { ids.clone() } else { vec![] };
            let receipt = json!({"status":"current","nodes":nodes,
                "normalized_ids":changed_ids,"reopened_ids":changed_ids});
            count(
                &format!("normalize N={n} all_changed={changed}"),
                &(serde_json::to_string_pretty(&receipt).unwrap() + "\n"),
            );
        }
        let node_records = (0..n).map(|i| json!({"id":id(i),"name":"Synthetic node",
            "status":"in_progress","micro_summary":"A short synthetic summary for output budgeting.",
            "children":if i == 0 {(1..n).map(id).collect::<Vec<_>>()} else {vec![]}})).collect();
        let outline = outline_artifacts::assemble(&outline_artifacts::Request {
            owner: id(0),
            source_version: "a".repeat(64),
            nodes: node_records,
        })
        .unwrap();
        for mode in ["shallow", "full"] {
            count(
                &format!("outline wide {mode} N={n}"),
                outline[mode]["text"].as_str().unwrap(),
            );
        }
    }
    // Twenty deepest hits of a chain: shared ancestors repeat for every hit.
    let n = 904;
    let mut rows = Vec::new();
    for hit in 1..=20 {
        let leaf_depth = n - 21 + hit;
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
    lean(
        "search 20 deepest chain hits N=904 rows=17890",
        json!({"status":"current", "version":"a".repeat(64),
        "version_scope":"authored_store","total_matches":904,"rows":rows}),
    );
    // A large header is legal independently of the 5,000-token body ceiling.
    let warning = " ordinary".repeat(16_384);
    let node = format!(
        "---\nid: {:?}\nname: \"Synthetic node\"\nmicro_summary: \"Sample\"\nstatus: \"in_progress\"\nsee_also: []\nwarnings: {warning:?}\n---\nSmall body.\n",
        id(0)
    );
    count("read with 16384-token warning", &node);
    lean(
        "runtime-check sample",
        json!({"status":"checked","runtime":"/tmp/example/runtime"}),
    );
    lean(
        "instructions sample",
        json!({"version":"a".repeat(64),"path":format!("/tmp/example/home/nodes/{}/node.md", id(0))}),
    );
    lean(
        "index sample",
        json!({"status":"current","version":"a".repeat(64),"version_scope":"authored_store","nodes":904,"links":1808}),
    );
    lean(
        "init sample",
        json!({"home":"/tmp/example/home","created":true,"starter_nodes":904,"pointer":"AGENTS.md","commit":"b".repeat(40)}),
    );
    lean(
        "advance sample",
        json!({"status":"current","home":"/tmp/example/home","reader_before":"b".repeat(40),"reader":"c".repeat(40),"accepted":"c".repeat(40),"version":"a".repeat(64),"advanced":true}),
    );
    lean(
        "worktree sample",
        json!({"worktree":"/tmp/example/draft","base":"b".repeat(40)}),
    );
    lean(
        "create sample",
        json!({"id":id(0),"path":format!("/tmp/example/draft/nodes/{}/node.md", id(0)),"parent":id(1),"committed":false}),
    );
    lean(
        "source-code sample",
        json!({"revision":"b".repeat(40),"path":"/tmp/example/source","maintenance_instructions":"/tmp/example/source/nodes/6da0b35a-955a-4ad2-bdce-2994ba26119c/node.md"}),
    );
    for (label, value) in [
        (
            "session-capture",
            json!({"status":"captured","archive_key":"example-key","commit":"b".repeat(40)}),
        ),
        (
            "session-attach",
            json!({"status":"attached","node_id":id(0),"attachment_name":"example.jsonl","archive_key":"example-key"}),
        ),
        ("session-restore", json!({"status":"restored","links":904})),
        (
            "session-backup",
            json!({"status":"backed_up","commit":"b".repeat(40),"destination_configured":true}),
        ),
        (
            "session-retention-config",
            json!({"enabled":true,"archive_root":"/tmp/example/archive","backup_enabled":true,"backup_configured":true}),
        ),
        (
            "account",
            json!({"id":"example","username":"example","email":"example@example.invalid","display_name":"Example","subscription_status":"active","paid":true}),
        ),
    ] {
        count(
            &format!("{label} sample"),
            &(serde_json::to_string_pretty(&value).unwrap() + "\n"),
        );
    }
    for n in [1, 904] {
        let paths: Vec<_> = (0..n).map(|i| format!("nodes/{}/node.md", id(i))).collect();
        lean(
            &format!("update refreshed paths={n}"),
            json!({"status":"updated","application":"v0.0.0",
            "knowledge":"example", "refresh_instruction_paths":serde_json::to_string(&paths).unwrap()}),
        );
    }
}
