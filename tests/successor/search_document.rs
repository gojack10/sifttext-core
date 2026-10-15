// Repo-side tests for the pure SiftText-to-searchable-document projection and its observer transport.
//
// These are worker-authored unit tests over the reusable helper and this crate's request boundary.
// The independent acceptance oracle stays the registered external check, which spawns the real
// native observer and compares its exact JSON output, or requires a nonzero exit for its invalid
// requests. Nothing below is a fixture answer or a copy of one: every expectation is read off the
// maintained contract (`title` = `name`; `micro_summary`, `scope` and `crystallization` joined by LF
// in that order) and off the four-field coverage the governing search owner preserves.
//
// This file is compiled as the test module of `src/bin/search-document-observe.rs`, which owns the
// `#[path]` declaration of the projection, so `cargo test --locked` runs these tests without loading
// that source twice. When this frontier is registered in `src/lib.rs` and Cargo, the `use` lines
// below become library paths and this file can move to its own `[[test]]` target unchanged.

use crate::document_json;
use crate::search_document::{Fields, project};
use serde_json::{Value, json};

/// The four covered fields, straight into the reusable helper.
fn fields<'a>(
    name: &'a str,
    micro_summary: &'a str,
    scope: &'a str,
    crystallization: &'a str,
) -> Fields<'a> {
    Fields {
        name,
        micro_summary,
        scope,
        crystallization,
    }
}

/// The helper's own boundary, so a mapping expectation never depends on the transport.
fn projected(
    name: &str,
    micro_summary: &str,
    scope: &str,
    crystallization: &str,
) -> (String, String) {
    let document =
        project(fields(name, micro_summary, scope, crystallization)).expect("valid authored text");
    (document.title.to_owned(), document.body)
}

fn request(supplied: &Value) -> String {
    supplied.to_string()
}

/// One accepted projection through the transport boundary, as raw serialized text.
fn raw(supplied: &str) -> String {
    document_json(supplied)
        .unwrap_or_else(|error| panic!("rejected a valid request {supplied}: {error}"))
}

/// The same accepted projection, parsed. `serde_json`'s map is ordered by key, so field-order
/// expectations belong on `raw`, not here.
fn document(supplied: &str) -> Value {
    let text = raw(supplied);
    // The helper returns one JSON value with no line terminator; only `main` adds the final newline.
    assert!(
        !text.ends_with('\n'),
        "the transport result carried its own newline"
    );
    serde_json::from_str(&text).expect("document_json must return exactly one JSON document")
}

fn body(supplied: &str) -> String {
    document(supplied)["body"]
        .as_str()
        .expect("body must be a string")
        .to_owned()
}

/// One complete four-field request, valid on its own.
fn complete() -> Value {
    json!({"name": "name", "micro_summary": "summary", "scope": "", "crystallization": "body"})
}

/// A complete request with one covered field removed.
fn without(key: &str) -> Value {
    let mut object = complete().as_object().expect("object").clone();
    assert!(
        object.remove(key).is_some(),
        "{key} is not one of the four covered fields"
    );
    Value::Object(object)
}

/// A complete request plus one extra field.
fn plus(key: &str, value: Value) -> Value {
    let mut object = complete().as_object().expect("object").clone();
    assert!(
        object.insert(key.to_owned(), value).is_none(),
        "{key} is not an extra field"
    );
    Value::Object(object)
}

/// A complete request with one covered field replaced by a non-string value.
fn typed(key: &str, value: Value) -> Value {
    let mut object = complete().as_object().expect("object").clone();
    assert!(
        object.insert(key.to_owned(), value).is_some(),
        "{key} is not a covered field"
    );
    Value::Object(object)
}

/// A complete request with one covered field replaced by a given string.
fn text(key: &str, value: &str) -> Value {
    typed(key, json!(value))
}

// --- the mapping itself ---------------------------------------------------------------

#[test]
fn title_is_the_name_and_body_is_the_three_texts_joined_by_line_feed() {
    assert_eq!(
        projected("Title only", "summary", "scope", "body"),
        ("Title only".into(), "summary\nscope\nbody".into())
    );
    // The order is fixed: summary, then scope, then crystallization. A reordered join puts the
    // question where the answer belongs, and any other separator fails the same assertion.
    let (_, body) = projected("N", "ONE", "TWO", "THREE");
    assert_eq!(body, "ONE\nTWO\nTHREE");
    assert_eq!(
        body.split('\n').collect::<Vec<_>>(),
        vec!["ONE", "TWO", "THREE"]
    );
    // Swapping the summary and the scope is a different document, not a different reading order.
    assert_eq!(projected("N", "S", "summary", "body").1, "S\nsummary\nbody");
}

