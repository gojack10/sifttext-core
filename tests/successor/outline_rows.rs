// Repo-side tests for the pure graph-to-outline-row projection.
//
// These are worker-authored unit tests over the projection only. The independent acceptance oracle
// stays the registered external check, which spawns the real native observer and compares its exact
// JSON output for its own specimens. Nothing here is a fixture-specific answer or a copy of them.
//
// This file is compiled as the test module of `src/bin/outline-rows-observe.rs`, which owns the
// single `#[path]` declaration of `src/outline_rows.rs`, so `cargo test --locked` runs these tests
// without loading that module twice. When this frontier is registered in `src/lib.rs` and Cargo,
// the `use` lines below become `use sifttext_file_graph::outline_rows::project;` and this file can
// move to its own `[[test]]` target unchanged.

use crate::outline_rows::project;
use serde_json::{Value, json};

const R: &str = "3f000000-0000-4000-8000-000000000001";
const A: &str = "3f000000-0000-4000-8000-000000000002";
const B: &str = "3f000000-0000-4000-8000-000000000003";
const C: &str = "3f000000-0000-4000-8000-000000000004";
const D: &str = "3f000000-0000-4000-8000-000000000005";
const OTHER: &str = "3f000000-0000-4000-8000-000000000006";
const OUTSIDE: &str = "3f000000-0000-4000-8000-000000000007";
const NOWHERE: &str = "3f000000-0000-4000-8000-0000000000ff";

fn node(id: &str, name: &str, status: &str, children: &[&str]) -> Value {
    json!({
        "id": id,
        "name": name,
        "status": status,
        "micro_summary": format!("{name} is a test branch."),
        "children": children,
    })
}

/// A chain of four levels, a sibling branch and an unrelated second tree. The array order always
/// disagrees with the authored child order, so a projection that sorted or trusted the array fails.
fn forest() -> Vec<Value> {
    let mut stuck = node(C, "Middle", "stuck", &[D]);
    stuck["blocker"] = json!("External wait; resume on receipt.");
    let mut root = node(R, "Root", "in_progress", &[B, A]);
    root["warnings"] = json!("Root guardrail.");
    vec![
        node(OUTSIDE, "outside", "not_examined", &[]),
        node(D, "Deep", "not_examined", &[]),
        node(OTHER, "other root", "in_progress", &[OUTSIDE]),
        node(B, "Sibling", "in_progress", &[]),
        stuck,
        node(A, "Branch", "in_progress", &[C]),
        root,
    ]
}

fn request(owner: &str, coverage: &str, nodes: Vec<Value>) -> Value {
    json!({
        "owner": owner,
        "coverage": coverage,
        "source_version": "projection-test-1",
        "nodes": nodes,
    })
}

/// Replaces one authored record in place with a complete authored record, so a test never creates a
/// duplicate identity or an accidental partial record while expressing a single deliberate defect.
fn with(mut nodes: Vec<Value>, id: &str, changed: Value) -> Vec<Value> {
    for item in &mut nodes {
        if item["id"] == id {
            *item = changed.clone();
            item["id"] = json!(id);
        }
    }
    nodes
}

fn projected(document: Value) -> Value {
    let request = serde_json::from_value::<crate::outline_rows::Request>(document)
        .expect("test request is a valid row request");
    let response = project(&request).expect("test request is a valid forest");
    serde_json::to_value(&response).expect("response serialises")
}

fn rejected(document: Value) -> String {
    match serde_json::from_value::<crate::outline_rows::Request>(document) {
        Ok(request) => match project(&request) {
            Ok(response) => panic!(
                "accepted an invalid request as {}",
                serde_json::to_string(&response).unwrap_or_default()
            ),
            Err(error) => error,
        },
        Err(error) => error.to_string(),
    }
}

