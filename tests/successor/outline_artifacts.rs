// Repo-side tests for pure generated-outline artifact assembly.
//
// These are worker-authored unit tests over the assembly only. The independent acceptance oracle
// stays the registered external check, which spawns the real native observer and compares its exact
// JSON output for its own specimens. Nothing here is a fixture-specific answer or a copy of them.
//
// This file is compiled as the test module of `src/bin/outline-artifacts-observe.rs`, which owns the
// single `#[path]` declaration of `src/outline_artifacts.rs`, so `cargo test --locked` runs these
// tests without loading that module twice.

use crate::outline_artifacts::{Request, assemble};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const R: &str = "50000000-0000-4000-8000-000000000001";
const A: &str = "50000000-0000-4000-8000-000000000002";
const C: &str = "50000000-0000-4000-8000-000000000003";
const B: &str = "50000000-0000-4000-8000-000000000004";
const LEAF: &str = "50000000-0000-4000-8000-000000000005";
const OUTSIDE: &str = "50000000-0000-4000-8000-000000000006";
const NOWHERE: &str = "50000000-0000-4000-8000-0000000000ff";
const VERSION: &str = "assembly-test-1";

fn node(id: &str, name: &str, status: &str, children: &[&str]) -> Value {
    json!({"id": id, "name": name, "status": status, "micro_summary": format!("{name} summary."),
           "children": children})
}

/// A root with two branches, one of them two levels deep, plus a leaf owner and an unrelated second
/// tree. Supplied array order deliberately disagrees with authored preorder. `scope` and `ruled_out`
/// ride on one record so a test can prove they never reach a generated view.
fn forest() -> Value {
    let mut deep = node(C, "Deep", "discarded", &[]);
    deep["discard_reason"] = json!("Rejected for a stated reason.");
    let mut branch = node(A, "Branch", "stuck", &[C]);
    branch["blocker"] = json!("Wait for input.");
    branch["warnings"] = json!("Guard this branch.");
    branch["scope"] = json!("NOT IN AN OUTLINE");
    branch["ruled_out"] = json!("NOT IN THE CORE");
    let mut plain = node(B, "Plain", "resolved", &[]);
    plain["crystallization"] = json!("Checked answer, kept out of every view.");
    let root = node(R, "Root", "in_progress", &[B, A]);
    json!([
        node(OUTSIDE, "other root", "in_progress", &[LEAF]),
        node(LEAF, "Leaf", "not_examined", &[]),
        plain,
        branch,
        deep,
        root,
    ])
}

fn artifact(owner: &str, source_version: &str, nodes: &Value) -> Value {
    assemble(&request(owner, source_version, nodes)).expect("test forest is valid")
}

fn request(owner: &str, source_version: &str, nodes: &Value) -> Request {
    serde_json::from_value::<Request>(json!({
        "owner": owner, "source_version": source_version, "nodes": nodes,
    }))
    .expect("test request is a valid artifact request")
}

fn rejection(owner: &str, nodes: &Value) -> String {
    match assemble(&request(owner, VERSION, nodes)) {
        Ok(assembled) => panic!(
            "assembled an invalid forest as {}",
            serde_json::to_string(&assembled).unwrap_or_default()
        ),
        Err(error) => error,
    }
}

/// Swaps one complete authored record in place, so a test never creates a duplicate identity or an
/// accidental partial record while expressing a single deliberate defect.
fn replace(nodes: &Value, id: &str, changed: Value) -> Value {
    Value::Array(
        nodes
            .as_array()
            .expect("forest")
            .iter()
            .map(|item| {
                if item["id"] == id {
                    let mut patched = changed.clone();
                    patched["id"] = json!(id);
                    patched
                } else {
                    item.clone()
                }
            })
            .collect(),
    )
}

fn push(mut nodes: Value, extra: Value) -> Value {
    nodes.as_array_mut().expect("forest").push(extra);
    nodes
}

fn complete(assembled: &Value, name: &str) -> String {
    assembled[name]["text"].as_str().expect("view text").to_owned()
}

fn lines(assembled: &Value, name: &str) -> Vec<String> {
    complete(assembled, name).lines().map(str::to_owned).collect()
}

/// Top-level document keys in rendered order; a two-space line is row data, not a key.
fn keys(assembled: &Value, name: &str) -> Vec<String> {
    lines(assembled, name)
        .iter()
        .filter(|line| !line.starts_with("  "))
        .map(|line| line.split_once([':', '[']).map_or(line.as_str(), |(key, _)| key).to_owned())
        .collect()
}

fn rendered_ids(assembled: &Value, name: &str) -> Vec<String> {
    lines(assembled, name)
        .iter()
        .filter(|line| line.starts_with("  "))
        .map(|line| {
            line.split('\t')
                .find(|cell| cell.starts_with("50000000-"))
                .expect("id cell")
                .to_owned()
        })
        .collect()
}

