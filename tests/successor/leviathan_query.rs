// Repo-side tests for the vendored Leviathan lexical query preparation.
//
// These are worker-authored unit tests over the pinned upstream code and this crate's request
// boundary. The independent acceptance oracle stays the registered external check, which spawns
// the real native observer and compares its exact JSON output for its own specimens. Nothing here
// is a fixture-specific answer or a copy of them: every expectation below is read off the pinned
// upstream source region copied into `src/vendor/leviathan/query_text.rs`.
//
// This file is compiled as the test module of `src/bin/leviathan-query-observe.rs`, which owns the
// single `#[path]` declaration of that module, so `cargo test --locked` runs these tests without
// loading it twice. When this frontier is registered in `src/lib.rs` and Cargo, the `use` lines
// below become `use sifttext_file_graph::vendor::leviathan::query_text::Query;` and this file can
// move to its own `[[test]]` target unchanged.

use crate::prepare;
use crate::query_text::{Query, query_words};
use serde_json::{Value, json};

fn prepared(query: &str) -> Value {
    let supplied = json!({ "query": query }).to_string();
    let text =
        prepare(&supplied).unwrap_or_else(|error| panic!("rejected a valid query {query:?}: {error}"));
    serde_json::from_str(&text).expect("prepare must return exactly one JSON document")
}

/// Upstream joins positive terms with ` OR `, so splitting the real expression recovers them.
fn terms(expression: &str) -> Vec<&str> {
    expression.split(" OR ").collect()
}

