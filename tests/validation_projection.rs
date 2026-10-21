//! Public output must shrink without changing rich observer/proof inputs.
use sifttext_file_graph::{
    candidate_validation::{CEILING, Failure, GraphCheck, Report, Row},
    outlines::decode_lean,
};

fn row(id: Option<&str>, check: &'static str, tokens: Option<usize>) -> Row {
    Row {
        id: id.map(str::to_owned),
        check,
        rule: "internal-rule",
        tokens,
        source: id.map(str::to_owned),
        target: None,
        law: Some("internal-law"),
    }
}
fn report() -> Report {
    Report {
        status: "rejected",
        version: Some("candidate".into()),
        before_version: Some("baseline".into()),
        nodes: 904,
        rows: vec![],
        graph: GraphCheck {
            mode: "full_law",
            reason: "unproven base",
            base_version: Some("baseline".into()),
            candidate_version: Some("candidate".into()),
        },
        failure: Some(Failure {
            stage: "candidate",
            kind: "invalid_candidate",
        }),
        problem: None,
    }
}

#[test]
fn only_problems_and_nonduplicate_locators_reach_the_cli() {
    let mut r = report();
    r.rows = vec![
        row(Some("a"), "crystallization_tokens", Some(4)),
        row(Some("b"), "crystallization_over_limit", Some(CEILING + 1)),
        row(Some("b"), "status_transition_or_reason", None),
        row(Some("b"), "status_transition_or_reason", None),
        row(Some("c"), "status_transition_or_reason", None),
    ];
    let full = serde_json::to_value(&r).unwrap();
    let cli = decode_lean(&(r.cli_lean().unwrap() + "\n")).unwrap();
    assert_eq!(cli["rows"].as_array().unwrap().len(), 3);
    assert_eq!(
        cli["rows"][0],
        serde_json::json!({
        "id":"b", "check":"crystallization_over_limit", "tokens":5001, "limit":5000})
    );
    assert_eq!(cli["before_version"], "baseline");
    assert_eq!(full["rows"].as_array().unwrap().len(), 5);
    assert_eq!(full["rows"][0]["rule"], "internal-rule");
    assert!(r.lean().unwrap().contains("crystallization_tokens"));
    assert_eq!(serde_json::to_value(&r).unwrap(), full);
}

#[test]
fn nested_publication_uses_the_same_failure_projection_without_changing_raw_reports() {
    let mut r = report();
    r.rows = vec![
        row(Some("a"), "crystallization_tokens", Some(4)),
        row(Some("b"), "crystallization_tokens", Some(CEILING + 1)),
        row(Some("b"), "crystallization_over_limit", Some(CEILING + 1)),
    ];
    let raw = serde_json::to_value(&r).unwrap();
    let lean = r.lean().unwrap();
    let receipt = serde_json::json!({
        "status":"rejected", "check":"candidate_validation_rejected",
        "published":false, "validation":&r,
        "semantic_findings":[{"node":"b", "rule":"semantic-rule", "reason":"Keep this finding."}]
    });
    let output: serde_json::Value = serde_json::from_str(
        &sifttext_file_graph::checked_publication::cli_json(&receipt).unwrap(),
    )
    .unwrap();
    assert_eq!(output["status"], "not_accepted");
    assert_eq!(output["semantic_findings"], receipt["semantic_findings"]);
    assert_eq!(
        output["validation"],
        decode_lean(&(r.cli_lean().unwrap() + "\n")).unwrap()
    );
    assert_eq!(
        output["validation"]["rows"],
        serde_json::json!([
            {"id":"b", "check":"crystallization_over_limit", "tokens":5001, "limit":5000}
        ])
    );
    assert!(output["validation"].get("nodes").is_none());
    assert!(output["validation"].get("before_version").is_none());
    assert_eq!(raw["rows"].as_array().unwrap().len(), 3);
    assert_eq!(raw["rows"][0]["rule"], "internal-rule");
    assert_eq!(serde_json::to_value(&r).unwrap(), raw);
    assert_eq!(receipt["validation"], raw);
    assert_eq!(r.lean().unwrap(), lean);
    assert!(lean.contains("crystallization_tokens"));
}

#[test]
fn independent_field_failures_remain_and_unrelated_baseline_identity_does_not() {
    let mut r = report();
    r.rows = vec![
        row(Some("a"), "authored_field_type", None),
        row(Some("a"), "authored_field_type", None),
    ];
    let cli = decode_lean(&(r.cli_lean().unwrap() + "\n")).unwrap();
    assert_eq!(cli["rows"].as_array().unwrap().len(), 2);
    assert!(cli.get("before_version").is_none());
}

#[test]
fn unknown_and_stale_are_not_recast_as_success_or_node_invalidity() {
    for (status, stage, kind) in [
        ("rejected", "before_capture", "unknown"),
        ("stale", "source_recheck", "source_changed"),
    ] {
        let mut r = report();
        r.status = status;
        r.version = None;
        r.before_version = None;
        r.failure = Some(Failure { stage, kind });
        r.rows = vec![row(None, "validation_unavailable", None)];
        let cli = decode_lean(&(r.cli_lean().unwrap() + "\n")).unwrap();
        assert_eq!(cli["status"], status);
        assert_eq!(cli["failure_stage"], stage);
        assert_eq!(cli["failure_kind"], kind);
        assert!(cli.get("version").is_none());
        assert!(cli.get("before_version").is_none());
        assert!(cli.get("rows").is_none());
        let receipt = serde_json::json!({
            "status":"rejected", "check":"candidate_validation_rejected", "validation":&r
        });
        let nested: serde_json::Value = serde_json::from_str(
            &sifttext_file_graph::checked_publication::cli_json(&receipt).unwrap(),
        )
        .unwrap();
        assert_eq!(nested["validation"], cli);
    }
}