#[test]
fn the_name_is_never_duplicated_into_the_body() {
    // A name whose own words are absent from the body must not be appended, prefixed or repeated.
    let (title, body) = projected("lookup login reset", "summary", "scope", "body");
    assert_eq!(title, "lookup login reset");
    assert_eq!(body, "summary\nscope\nbody");
    assert!(!body.contains("lookup"));
    // Even a name that repeats a body word stays out of the body.
    assert_eq!(
        projected("same", "same", "same", "same").1,
        "same\nsame\nsame"
    );
}

#[test]
fn an_empty_optional_text_still_contributes_its_separator() {
    // Empty `scope` is a legitimate authored value, not a missing field: two separators remain.
    assert_eq!(
        projected("Empty optional scope", "summary", "", "body").1,
        "summary\n\nbody"
    );
    assert_eq!(
        projected("Unresolved empty answer", "summary", "question", "").1,
        "summary\nquestion\n"
    );
    assert_eq!(
        projected("Both optional texts empty", "summary", "", "").1,
        "summary\n\n"
    );
    // Whitespace-only optional text is preserved, never dropped and never collapsed.
    assert_eq!(
        projected("Blank optional kept", "summary", " ", "\t").1,
        "summary\n \n\t"
    );
}

#[test]
fn nothing_is_trimmed_flattened_or_otherwise_rewritten() {
    // Leading/trailing whitespace, NBSP, tab and CR survive in both fields. A trim on the way out
    // would change the title and would merge the summary into the scope line.
    let (title, body) = projected(
        "  Exact name\t\u{a0}  ",
        "  summary \t",
        "\n question \r\n",
        "\tanswer\n",
    );
    assert_eq!(title, "  Exact name\t\u{a0}  ");
    assert_eq!(body, "  summary \t\n\n question \r\n\n\tanswer\n");
    // Embedded LF and CR are never normalized to one another or folded into a single line.
    assert_eq!(
        projected("N", "a\r\nb", "c\nd", "e\r\rf").1,
        "a\r\nb\nc\nd\ne\r\rf"
    );
    // NUL is an ordinary character here: a C-string boundary or a lossy conversion would drop it.
    let body = projected("N", "s\u{0}um", "", "body\u{0}tail\r").1;
    assert_eq!(body, "s\u{0}um\n\nbody\u{0}tail\r");
    assert_eq!(body.chars().filter(|c| *c == '\u{0}').count(), 2);
    // A trailing LF on the name is part of the title.
    assert_eq!(projected("name\n", "summary", "", "").0, "name\n");
}

#[test]
fn unicode_survives_exactly_including_combining_marks_and_emoji() {
    let (title, body) = projected(
        "日本語 é\u{301} 👩🏽‍💻",
        "Résumé Ω",
        "中文 scope",
        "العربية body",
    );
    assert_eq!(title, "日本語 é\u{301} 👩🏽‍💻");
    assert_eq!(body, "Résumé Ω\n中文 scope\nالعربية body");
    // Code-point identity, not a normalized form and not a reordered sequence.
    assert_eq!(
        title.chars().collect::<Vec<_>>(),
        "日本語 é\u{301} 👩🏽‍💻".chars().collect::<Vec<_>>()
    );
    assert!(!body.contains("  "));
}

#[test]
fn literal_reference_and_metadata_text_stays_opaque_text() {
    // Link syntax, fenced code and text that looks like a field name are searchable content here,
    // never parsed into relationships, generated context or status.
    let (title, body) = projected(
        "Literal syntax",
        "[[Name|uuid]] is text",
        "```json\n{\"x\":1}\n```",
        "# Heading\nsee_also: literal\n\\path\t\"quoted\"",
    );
    assert_eq!(title, "Literal syntax");
    assert_eq!(
        body,
        "[[Name|uuid]] is text\n```json\n{\"x\":1}\n```\n# Heading\nsee_also: literal\n\\path\t\"quoted\""
    );
    // A metadata-looking summary is not interpreted: `status: resolved` is just searchable text.
    let (_, body) = projected(
        "Opaque text",
        "status: resolved",
        "warnings: literal",
        "generated_context: literal\nsee_also: [[A|id]]",
    );
    assert_eq!(
        body,
        "status: resolved\nwarnings: literal\ngenerated_context: literal\nsee_also: [[A|id]]"
    );
}

#[test]
fn the_projection_is_pure_and_adds_nothing_of_its_own() {
    let supplied = fields("N", "M", "S", "C");
    let first = project(supplied).expect("valid");
    let second = project(supplied).expect("valid");
    assert_eq!(first, second);
    // No generated date, path, status, reason, boost, token count or surrounding whitespace appears.
    assert_eq!(first.title, "N");
    assert_eq!(first.body, "M\nS\nC");
    assert!(!first.body.starts_with('\n'));
    assert!(!first.body.ends_with("\n\n"));
    // The helper borrows its inputs and leaves them untouched.
    assert_eq!(supplied.crystallization, "C");
    assert_eq!(supplied.scope, "S");
}

