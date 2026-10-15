// Repo-side tests for the pure LEAN outline renderer.
//
// These are worker-authored unit tests over the encoder only. The independent acceptance oracle
// stays the parent-supplied external check, which spawns the real native observer and decodes its
// output with the pinned TypeScript implementation. Nothing here is a fixture-specific answer.
//
// This file is compiled as the test module of `src/bin/lean-render-observe.rs`, which owns the
// single `#[path]` declaration of `src/lean_render.rs`, so `cargo test --locked` runs these tests
// without loading that module twice. When this frontier is registered in `src/lib.rs` and Cargo,
// the `use` line below becomes `use sifttext_file_graph::lean_render::render;` and this file can
// move to its own `[[test]]` target unchanged.

use crate::lean_render::render;
use serde_json::json;

fn rendered(document: &serde_json::Value) -> String {
    render(document).unwrap_or_else(|error| panic!("rejected valid outline document: {error}"))
}

fn rejected(document: &serde_json::Value) -> String {
    match render(document) {
        Ok(text) => panic!("accepted out-of-boundary document as {text:?}"),
        Err(error) => error,
    }
}

#[test]
fn empty_object_matches_the_pinned_encoder() {
    // The pinned encoder writes an empty object as "{}" and its decoder rejects an empty
    // document, so the renderer must emit the empty-object token instead of no lines at all.
    assert_eq!(render(&json!({})), Ok("{}".to_owned()));
}

#[test]
fn scalar_metadata_and_empty_rows() {
    let document = json!({
        "coverage": "shallow",
        "source_version": "synthetic-version-1",
        "path_template": "nodes/{id}/node.md",
        "token_encoding": "o200k_base",
        "rows": []
    });
    assert_eq!(
        rendered(&document),
        "coverage:shallow\npath_template:nodes/{id}/node.md\nsource_version:synthetic-version-1\ntoken_encoding:o200k_base\nrows[0]:"
    );
}

#[test]
fn uniform_rows_use_the_tabular_dialect() {
    let document = json!({
        "coverage": "full",
        "rows": [
            { "id": "a", "depth": 0, "children": 2 },
            { "id": "b", "depth": 1, "children": 0 }
        ]
    });
    assert_eq!(
        rendered(&document),
        "coverage:full\nrows[2]:children\tdepth\tid\n  2\t0\ta\n  0\t1\tb"
    );
}

#[test]
fn optional_row_fields_use_the_semi_tabular_marker() {
    let document = json!({
        "rows": [
            { "id": "a", "status": "in_progress", "warnings": "Keep \" and \\ plus\ttab\nLF\rCR and Ω漢字." },
            { "id": "b", "status": "stuck", "blocker": "Waiting\nfor input." }
        ]
    });
    let text = rendered(&document);
    let mut lines = text.split('\n');
    assert_eq!(lines.next(), Some("rows[2]:id\tstatus\t~"));
    assert_eq!(
        lines.next(),
        Some("  a\tin_progress\twarnings:\"Keep \"\" and \\\\ plus\ttab\\nLF\rCR and Ω漢字.\"")
    );
    assert_eq!(
        lines.next(),
        Some("  b\tstuck\tblocker:\"Waiting\\nfor input.\"")
    );
    assert_eq!(lines.next(), None);
}

#[test]
fn lossless_cell_escaping_keeps_quotes_tabs_lf_cr_backslash_and_unicode() {
    let document = json!({
        "rows": [ { "id": "a", "v": "q\"\\\t\n\rΩ" }, { "id": "b", "v": "plain" } ]
    });
    assert_eq!(
        rendered(&document),
        "rows[2]:id\tv\n  a\t\"q\"\"\\\\\t\\n\rΩ\"\n  b\tplain"
    );
}

#[test]
fn codec_only_strings_stay_strings() {
    let document = json!({
        "rows": [
            { "id": "codec-1", "value": "" },
            { "id": "codec-2", "value": "F" },
            { "id": "codec-3", "value": "1e3" },
            { "id": "codec-4", "value": " leading and trailing " }
        ]
    });
    assert_eq!(
        rendered(&document),
        "rows[4]:id\tvalue\n  codec-1\t\"\"\n  codec-2\t\"F\"\n  codec-3\t\"1e3\"\n  codec-4\t\" leading and trailing \""
    );
}

#[test]
fn ambiguous_empty_object_string_keeps_its_quotes() {
    // The pinned decoder reads a bare `key:{}` as an empty object, so a literal "{}" string
    // must stay quoted in scalar context. In a cell it is unambiguous and stays bare.
    let document = json!({ "coverage": "{}", "rows": [ { "id": "a", "name": "{}" } ] });
    assert_eq!(rendered(&document), "coverage:\"{}\"\nrows[1]:id\tname\n  a\t{}");
}

#[test]
fn supplied_row_order_survives() {
    let document = json!({ "rows": [ { "id": "z" }, { "id": "m" }, { "id": "a" } ] });
    assert_eq!(rendered(&document), "rows[3]:id\n  z\n  m\n  a");
}

#[test]
fn rows_without_a_shared_field_use_the_dashed_list() {
    let document = json!({ "rows": [ { "a": 1 }, { "b": "x" } ] });
    assert_eq!(rendered(&document), "rows[2]:\n  - a:1\n  - b:x");
}

#[test]
fn hyphenated_and_underscored_keys_are_supported() {
    let document = json!({ "discard_reason": "Rejected: example.", "rows": [ { "node-id": "a_b" } ] });
    assert_eq!(
        rendered(&document),
        "discard_reason:Rejected: example.\nrows[1]:node-id\n  a_b"
    );
}

#[test]
fn boundary_rejections_name_the_actual_problem() {
    assert!(rejected(&json!("scalar")).contains("JSON object"));
    assert!(rejected(&json!({ "bad.key": 1 })).contains("unsupported key"));
    assert!(rejected(&json!({ "-leading": 1 })).contains("unsupported key"));
    assert!(rejected(&json!({ "rows": [{ "id": "x", "bad.key": 1 }] })).contains("unsupported key"));
    assert!(rejected(&json!({ "rows": "nope" })).contains("array"));
    assert!(rejected(&json!({ "rows": [1] })).contains("each row must be an object"));
    assert!(rejected(&json!({ "rows": [{}] })).contains("at least one field"));
    assert!(rejected(&json!({ "rows": [{ "id": "x", "nested": { "a": 1 } }] }))
        .contains("non-scalar"));
    assert!(rejected(&json!({ "coverage": { "a": 1 }, "rows": [] })).contains("non-scalar"));
}