fn row(assembled: &Value, name: &str, id: &str) -> Vec<String> {
    lines(assembled, name)
        .into_iter()
        .find(|line| line.starts_with("  ") && line.split('\t').any(|cell| cell == id))
        .unwrap_or_else(|| panic!("no rendered row for {id} in the {name} view"))
        .strip_prefix("  ")
        .expect("row indent")
        .split('\t')
        .map(str::to_owned)
        .collect()
}

fn columns(assembled: &Value, name: &str) -> Vec<String> {
    lines(assembled, name)
        .into_iter()
        .find(|line| line.starts_with("rows["))
        .expect("rows header")
        .split_once(':')
        .expect("header columns")
        .1
        .split('\t')
        .filter(|column| *column != "~")
        .map(str::to_owned)
        .collect()
}

fn tokens(assembled: &Value, name: &str) -> u64 {
    assembled[name]["meta"]["tokens"].as_u64().expect("tokens")
}

fn hash(assembled: &Value, name: &str) -> String {
    assembled[name]["meta"]["hash"].as_str().expect("hash").to_owned()
}

fn branches(assembled: &Value) -> Vec<String> {
    let mut ids: Vec<String> = assembled["shallow"]["meta"]["branch_full_tokens"]
        .as_object()
        .expect("branch map")
        .keys()
        .cloned()
        .collect();
    ids.sort_unstable();
    ids
}

fn cost_of(assembled: &Value, id: &str) -> u64 {
    assembled["shallow"]["meta"]["branch_full_tokens"][id]
        .as_u64()
        .expect("branch cost")
}

#[test]
fn the_result_is_exactly_two_views_of_text_and_metadata() {
    let nodes = forest();
    let assembled = artifact(R, VERSION, &nodes);
    let mut top: Vec<&str> = assembled
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    top.sort_unstable();
    assert_eq!(top, ["full", "shallow"]);
    for name in ["full", "shallow"] {
        let mut keys: Vec<String> = assembled[name].as_object().expect("view").keys().cloned().collect();
        keys.sort_unstable();
        assert_eq!(keys, ["meta", "text"]);
    }
}

#[test]
fn each_text_is_canonical_lexicographic_lean_with_one_final_lf() {
    let nodes = forest();
    let assembled = artifact(R, VERSION, &nodes);
    for name in ["full", "shallow"] {
        let bytes = complete(&assembled, name);
        assert!(bytes.ends_with('\n'), "{name} lacks its final LF");
        assert!(!bytes.ends_with("\n\n"), "{name} has more than one final LF");
        assert_eq!(
            keys(&assembled, name),
            ["coverage", "path_template", "rows", "source_version", "token_encoding"],
            "{name} keys are not canonical lexicographic with the rows block in place"
        );
        let mut sorted = columns(&assembled, name);
        sorted.sort_unstable();
        assert_eq!(columns(&assembled, name), sorted, "{name} row columns are not lexicographic");
    }
}

#[test]
fn full_views_carry_no_cost_and_shallow_rows_carry_only_their_own_cost() {
    let nodes = forest();
    let assembled = artifact(R, VERSION, &nodes);
    assert!(
        !complete(&assembled, "full").contains("full_view_tokens"),
        "a full artifact leaked a cost field"
    );
    let at = columns(&assembled, "shallow")
        .iter()
        .position(|column| column == "full_view_tokens")
        .expect("shallow cost column");
    for id in rendered_ids(&assembled, "shallow") {
        let cell = row(&assembled, "shallow", &id)[at].parse::<u64>().expect("numeric row cost");
        assert!(cell > 0, "shallow row {id} has no positive cost");
        assert_eq!(
            cell,
            if id == R { tokens(&assembled, "full") } else { cost_of(&assembled, &id) },
            "the shallow row and the sidecar disagree for {id}"
        );
    }
}

#[test]
fn coverage_membership_authored_order_and_counts_are_the_accepted_projection() {
    let nodes = forest();
    let assembled = artifact(R, VERSION, &nodes);
    assert_eq!(
        rendered_ids(&assembled, "shallow"),
        [R, B, A],
        "shallow is not owner plus direct children in authored order"
    );
    let full = rendered_ids(&assembled, "full");
    assert_eq!(full, [R, B, A, C], "full is not authored preorder");
    for outside in [OUTSIDE, LEAF] {
        assert!(!full.contains(&outside.to_owned()), "{outside} leaked into the owner subtree");
        assert!(!complete(&assembled, "shallow").contains(outside), "{outside} leaked into the shallow view");
    }
    // Owner-relative depth and graph-wide counts survive into the bytes: root 0 / 2 children / 3
    // descendants, grandchild at depth 2.
    assert_eq!(row(&assembled, "full", R)[..3], ["2", "0", "3"]);
    assert_eq!(row(&assembled, "full", C)[..3], ["0", "2", "0"]);
}