// --- required nonblank authored text ---------------------------------------------------

#[test]
fn a_blank_or_whitespace_only_name_or_summary_is_rejected_not_repaired() {
    // Every entry is empty or entirely Unicode `White_Space`.
    for blank in [
        "",
        " ",
        "\t",
        "\n",
        "\r\n",
        " \t\n",
        "\u{2003}",
        "\u{a0}",
        "\u{2028}\u{2029}",
        "\u{85}",
        "\u{202f}\u{3000}",
        "\u{2000}\u{200a}",
        "\u{1680}\u{205f}",
    ] {
        assert!(
            project(fields(blank, "summary", "", "")).is_err(),
            "helper accepted a blank name {blank:?}"
        );
        assert!(
            project(fields("name", blank, "", "")).is_err(),
            "helper accepted a blank summary {blank:?}"
        );
        assert!(
            document_json(&request(&text("name", blank))).is_err(),
            "transport accepted a blank name {blank:?}"
        );
        assert!(
            document_json(&request(&text("micro_summary", blank))).is_err(),
            "transport accepted a blank summary {blank:?}"
        );
    }
    // A zero-width space and the zero-width joiners are not Unicode `White_Space`, so they are
    // ordinary nonblank text: the rule is the property, not a hand-picked character list.
    assert!(project(fields("\u{200b}", "summary", "", "")).is_ok());
    assert!(project(fields("name", "\u{200d}\u{200c}", "", "")).is_ok());
    // The same whitespace characters are legal inside an otherwise nonblank field.
    assert!(
        project(fields(
            "  \u{a0} real \u{2003}",
            "\u{2028} text",
            "\u{3000}",
            "\t\n"
        ))
        .is_ok()
    );
}

#[test]
fn rejection_names_the_offending_field_and_leaves_no_partial_document() {
    let blank_name =
        project(fields("", "summary", "scope", "body")).expect_err("a blank name must reject");
    let blank_summary =
        project(fields("name", " ", "scope", "body")).expect_err("a blank summary must reject");
    assert_eq!(blank_name, "blank authored name");
    assert_eq!(blank_summary, "blank authored micro-summary");
    // Each class names itself, so a mutation cannot turn one rule into the other silently.
    assert_eq!(
        document_json(&request(&text("name", "\u{a0}"))).expect_err("blank"),
        "blank authored name"
    );
    assert_eq!(
        document_json(&request(&text("micro_summary", "\u{2003}\u{a0}\t"))).expect_err("blank"),
        "blank authored micro-summary"
    );
    // A rejection produces no document at all, not an empty or partial one.
    assert!(document_json(&request(&text("name", ""))).is_err());
    assert!(document_json(&request(&text("micro_summary", "\n"))).is_err());
}

#[test]
fn an_empty_crystallization_is_represented_not_approved() {
    // This surface takes no status, so an empty answer is a possibly unresolved record. It must not
    // be treated as an invalid store here, and must not gain a fabricated answer either.
    let (title, body) = projected("Unresolved empty answer", "summary", "question", "");
    assert_eq!(title, "Unresolved empty answer");
    assert_eq!(body, "summary\nquestion\n");
    assert_eq!(
        document(&request(&typed("crystallization", json!("")))),
        json!({"title": "name", "body": "summary\n\n"})
    );
}

// --- transport shape -------------------------------------------------------------------