fn ids(document: &Value) -> Vec<&str> {
    document["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row["id"].as_str().unwrap_or_default())
        .collect()
}

fn row(document: &Value, id: &str) -> Value {
    document["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|item| item["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("missing row for {id}"))
}

fn keys(value: &Value) -> Vec<String> {
    let mut names: Vec<String> = value.as_object().expect("object").keys().cloned().collect();
    names.sort_unstable();
    names
}

#[test]
fn full_coverage_is_authored_preorder_not_array_order() {
    let document = projected(request(R, "full", forest()));
    assert_eq!(ids(&document), [R, B, A, C, D]);
    assert_eq!(document["rows"][0]["depth"], 0);
    assert_eq!(document["rows"][2]["depth"], 1);
    assert_eq!(document["rows"][3]["depth"], 2);
    assert_eq!(document["rows"][4]["depth"], 3);
}

#[test]
fn shallow_coverage_stops_at_direct_children() {
    let document = projected(request(R, "shallow", forest()));
    assert_eq!(ids(&document), [R, B, A]);
    assert_eq!(document["rows"][1]["depth"], 1);
    assert_eq!(document["rows"][2]["depth"], 1);
}

#[test]
fn counts_span_the_complete_graph_in_every_view() {
    let shallow = projected(request(R, "shallow", forest()));
    let full = projected(request(R, "full", forest()));
    for document in [&shallow, &full] {
        assert_eq!(row(document, R)["children"], 2);
        // Descendants exclude the node itself and never shrink because the view is shallow.
        assert_eq!(row(document, R)["descendants"], 4);
        assert_eq!(row(document, A)["children"], 1);
        assert_eq!(row(document, A)["descendants"], 2);
        assert_eq!(row(document, B)["children"], 0);
        assert_eq!(row(document, B)["descendants"], 0);
    }
}

#[test]
fn depth_is_owner_relative_and_unrelated_trees_stay_outside() {
    let internal = projected(request(A, "shallow", forest()));
    assert_eq!(ids(&internal), [A, C]);
    assert_eq!(internal["rows"][0]["depth"], 0);
    assert_eq!(internal["rows"][1]["depth"], 1);
    // The hidden grandchild is still counted, and its deeper warning stays out of the shallow view.
    assert_eq!(row(&internal, C)["descendants"], 1);
    let leaf = projected(request(D, "full", forest()));
    assert_eq!(ids(&leaf), [D]);
    assert_eq!(leaf["rows"][0]["depth"], 0);
    assert_eq!(row(&leaf, D)["descendants"], 0);
    for document in [&internal, &leaf] {
        for outside in [OTHER, OUTSIDE, B] {
            assert!(
                !ids(document).contains(&outside),
                "{outside} leaked into an owner-relative view"
            );
        }
    }
}

#[test]
fn metadata_is_fixed_and_the_supplied_version_is_returned_unchanged() {
    let document = projected(request(R, "shallow", forest()));
    assert_eq!(document["coverage"], "shallow");
    assert_eq!(document["source_version"], "projection-test-1");
    assert_eq!(document["source_version_scope"], "outline_view");
    assert_eq!(document["path_template"], "nodes/{id}/node.md");
    assert_eq!(document["token_encoding"], "o200k_base");
    assert_eq!(
        keys(&document),
        [
            "coverage",
            "path_template",
            "rows",
            "source_version",
            "source_version_scope",
            "token_encoding"
        ]
    );
}

#[test]
fn rows_carry_exactly_the_navigation_fields() {
    let document = projected(request(R, "full", forest()));
    assert_eq!(
        keys(&row(&document, B)),
        [
            "children",
            "depth",
            "descendants",
            "id",
            "micro_summary",
            "name",
            "status"
        ]
    );
    assert_eq!(
        keys(&row(&document, C)),
        [
            "blocker",
            "children",
            "depth",
            "descendants",
            "id",
            "micro_summary",
            "name",
            "status"
        ]
    );
}

#[test]
fn resolved_and_discarded_branches_stay_visible_with_their_active_text() {
    let resolved = projected(request(
        OTHER,
        "full",
        with(
            forest(),
            OTHER,
            json!({
                "name": "Other root",
                "status": "resolved",
                "micro_summary": "A completed second tree.",
                "children": [OUTSIDE],
                "crystallization": "Exact bounded result.",
                "scope": "Scope must never appear in a row.",
                "updated_at": "2026-10-08T00:00:00Z",
            }),
        ),
    ));
    assert_eq!(ids(&resolved), [OTHER, OUTSIDE]);
    assert_eq!(resolved["rows"][0]["status"], "resolved");
    assert_eq!(
        keys(&resolved["rows"][0]),
        [
            "children",
            "depth",
            "descendants",
            "id",
            "micro_summary",
            "name",
            "status"
        ]
    );

    let document = projected(request(
        R,
        "full",
        with(
            forest(),
            C,
            json!({
                "name": "Middle",
                "status": "discarded",
                "micro_summary": "A rejected branch.",
                "children": [D],
                "discard_reason": "Rejected: tab\there\nand \"quotes\" plus \\ and Ω漢字.",
            }),
        ),
    ));
    assert_eq!(ids(&document), [R, B, A, C, D]);
    assert_eq!(row(&document, C)["status"], "discarded");
    assert_eq!(
        row(&document, C)["discard_reason"],
        "Rejected: tab\there\nand \"quotes\" plus \\ and Ω漢字."
    );
    assert_eq!(row(&document, C).get("blocker"), None);
}

#[test]
fn guardrails_and_summaries_survive_without_trimming() {
    let document = projected(request(
        R,
        "full",
        with(
            forest(),
            A,
            json!({
                "name": "Branch",
                "status": "in_progress",
                "micro_summary": "  Preserve spaces\nand newline.  ",
                "children": [C],
                "warnings": "Keep\tthis\nline\r\nNUL:\u{0000} backslash:\\ end Ω漢字",
            }),
        ),
    ));
    assert_eq!(
        row(&document, A)["warnings"],
        "Keep\tthis\nline\r\nNUL:\u{0000} backslash:\\ end Ω漢字"
    );
    assert_eq!(
        row(&document, A)["micro_summary"],
        "  Preserve spaces\nand newline.  "
    );

    // An empty optional authored field is absent from the row, not an empty string.
    let emptied = projected(request(
        R,
        "full",
        with(
            forest(),
            B,
            json!({
                "name": "Sibling",
                "status": "not_examined",
                "micro_summary": "Known, not attempted.",
                "children": [],
                "warnings": "",
                "blocker": "",
                "deferred_reason": "",
                "discard_reason": "",
            }),
        ),
    ));
    assert_eq!(keys(&row(&emptied, B)).len(), 7);
    assert_eq!(row(&emptied, B).get("warnings"), None);
}

#[test]
fn invalid_forests_are_rejected_over_the_complete_input() {
    let blank_summary = with(
        forest(),
        D,
        json!({
            "name": "Deep",
            "status": "not_examined",
            "micro_summary": " \t\n ",
            "children": [],
        }),
    );
    // The offender is a descendant a shallow view never displays, so a shallow request must reject
    // it too rather than label an invalid store current.
    for coverage in ["shallow", "full"] {
        assert!(
            rejected(request(R, coverage, blank_summary.clone()))
                .contains("blank authored summary"),
            "a blank summary on a hidden deep node was accepted for {coverage} coverage"
        );
    }

    let duplicate_child = with(
        forest(),
        R,
        json!({"name": "Root", "status": "in_progress", "micro_summary": "Root.", "children": [B, B, A]}),
    );
    assert!(rejected(request(R, "full", duplicate_child)).contains("twice"));

    let mut duplicate_id = forest();
    duplicate_id.push(node(A, "Clone", "not_examined", &[]));
    assert!(rejected(request(R, "full", duplicate_id)).contains("duplicate node id"));

    let missing_child = with(
        forest(),
        A,
        json!({"name": "Branch", "status": "in_progress", "micro_summary": "Branch.", "children": [NOWHERE]}),
    );
    assert!(rejected(request(R, "full", missing_child)).contains("missing child"));

    let shared_child = with(
        forest(),
        OTHER,
        json!({"name": "Other root", "status": "in_progress", "micro_summary": "Other.", "children": [OUTSIDE, D]}),
    );
    assert!(rejected(request(R, "full", shared_child)).contains("more than one parent"));

    let cycle = with(
        forest(),
        D,
        json!({"name": "Deep", "status": "in_progress", "micro_summary": "Deep.", "children": [R]}),
    );
    assert!(rejected(request(R, "full", cycle)).contains("cycle"));

    // A detached self-parent is still a cycle, even outside the owner's subtree.
    let self_cycle = vec![node(OUTSIDE, "outside", "in_progress", &[OUTSIDE])];
    assert!(rejected(request(OUTSIDE, "full", self_cycle)).contains("cycle"));

    let malformed_id = with(
        forest(),
        B,
        json!({"name": "Sibling", "status": "in_progress", "micro_summary": "Sibling.", "children": ["not-a-uuid"]}),
    );
    assert!(rejected(request(R, "full", malformed_id)).contains("malformed child id"));
}

#[test]
fn invalid_authored_text_and_unknown_enums_are_rejected() {
    let missing_summary = with(
        forest(),
        B,
        json!({"name": "Sibling", "status": "in_progress", "children": []}),
    );
    assert!(rejected(request(R, "full", missing_summary)).contains("micro_summary"));

    let blank_reason = with(
        forest(),
        C,
        json!({
            "name": "Middle",
            "status": "stuck",
            "micro_summary": "Middle.",
            "children": [D],
            "blocker": " \n",
        }),
    );
    assert!(rejected(request(R, "full", blank_reason)).contains("nonblank blocker"));

    let mut missing_reason = forest();
    for item in &mut missing_reason {
        if item["id"] == C {
            item.as_object_mut().unwrap().remove("blocker");
        }
    }
    assert!(rejected(request(R, "full", missing_reason)).contains("nonblank blocker"));

    let deferred_without_reason = with(
        forest(),
        C,
        json!({
            "name": "Middle",
            "status": "deferred",
            "micro_summary": "Middle.",
            "children": [D],
            "blocker": "Stale blocker is not a deferral reason.",
        }),
    );
    assert!(
        rejected(request(R, "full", deferred_without_reason)).contains("nonblank deferred_reason")
    );

    let resolved_without_answer = with(
        forest(),
        B,
        json!({"name": "Sibling", "status": "resolved", "micro_summary": "Sibling.", "children": []}),
    );
    assert!(
        rejected(request(R, "full", resolved_without_answer)).contains("blank crystallization")
    );

    let blank_answer = with(
        forest(),
        B,
        json!({
            "name": "Sibling",
            "status": "resolved",
            "micro_summary": "Sibling.",
            "children": [],
            "crystallization": "  ",
        }),
    );
    assert!(rejected(request(R, "full", blank_answer)).contains("blank crystallization"));

    let unknown_status = with(
        forest(),
        B,
        json!({"name": "Sibling", "status": "archived", "micro_summary": "Sibling.", "children": []}),
    );
    assert!(rejected(request(R, "full", unknown_status)).contains("archived"));
    assert!(rejected(request(R, "partial", forest())).contains("unknown variant"));

    assert!(
        rejected(request(NOWHERE, "full", forest())).contains("not a supplied node"),
        "an unavailable owner was accepted"
    );
    assert!(rejected(request(R, "full", vec![])).contains("not a supplied node"));

    let mut without_version = request(R, "full", forest());
    without_version
        .as_object_mut()
        .unwrap()
        .remove("source_version");
    assert!(rejected(without_version).contains("source_version"));

    let mut blank_version = request(R, "full", forest());
    blank_version["source_version"] = json!(" \t ");
    assert!(rejected(blank_version).contains("source_version"));
}

#[test]
fn malformed_json_never_reaches_the_projection() {
    assert!(serde_json::from_str::<crate::outline_rows::Request>("{not-json").is_err());
    assert!(serde_json::from_str::<crate::outline_rows::Request>("[]").is_err());
}