#[test]
fn upstream_words_phrases_and_exclusions_are_preserved_in_source_order() {
    let q = Query::parse(r#"Login LOOP on app-01 "reset link" -sso -"single sign-on" OR* NEAR"#);
    assert_eq!(q.words, ["login", "loop", "app-01", "near"]);
    assert_eq!(q.phrases, ["reset link"]);
    assert_eq!(q.excluded, ["sso", "single sign-on"]);
    let positive = q.fts().unwrap();
    assert_eq!(
        positive,
        r#""login" OR "loop" OR "app-01" OR "near" OR "reset link""#
    );
    let negative = q.not_fts().unwrap();
    assert_eq!(negative, r#""sso" OR "single sign-on""#);
    assert_eq!(q.highlight(), ["login", "loop", "app-01", "near", "reset", "link"]);
    assert!(!q.is_empty());
}

#[test]
fn words_deduplicate_while_phrases_and_exclusions_keep_source_occurrences() {
    assert_eq!(Query::parse("Kettle kettle KETTLE mug").words, ["kettle", "mug"]);
    assert_eq!(
        Query::parse(r#""dry run" "dry run""#).phrases,
        ["dry run", "dry run"]
    );

    let q = Query::parse("alpha -beta -beta");
    assert_eq!(q.excluded, ["beta", "beta"]);
    let negative = q.not_fts().unwrap();
    assert_eq!(negative, r#""beta" OR "beta""#);
}

#[test]
fn an_unclosed_quote_keeps_the_rest_of_the_input_as_one_phrase() {
    let q = Query::parse(r#""unterminated phrase"#);
    assert_eq!(q.phrases, ["unterminated phrase"]);
    assert!(q.words.is_empty());
    let positive = q.fts().unwrap();
    assert_eq!(positive, r#""unterminated phrase""#);

    // A negated unclosed quote keeps upstream's phrase tokenization, stopwords included.
    let q = Query::parse(r#"Alpha -"rest of the line"#);
    assert_eq!(q.words, ["alpha"]);
    assert_eq!(q.excluded, ["rest of the line"]);
    let negative = q.not_fts().unwrap();
    assert_eq!(negative, r#""rest of the line""#);
}

#[test]
fn stopword_and_punctuation_only_input_is_a_valid_empty_result() {
    for query in ["", "   ", "\t\n", "the and OR with", r#"!!! "" - ---"#, "…—-., _ -"] {
        let q = Query::parse(query);
        assert!(q.words.is_empty() && q.phrases.is_empty(), "{query:?} kept positive terms");
        assert!(q.is_empty(), "{query:?} must be empty");
        assert!(q.fts().is_none(), "{query:?} must build no positive expression");
        assert!(q.not_fts().is_none(), "{query:?} must build no exclusion expression");
        assert!(q.highlight().is_empty(), "{query:?} must highlight nothing");
    }

    // Exclusions alone never make a query non-empty.
    let q = Query::parse("-quiet");
    assert!(q.is_empty());
    assert_eq!(q.excluded, ["quiet"]);
    let negative = q.not_fts().unwrap();
    assert_eq!(negative, r#""quiet""#);
}

#[test]
fn trailing_punctuation_is_trimmed_and_query_syntax_stays_inert() {
    assert_eq!(Query::parse("Alpha.. beta- gamma_").words, ["alpha", "beta", "gamma"]);

    // Upstream splits on word characters, so FTS5 operators and column syntax are just words.
    let q = Query::parse("NEAR(foo) col:span 50%-off");
    assert_eq!(q.words, ["near", "foo", "col", "span", "50", "off"]);
    let positive = q.fts().unwrap();
    assert_eq!(positive, r#""near" OR "foo" OR "col" OR "span" OR "50" OR "off""#);

    // A quote inside a chunk ends the phrase there and the remainder is re-scanned, unchanged.
    // `a` is an upstream stopword, so the one-word phrase contributes only `quoted`.
    let q = Query::parse(r#"a "quoted "inner" tail"#);
    assert_eq!(q.words, ["quoted", "inner", "tail"]);
    assert!(q.phrases.is_empty());
}

#[test]
fn tokenization_is_unicode_aware_and_case_folded() {
    let q = Query::parse("ÄPFEL 東京 résumé");
    assert_eq!(q.words, ["äpfel", "東京", "résumé"]);
    let positive = q.fts().unwrap();
    assert_eq!(positive, r#""äpfel" OR "東京" OR "résumé""#);
    assert_eq!(q.highlight(), ["äpfel", "東京", "résumé"]);
    assert_eq!(query_words("ÄPFEL 東京").collect::<Vec<_>>(), ["äpfel", "東京"]);
}

#[test]
fn positive_expression_caps_at_64_terms_and_exclusion_at_32() {
    let words: Vec<String> = (1..=70).map(|n| format!("cap{n}")).collect();
    let q = Query::parse(&words.join(" "));
    assert_eq!(q.words.len(), 70, "the word list itself is never truncated");
    assert_eq!(q.highlight().len(), 70);
    let positive = q.fts().unwrap();
    let expression = terms(&positive);
    assert_eq!(expression.len(), 64);
    assert_eq!(expression.first().unwrap(), &r#""cap1""#);
    assert_eq!(expression.last().unwrap(), &r#""cap64""#);

    // Words and phrases share the one 64-term budget, words first.
    let mut mixed = vec![r#""head phrase""#.to_owned()];
    mixed.extend((1..=70).map(|n| format!("mid{n}")));
    let q = Query::parse(&mixed.join(" "));
    assert_eq!(q.phrases, ["head phrase"]);
    let positive = q.fts().unwrap();
    let expression = terms(&positive);
    assert_eq!(expression.len(), 64);
    assert_eq!(expression.first().unwrap(), &r#""mid1""#);
    assert_eq!(expression.last().unwrap(), &r#""mid64""#);

    let excluded: Vec<String> = (1..=40).map(|n| format!("-drop{n}")).collect();
    let q = Query::parse(&format!("keep {}", excluded.join(" ")));
    assert_eq!(q.excluded.len(), 40);
    let negative = q.not_fts().unwrap();
    let expression = terms(&negative);
    assert_eq!(expression.len(), 32);
    assert_eq!(expression.first().unwrap(), &r#""drop1""#);
    assert_eq!(expression.last().unwrap(), &r#""drop32""#);
}

#[test]
fn observer_result_is_exactly_the_seven_preparation_fields() {
    let value = prepared(r#"Trail "quiet mode" -hush"#);
    let mut keys: Vec<String> = value
        .as_object()
        .expect("object result")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    let mut expected = vec![
        "excluded",
        "fts",
        "highlight",
        "is_empty",
        "not_fts",
        "phrases",
        "words",
    ];
    expected.sort();
    assert_eq!(
        keys,
        expected,
        "the result carries exactly the seven preparation fields"
    );
    assert_eq!(value["words"], json!(["trail"]));
    assert_eq!(value["phrases"], json!(["quiet mode"]));
    assert_eq!(value["excluded"], json!(["hush"]));
    assert_eq!(value["fts"], r#""trail" OR "quiet mode""#);
    assert_eq!(value["not_fts"], r#""hush""#);
    assert_eq!(value["highlight"], json!(["trail", "quiet", "mode"]));
    assert_eq!(value["is_empty"], false);

    // The empty case still carries every key, with the two expressions as real nulls.
    let value = prepared("the");
    assert_eq!(value["words"], json!([]));
    assert_eq!(value["phrases"], json!([]));
    assert_eq!(value["excluded"], json!([]));
    assert_eq!(value["highlight"], json!([]));
    assert!(value["fts"].is_null(), "absent positive expression must be null");
    assert!(value["not_fts"].is_null(), "absent exclusion expression must be null");
    assert_eq!(value["is_empty"], true);
}

#[test]
fn raw_malformed_json_is_rejected() {
    // The registered packet carries one valid-JSON non-object negative. These are the transport
    // defects around it: empty, truncated, byte garbage, the wrong container and trailing content.
    for supplied in [
        "",
        "{",
        r#"{"query":"unterminated"#,
        r#"{"query": "unclosed}"#,
        r#"{"query":"ok"} trailing"#,
        "[]",
        r#"[{"query":"x"}]"#,
        "null",
        "17",
        "not JSON",
        r#"{"query":"x","query":"y"}"#,
    ] {
        assert!(prepare(supplied).is_err(), "accepted malformed request {supplied:?}");
    }
}

#[test]
fn missing_and_nonstring_query_fields_are_rejected() {
    for supplied in [
        "{}",
        r#"{"query":17}"#,
        r#"{"query":null}"#,
        r#"{"query":true}"#,
        r#"{"query":["login"]}"#,
        r#"{"query":{"text":"login"}}"#,
        r#"{"op":"query","query":"login"}"#,
        r#"{"query":"login","limit":10}"#,
    ] {
        assert!(
            prepare(supplied).is_err(),
            "accepted a request that is not exactly one `{{query}}` string: {supplied}"
        );
    }
}

#[test]
fn preparation_is_a_pure_function_of_the_request_text() {
    // Same request, same bytes: no cache, no file and no SQLite state can change a result.
    let query = r#"Repeat "same phrase" -excl"#;
    assert_eq!(prepared(query), prepared(query));
    assert_eq!(Query::parse(query), Query::parse(query));

    // SQL keywords are ordinary words here: this surface never builds or runs a statement.
    let value = prepared("select login from nodes where id");
    assert_eq!(value["words"], json!(["select", "login", "nodes", "id"]));
    assert_eq!(value["fts"], r#""select" OR "login" OR "nodes" OR "id""#);
    assert_eq!(prepared("from where with")["fts"], Value::Null);
}