#[test]
fn a_valid_response_is_exactly_title_then_body() {
    let supplied = r#"{"name":"N","micro_summary":"M","scope":"S","crystallization":"C"}"#;
    assert_eq!(raw(supplied), r#"{"title":"N","body":"M\nS\nC"}"#);
    assert_eq!(document(supplied), json!({"title": "N", "body": "M\nS\nC"}));
    // Request key order is irrelevant; the projection order comes from the fields, not the wire.
    let reordered = r#"{"crystallization":"C","scope":"S","micro_summary":"M","name":"N"}"#;
    assert_eq!(document(reordered), document(supplied));
    assert_eq!(body(reordered), "M\nS\nC");
    // Surrounding and embedded whitespace in the document text is content, not formatting.
    assert_eq!(
        raw(r#"  {"name":"N","micro_summary":"M","scope":"S","crystallization":"C"}  "#),
        r#"{"title":"N","body":"M\nS\nC"}"#
    );
}

#[test]
fn malformed_raw_json_is_rejected() {
    for supplied in [
        "",
        "   ",
        "{",
        "[]",
        "null",
        "17",
        "\"N\"",
        "not JSON",
        "{}",
        r#"{"name":"N","micro_summary":"M","scope":"S","crystallization":"C"} trailing"#,
        r#"{"name":"N","micro_summary":"M","scope":"S","crystallization":"C"}{}"#,
        "{\"name\":\"N\",\"micro_summary\":\"M\",\"scope\":\"S\",\"crystallization\":\"C\"}\n{}",
        r#"{"name":"N","micro_summary":"M","scope":"S","crystallization":"C",}"#,
        r#"{"name":"N","micro_summary":"M","scope":"S""#,
        r#"{"name":"unterminated","micro_summary":"M","scope":"S","crystallization":"C"#,
        "{\"name\":\"N\",\"micro_summary\":\"M\",\"scope\":\"S\",\"crystallization\":\"C\"}}",
    ] {
        assert!(
            document_json(supplied).is_err(),
            "accepted malformed request {supplied:?}"
        );
    }
}

#[test]
fn missing_wrong_typed_and_unknown_fields_are_rejected() {
    for key in ["name", "micro_summary", "scope", "crystallization"] {
        assert!(
            document_json(&request(&without(key))).is_err(),
            "accepted a request without {key}"
        );
    }
    for supplied in [
        // A present-but-null covered field is not the caller's empty-scope normalization.
        typed("name", Value::Null),
        typed("micro_summary", Value::Null),
        typed("scope", Value::Null),
        typed("crystallization", Value::Null),
        // Wrong value types at each of the four covered fields.
        typed("name", json!(17)),
        typed("name", json!(["N"])),
        typed("micro_summary", json!(false)),
        typed("micro_summary", json!({"text": "summary"})),
        typed("scope", json!({})),
        typed("scope", json!([""])),
        typed("crystallization", json!([])),
        typed("crystallization", json!(0)),
        // Anything that is not one of the four covered fields, including a generated or authored
        // field that would otherwise leak into a searchable document.
        plus("warnings", json!("not a searchable input field")),
        plus("status", json!("resolved")),
        plus("title", json!("alternate title")),
        plus("body", json!("alternate body")),
        plus("content", json!("alternate body")),
        plus("generated_context", json!("must not be searchable")),
        plus("id", json!("10000000-0000-4000-8000-000000000001")),
        plus("see_also", json!([{"target": "x", "reason": "y"}])),
        plus("children", json!([])),
        plus("blocker", json!("External wait.")),
        // Non-object documents.
        Value::Null,
        json!([1, 2, 3]),
        json!("just a string"),
        json!(17),
        json!(true),
    ] {
        let supplied = request(&supplied);
        assert!(
            document_json(&supplied).is_err(),
            "accepted an invalid request: {supplied}"
        );
    }
    // The shape those rejections differ from is still accepted, and an empty scope is legal.
    assert_eq!(
        document(&request(&complete())),
        json!({"title": "name", "body": "summary\n\nbody"})
    );
}

#[test]
fn transport_shape_and_authored_validity_are_different_failures() {
    // Collapsing the two classes would let a mutation drop one rule while the surface still exits
    // nonzero, so each class names itself.
    let malformed = document_json("{").expect_err("malformed JSON must reject");
    let unknown = document_json(&request(&plus("status", json!("resolved"))))
        .expect_err("an unknown field must reject");
    let absent =
        document_json(&request(&without("scope"))).expect_err("an absent field must reject");
    let wrong_type = document_json(&request(&typed("name", json!(17))))
        .expect_err("a non-string name must reject");
    let blank = document_json(&request(&text("name", " "))).expect_err("a blank name must reject");
    assert!(
        malformed.starts_with("malformed search document request:"),
        "{malformed}"
    );
    assert!(unknown.contains("unknown field `status`"), "{unknown}");
    assert!(absent.contains("missing field `scope`"), "{absent}");
    assert!(wrong_type.contains("invalid type"), "{wrong_type}");
    assert_eq!(blank, "blank authored name");
}

#[test]
fn a_realistic_authored_pair_round_trips_every_byte_through_the_transport() {
    // Built with JSON escapes rather than a raw string, because the crystallization contains a
    // quote-hash sequence that would otherwise close a single-hash raw literal.
    let supplied = "{\"name\":\"  Wayfinder\\tΩ  \",\"micro_summary\":\"\\n\\u2003Navigates.\\n\",\"scope\":\"\",\"crystallization\":\"# Heading\\r\\nsee_also: [[A|uuid]]\\u0000\"}";
    let expected_title = "  Wayfinder\tΩ  ";
    let expected_body = "\n\u{2003}Navigates.\n\n\n# Heading\r\nsee_also: [[A|uuid]]\u{0}";
    assert_eq!(
        document(supplied),
        json!({"title": expected_title, "body": expected_body})
    );
    // summary, then the empty scope held between two separators, then the crystallization verbatim.
    assert_eq!(
        body(supplied).split('\n').take(5).collect::<Vec<_>>(),
        vec!["", "\u{2003}Navigates.", "", "", "# Heading\r"]
    );
}