#[test]
fn the_shallow_sidecar_reports_the_owner_and_only_direct_children() {
    let nodes = forest();
    let assembled = artifact(R, VERSION, &nodes);
    assert_eq!(
        assembled["shallow"]["meta"]["full_view_tokens"]
            .as_u64()
            .expect("owner cost"),
        tokens(&assembled, "full"),
        "the shallow owner cost is not the complete full view cost"
    );
    assert_eq!(branches(&assembled), [A, B], "branch costs are not exactly the direct children");
    assert_ne!(
        tokens(&assembled, "full"),
        tokens(&assembled, "shallow"),
        "two different complete texts must not share one count"
    );
}

#[test]
fn every_branch_cost_is_reproducible_from_that_branch_owner_relative_full_artifact() {
    let nodes = forest();
    let assembled = artifact(R, VERSION, &nodes);
    for id in [A, B] {
        let own = artifact(id, VERSION, &nodes);
        assert_eq!(
            cost_of(&assembled, id),
            tokens(&own, "full"),
            "the sidecar cost for {id} is not its own complete full artifact cost"
        );
    }
    // A deeper descendant is billed inside its branch artifact, never as a separate branch.
    let own_deep = artifact(C, VERSION, &nodes);
    assert!(
        cost_of(&assembled, A) > tokens(&own_deep, "full"),
        "a branch artifact must cost more than its own descendant's"
    );
    assert!(!branches(&assembled).contains(&C.to_owned()), "a grandchild is exposed as a branch");
    // The owner-relative artifact is rebased, not a depth-shifted copy of the ancestor's rows.
    let internal = artifact(A, VERSION, &nodes);
    assert_eq!(rendered_ids(&internal, "full"), [A, C]);
    assert_eq!(row(&internal, "full", A)[..3], ["1", "0", "1"]);
    assert_eq!(row(&internal, "full", C)[..3], ["0", "1", "0"]);
}

#[test]
fn a_leaf_and_an_internal_owner_use_the_same_rule() {
    let nodes = forest();
    let leaf = artifact(LEAF, VERSION, &nodes);
    assert_eq!(
        leaf["shallow"]["meta"]["full_view_tokens"]
            .as_u64()
            .expect("owner cost"),
        tokens(&leaf, "full"),
        "a leaf owner still needs its own full view cost"
    );
    assert_eq!(
        leaf["shallow"]["meta"]["branch_full_tokens"]
            .as_object()
            .expect("empty branch map")
            .len(),
        0,
        "a leaf owner has no branch costs"
    );
    assert_eq!(rendered_ids(&leaf, "full"), [LEAF]);
    let internal = artifact(A, VERSION, &nodes);
    assert_eq!(branches(&internal), [C]);
    assert_eq!(cost_of(&internal, C), tokens(&artifact(C, VERSION, &nodes), "full"));
}

#[test]
fn metadata_is_the_exact_hash_and_ordinary_count_of_the_complete_text() {
    let nodes = forest();
    let assembled = artifact(R, VERSION, &nodes);
    for name in ["full", "shallow"] {
        let mut keys: Vec<String> = assembled[name]["meta"]
            .as_object()
            .expect("meta")
            .keys()
            .cloned()
            .collect();
        keys.sort_unstable();
        let expected: Vec<&str> = if name == "full" {
            vec!["encoding", "hash", "tokens"]
        } else {
            vec!["branch_full_tokens", "encoding", "full_view_tokens", "hash", "tokens"]
        };
        assert_eq!(keys, expected);
        assert_eq!(
            hash(&assembled, name),
            format!("{:x}", Sha256::digest(complete(&assembled, name).as_bytes())),
            "{name} hash is not SHA-256 of the complete text"
        );
        assert_eq!(assembled[name]["meta"]["encoding"], "o200k_base");
        assert!(tokens(&assembled, name) > 0);
        assert!(
            !complete(&assembled, name).contains("branch_full_tokens"),
            "a sidecar field entered the counted {name} text"
        );
    }
}

#[test]
fn source_text_survives_the_generated_bytes_without_leaking_authored_fields() {
    let nodes = replace(
        &replace(
            &forest(),
            C,
            json!({"id": C, "name": "Deep", "status": "discarded", "micro_summary": "Ω漢字 summary.",
                   "children": [], "discard_reason": "Rejected Ω.\nKeep the LF."}),
        ),
        A,
        json!({"id": A, "name": "Branch", "status": "stuck", "micro_summary": "  padded Ω  ",
               "children": [C], "blocker": "Tab\there.", "warnings": "Quotes \" and backslash \\.",
               "scope": "NOT IN AN OUTLINE", "ruled_out": "NOT IN THE CORE"}),
    );
    let assembled = artifact(R, VERSION, &nodes);
    let full = complete(&assembled, "full");
    assert!(full.contains("Ω漢字 summary."), "Unicode was not preserved");
    assert!(
        full.contains(r"Rejected Ω.\nKeep the LF."),
        "a multiline reason was not escaped losslessly: {full}"
    );
    assert!(
        full.contains("Tab\there"),
        "a literal tab did not survive inside its quoted cell: {full}"
    );
    assert!(
        full.contains(r#"Quotes "" and backslash \\."#),
        "quotes and backslashes were not escaped losslessly: {full}"
    );
    assert!(full.contains("\"  padded Ω  \""), "outer whitespace was not preserved: {full}");
    assert!(!full.contains("NOT IN AN OUTLINE"), "authored scope leaked into a generated view");
    assert!(!full.contains("NOT IN THE CORE"), "ruled_out text leaked into a generated view");
    assert!(!full.contains("Checked answer"), "a resolved body leaked into a generated view");
}

#[test]
fn the_supplied_version_is_propagated_quoted_and_hashed() {
    let nodes = forest();
    let numeric = artifact(R, "42", &nodes);
    for name in ["full", "shallow"] {
        assert!(
            complete(&numeric, name).contains("source_version:\"42\""),
            "{name} did not quote the numeric-looking supplied version"
        );
    }
    let plain = artifact(R, "v-next", &nodes);
    assert_ne!(hash(&numeric, "full"), hash(&plain, "full"), "a changed version must change the full bytes");
    assert_ne!(hash(&numeric, "shallow"), hash(&plain, "shallow"), "a changed version must change the shallow bytes");
    // Branch artifacts are billed at the same supplied version as the owner's.
    let own = artifact(A, "42", &nodes);
    assert!(complete(&own, "full").contains("source_version:\"42\""));
    assert_eq!(cost_of(&numeric, A), tokens(&own, "full"));
}

#[test]
fn an_invalid_record_outside_the_selected_view_still_rejects() {
    let blank = replace(
        &forest(),
        C,
        json!({"id": C, "name": "Deep", "status": "discarded", "micro_summary": "Deep summary.",
               "children": [], "discard_reason": " \t"}),
    );
    assert!(rejection(R, &blank).contains("discard_reason"));
    // The same invalid record is not displayed by this owner at all, and is still invalid store.
    assert!(rejection(LEAF, &blank).contains("discard_reason"));
}

#[test]
fn invalid_store_identity_and_version_are_rejected_not_repaired() {
    assert!(
        rejection(
            R,
            &replace(&forest(), B, json!({"id": B, "name": "Plain", "status": "in_progress",
                "micro_summary": " ", "children": []}))
        )
        .contains("blank authored summary")
    );
    assert!(
        rejection(
            R,
            &replace(&forest(), B, json!({"id": B, "name": "Plain", "status": "resolved",
                "micro_summary": "Plain summary.", "children": [], "crystallization": "  "}))
        )
        .contains("resolved with blank crystallization")
    );
    assert!(
        rejection(R, &push(forest(), node(B, "Plain again", "in_progress", &[])))
            .contains("duplicate node id")
    );
    assert!(
        rejection(
            R,
            &replace(&forest(), OUTSIDE, json!({"id": OUTSIDE, "name": "other root",
                "status": "in_progress", "micro_summary": "other root summary.", "children": [NOWHERE]}))
        )
        .contains("missing child")
    );
    assert!(
        rejection(
            R,
            &replace(&forest(), LEAF, json!({"id": LEAF, "name": "Leaf", "status": "in_progress",
                "micro_summary": "Leaf summary.", "children": [OUTSIDE]}))
        )
        .contains("cycle")
    );
    assert!(rejection(NOWHERE, &forest()).contains("is not a supplied node"));
    for version in ["", " \t "] {
        assert!(
            assemble(&request(R, version, &forest())).is_err(),
            "accepted source_version {version:?}"
        );
    }
}

#[test]
fn the_result_is_pure_text_and_costs_with_no_file_or_freshness_surface() {
    // The observable boundary is two complete texts plus their sidecars: no store, cache,
    // version-comparison or publication key appears anywhere, and no generated file is claimed.
    let assembled = artifact(R, VERSION, &forest());
    let encoded = serde_json::to_string(&assembled).expect("serialisable");
    for forbidden in ["sqlite", "stale", "current", "meta.json", "outline.lean", "refresh", "path\":"] {
        assert!(!encoded.contains(forbidden), "the artifact result leaked {forbidden:?}");
    }
    assert!(encoded.contains("nodes/{id}/node.md"), "the canonical path template is missing");
}
