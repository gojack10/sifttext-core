// Repo-side tests for the vendored Leviathan fixed title/body result-card construction.
//
// These are worker-authored unit tests over the pinned upstream card code and this crate's request
// boundary. The independent acceptance oracle stays the registered external check, which spawns the
// real native observer and compares its exact JSON output for its own specimens. Nothing here is a
// fixture-specific answer or a copy of them: every expectation below is read off the pinned upstream
// `src/card.rs` (`Card::build`, `one_line`, `snippet`), `src/fields.rs` (`placeholder_key`,
// `Mapping::first`, `Mapping::strings`, `Mapping::is_empty_value`) and `src/text.rs`
// (`Query::highlight`, `truncate`) sources, and every clipped string is written as a computed
// character count rather than a copied literal.
//
// This file is compiled as the test module of `src/bin/leviathan-card-observe.rs`, which owns the
// `#[path]` declarations of the vendored query, snippet and card modules, so `cargo test --locked`
// runs these tests without loading any of those sources twice. When this frontier is registered in
// `src/lib.rs` and Cargo, the `use` lines below become
// `use sifttext_file_graph::vendor::leviathan::card::build;` and this file can move to its own
// `[[test]]` target unchanged.

use crate::card::{self, build};
use crate::card_json;
use crate::query_text::Query;
use serde_json::Value;

const ELLIPSIS: char = '\u{2026}';

/// `alpha ` plus enough characters to fill a 39-character clip, then the ellipsis marker.
fn clipped_alpha(prefix: &str, total: usize) -> String {
    format!("{}{}{}", prefix, "x".repeat(total - prefix.chars().count()), ELLIPSIS)
}

/// The helper's own boundary, so an expectation can be stated without the transport.
fn built(id: &str, title: &str, body: &str, query: &str, max_chars: usize) -> card::Card {
    build(id, title, body, &Query::parse(query).highlight(), max_chars)
}

fn request(id: &str, title: &str, body: &str, query: &str, max_chars: u64) -> String {
    format!(
        r#"{{"id":{},"title":{},"body":{},"query":{},"max_chars":{max_chars}}}"#,
        serde_json::to_string(id).expect("id"),
        serde_json::to_string(title).expect("title"),
        serde_json::to_string(body).expect("body"),
        serde_json::to_string(query).expect("query"),
    )
}

fn card(id: &str, title: &str, body: &str, query: &str, max_chars: u64) -> Value {
    let supplied = request(id, title, body, query, max_chars);
    let text =
        card_json(&supplied).unwrap_or_else(|error| panic!("rejected a valid request {supplied}: {error}"));
    serde_json::from_str(&text).expect("card must be exactly one JSON document")
}

fn keys(value: &Value) -> Vec<String> {
    value.as_object().expect("object result").keys().cloned().collect()
}

#[test]
fn the_card_carries_only_id_title_and_snippet_in_upstream_order() {
    // Upstream `Card` declares `id`, then `title`, then ... `snippet`; the members this fixed
    // mapping cannot set are absent entirely, not null and not an empty map.
    assert_eq!(
        card_json(&request("n1", "Hello", "Alpha beta.", "alpha", 300)).expect("valid request"),
        "{\"id\":\"n1\",\"title\":\"Hello\",\"snippet\":\"Alpha beta.\"}"
    );

    // No empty `fields` map, no `other_group`, no `group`/`group_name`/`date`/`rank`/`relevance`.
    let bare = card("", "", "", "", 300);
    assert_eq!(keys(&bare), ["id"], "an all-empty card carries only the id");
    assert_eq!(bare["id"], "");
}

#[test]
fn an_omitted_title_and_an_omitted_snippet_leave_no_null_member() {
    // No highlight terms means no snippet, and the title key is simply absent rather than null.
    assert_eq!(keys(&card("n1", "Hello", "Alpha beta.", "the and", 300)), ["id", "title"]);
    // An empty-value title is dropped, so only the snippet remains beside the id.
    let no_title = card("n1", " \t.!! ", "Alpha beta.", "alpha", 300);
    assert_eq!(keys(&no_title), ["id", "snippet"]);
    assert_eq!(no_title["snippet"], "Alpha beta.");
}

#[test]
fn the_id_is_carried_exactly_as_supplied_and_never_normalized() {
    // `Card::build` writes `id.to_string()` from the caller's id, not a `Mapping::record_id` lookup
    // and not a trim, so an id that looks like a path or another protocol stays opaque display text.
    assert_eq!(card(" n1 ", "Hello", "Alpha beta.", "omega", 300)["id"], " n1 ");
    assert_eq!(card("  ../nodes/x\n", "Hello", "Alpha beta.", "omega", 300)["id"], "  ../nodes/x\n");
    assert_eq!(built("n/a", "Hello", "Alpha beta.", "omega", 300).id, "n/a");
}

#[test]
fn title_flattening_and_clipping_follow_first_one_line_then_truncate() {
    // `Mapping::first` trims, `one_line` collapses each whitespace run, `truncate` clips.
    assert_eq!(
        built("n1", " \n Hello\t world \r ", "Alpha beta.", "alpha", 300).title.as_deref(),
        Some("Hello world")
    );
    // Unicode whitespace is whitespace for both `trim` and `split_whitespace`.
    assert_eq!(
        built("n1", "\u{a0}Alpha\u{2003} beta\u{2028}", "Alpha beta", "alpha", 300).title.as_deref(),
        Some("Alpha beta")
    );
    // The title cap is `max(max_chars, 40)` clamped to at most 240 characters.
    let x56 = format!("alpha {}", "x".repeat(50));
    assert_eq!(built("n1", &x56, "", "alpha", 40).title.as_deref(), Some(clipped_alpha("alpha ", 39).as_str()));
    let x241 = "x".repeat(241);
    assert_eq!(
        built("n1", &x241, "", "omega", 300).title.as_deref(),
        Some(format!("{}{}", "x".repeat(239), ELLIPSIS).as_str()),
        "a 300 cap still clips a title at 240"
    );
    // The clamp to 40 is upstream, so 0, 10 and 40 agree.
    let x45 = "x".repeat(45);
    let clipped_40 = format!("{}{}", "x".repeat(39), ELLIPSIS);
    for requested in [0usize, 10, 40] {
        assert_eq!(
            built("n1", &x45, "", "omega", requested).title.as_deref(),
            Some(clipped_40.as_str()),
            "cap {requested} must clamp to 40"
        );
    }
    // Exactly 40 characters is returned whole, with no marker.
    let whole = "Alpha 0123456789 0123456789 0123456789xy";
    assert_eq!(whole.chars().count(), 40);
    assert_eq!(built("n1", whole, "", "omega", 40).title.as_deref(), Some(whole));
    // Clipping counts characters, never bytes: 45 three-byte characters still clip at 40 characters.
    let east = "東".repeat(45);
    let clipped_east = format!("{}{}", "東".repeat(39), ELLIPSIS);
    assert_eq!(built("n1", &east, "", "omega", 40).title.as_deref(), Some(clipped_east.as_str()));
    assert_eq!(clipped_east.chars().count(), 40);
    assert!(clipped_east.len() > 40, "the clip is 39 three-byte characters plus the marker");
}

#[test]
fn empty_values_drop_the_title_but_arbitrary_placeholder_text_does_not() {
    // `Mapping::is_empty_value` with no configured placeholders: trim, strip terminal `.`/`!`, trim.
    for blank in ["", " ", "\n\t ", ".", "!", ".!", " \t.!! ", "\u{a0}\u{2003}"] {
        assert_eq!(built("n1", blank, "Alpha beta.", "alpha", 300).title, None, "kept a blank title {blank:?}");
    }
    // A `?` is not a stripped terminal and `n/a` is not a configured placeholder here.
    assert_eq!(built("n1", "?", "Alpha beta.", "alpha", 300).title.as_deref(), Some("?"));
    assert_eq!(built("n1", "n/a", "Alpha beta.", "alpha", 300).title.as_deref(), Some("n/a"));
    assert_eq!(built("n1", "Done.", "Alpha beta.", "alpha", 300).title.as_deref(), Some("Done."));
    // An empty body is still not a rejected card: it is only filtered out of the searched strings.
    let only_title = built("n1", "Alpha beta.", "", "alpha", 300);
    assert_eq!(only_title.title.as_deref(), Some("Alpha beta."));
    assert_eq!(only_title.snippet, None, "the displayed title suppresses itself");
}

#[test]
fn a_displayed_title_suppresses_the_same_text_and_a_capped_title_does_not() {
    // The full flattened text is contained in the shown title, so the whole text is skipped.
    assert_eq!(built("n1", "Alpha beta.", "Alpha beta.", "alpha", 300).snippet, None);
    assert_eq!(built("n1", "Alpha", "Alpha", "alpha", 300).snippet, None);
    // Shown containment is case-sensitive, exactly as upstream: a differently cased title still
    // leaves the body searchable.
    let shown = built("n1", "ALPHA", "Alpha beta.", "alpha", 300);
    assert_eq!(shown.title.as_deref(), Some("ALPHA"));
    assert_eq!(shown.snippet.as_deref(), Some("Alpha beta."));
    // A title longer than its own shown clip is not contained in that clip, so upstream selects the
    // original text and then clips it to the same visible string.
    let long = format!("alpha {}", "x".repeat(50));
    let clipped = clipped_alpha("alpha ", 39);
    let both = built("n1", &long, "", "alpha", 40);
    assert_eq!(both.title.as_deref(), Some(clipped.as_str()));
    assert_eq!(both.snippet.as_deref(), Some(clipped.as_str()));
    // A shown segment suppresses only that segment, and the next matching segment wins.
    assert_eq!(
        built("n1", "Alpha visible.", "Alpha visible. Beta hidden.", "alpha beta", 300).snippet.as_deref(),
        Some("Beta hidden.")
    );
}

#[test]
fn searched_strings_keep_upstream_order_and_empty_value_filtering() {
    // `text = [title, body]`: the displayed title is filtered first, so a body only wins when it
    // carries more distinct stems than anything not already shown.
    assert_eq!(
        built("n1", "Alpha alone.", "Alpha beta paired.", "alpha beta", 300).snippet.as_deref(),
        Some("Alpha beta paired.")
    );
    assert_eq!(built("n1", "Hello", " \n .! ", "alpha", 300).snippet, None, "a blank body is filtered out");
    // The searched body is flattened the way upstream flattens it before matching and returning.
    assert_eq!(
        built("n1", "Hello", "Alpha\t beta\r\n stays.", "alpha", 300).snippet.as_deref(),
        Some("Alpha beta stays.")
    );
    // An empty title is not searched at all, so the body alone supplies the excerpt.
    assert_eq!(built("n1", "", "Alpha beta.", "alpha", 300).snippet.as_deref(), Some("Alpha beta."));
    // The two searched strings keep upstream path order: the title is searched first, so an equal
    // distinct-stem count in the body never displaces it. Swapping the order would select the body.
    let long = format!("alpha {}", "x".repeat(50));
    let clipped = clipped_alpha("alpha ", 39);
    let ordered = built("n1", &long, "Alpha beta.", "alpha", 40);
    assert_eq!(ordered.title.as_deref(), Some(clipped.as_str()));
    assert_eq!(ordered.snippet.as_deref(), Some(clipped.as_str()), "the body displaced the earlier title");
}

#[test]
fn highlight_terms_come_from_the_real_upstream_query_parser() {
    // Empty, stopword-only and exclusion-only queries highlight nothing, so no snippet at all.
    for query in ["", "   ", "the and", "-alpha", "the -and of"] {
        assert!(Query::parse(query).highlight().is_empty(), "{query:?} still produced highlight terms");
        assert_eq!(built("n1", "Hello", "Alpha beta.", query, 300).snippet, None, "a snippet for {query:?}");
    }
    // A quoted phrase keeps its stopword tokens, so `"the and"` does highlight.
    assert_eq!(built("n1", "", "The and yes.", "\"the and\"", 300).snippet.as_deref(), Some("The and yes."));
    // A phrase plus an exclusion contributes both phrase terms and never the excluded word.
    assert_eq!(
        built("n1", "Hello", "Alpha alone. Alpha beta paired.", "\"alpha beta\" -gamma", 300).snippet.as_deref(),
        Some("Alpha beta paired.")
    );
    // Multiple words score as distinct stems, so the richer segment wins.
    assert_eq!(
        built("n1", "", "Alpha one. Alpha beta two.", "alpha beta", 300).snippet.as_deref(),
        Some("Alpha beta two.")
    );
    // Query syntax is inert text here: nothing is built, matched or executed.
    assert_eq!(
        built("n1", "Hello", "select login from nodes", "select", 300).snippet.as_deref(),
        Some("select login from nodes")
    );
    // Unicode terms survive the real parser.
    assert_eq!(
        built("n1", "Café 東京", "Unicode café 東京 works.", "café 東京", 300).snippet.as_deref(),
        Some("Unicode café 東京 works.")
    );
}

#[test]
fn sentence_splitting_and_the_leading_window_are_upstream_ones() {
    // A segment ends after `.!?;` only when whitespace follows, so the earliest one-hit segment wins.
    assert_eq!(
        built("n1", "Hello", "Alpha first! Alpha second?", "alpha", 300).snippet.as_deref(),
        Some("Alpha first!")
    );
    // A version and a hostname stay whole.
    assert_eq!(
        built("n1", "Hello", "See v1.2 on alpha.dev now.", "alpha", 300).snippet.as_deref(),
        Some("See v1.2 on alpha.dev now.")
    );
    // The window starts 40 characters before the first match and gains a leading ellipsis.
    let far = "0123456789 0123456789 0123456789 0123456789 0123456789 alpha tail".to_string();
    assert_eq!(
        built("n1", "Hello", &far, "alpha", 60).snippet.as_deref(),
        Some("\u{2026}456789 0123456789 0123456789 0123456789 alpha tail")
    );
    // At the clamped 40 cap the window can drop the matched word entirely; upstream does exactly
    // this, and the behaviour is preserved rather than repaired.
    assert_eq!(
        built("n1", "Hello", &far, "alpha", 40).snippet.as_deref(),
        Some("\u{2026}456789 0123456789 0123456789 0123456789\u{2026}")
    );
    // A cap of 0 still searches with the clamped 40, and a match at offset 0 keeps the window at the
    // start with no leading marker.
    let body = format!("alpha {}", "x".repeat(48));
    let clipped_body = clipped_alpha("alpha ", 39);
    let clipped = built("n1", "Hello", &body, "alpha", 0);
    assert_eq!(clipped.title.as_deref(), Some("Hello"));
    assert_eq!(clipped.snippet.as_deref(), Some(clipped_body.as_str()));
}

#[test]
fn building_a_card_is_a_pure_function_of_the_request() {
    let first = built("n1", "Hello", "Alpha beta. Gamma.", "alpha gamma", 300);
    assert_eq!(first, built("n1", "Hello", "Alpha beta. Gamma.", "alpha gamma", 300));
    // Equal distinct-stem counts never replace the earlier segment.
    assert_eq!(first.snippet.as_deref(), Some("Alpha beta."));
    // The inputs are never mutated or re-normalized: the helper only reads them.
    let title = "  Hé  llo  ".to_string();
    let body = "alpha\n beta".to_string();
    let built = build("n1", &title, &body, &["alpha".to_string()], 300);
    assert_eq!(title, "  Hé  llo  ");
    assert_eq!(body, "alpha\n beta");
    assert_eq!(built.title.as_deref(), Some("Hé llo"));
    assert_eq!(built.snippet.as_deref(), Some("alpha beta"));
    // An empty display pair still builds a card instead of failing.
    assert_eq!(build("n1", "", "", &[], 300), card::Card { id: "n1".into(), title: None, snippet: None });
}

#[test]
fn the_transport_uses_the_real_upstream_query_parser() {
    // These go through `card_json`, so the observer's own `Query::parse` line is covered, not just
    // the helper. A quoted phrase keeps its stopword tokens and an exclusion never becomes a term.
    let phrase = card("n1", "", "The and yes.", "\"the and\"", 300);
    assert_eq!(phrase["snippet"], "The and yes.");
    let exclusion = card("n1", "", "Alpha beta. Gamma delta.", "alpha -beta", 300);
    // The excluded word never becomes a highlight term, and upstream never drops a segment for it:
    // the earliest `alpha` segment still wins, exactly as the pinned scorer behaves.
    assert_eq!(exclusion["snippet"], "Alpha beta.");
    // A stopword-only query highlights nothing at all, so no snippet survives the transport.
    assert_eq!(keys(&card("n1", "Hello", "Alpha beta.", "the and", 300)), ["id", "title"]);
    assert_eq!(keys(&card("n1", "Hello", "Alpha beta.", "-alpha", 300)), ["id", "title"]);
    // FTS5 syntax is inert text through the parser, never an executed expression.
    assert_eq!(card("n1", "", "login OR logout", "login", 300)["snippet"], "login OR logout");
}

#[test]
fn raw_malformed_json_is_rejected() {
    // The registered packet carries serialized transport negatives. These are the malformed-byte
    // classes it cannot express: empty input, truncation, garbage and trailing content.
    for supplied in [
        "",
        " ",
        "{",
        "[]",
        "null",
        "17",
        "not JSON",
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":300} trailing"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":300,}"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha""#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":300"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":300}{}"#,
    ] {
        assert!(card_json(supplied).is_err(), "accepted malformed request {supplied:?}");
    }
}

#[test]
fn missing_wrong_typed_and_unknown_fields_are_rejected() {
    assert!(card_json(&request("n1", "Hello", "Alpha", "alpha", 300)).is_ok());
    for supplied in [
        "{}",
        "null",
        "[]",
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":300,"extra":true}"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":300,"rank":1}"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":300,"relevance":0.5}"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":300,"snippet":"x"}"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":300,"fields":{}}"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":300,"group":"g"}"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":300,"date":"2024-03-01"}"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":300,"scope":"S"}"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":300,"status":"resolved"}"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":300,"id":"n2"}"#,
    ] {
        assert!(card_json(supplied).is_err(), "accepted an invalid request: {supplied}");
    }
    // Each required key removed on its own: an absent key is never an empty string.
    for drop in ["id", "title", "body", "query", "max_chars"] {
        let kept: Vec<String> = [
            r#""id":"n1""#.to_string(),
            r#""title":"Hello""#.to_string(),
            r#""body":"Alpha""#.to_string(),
            r#""query":"alpha""#.to_string(),
            r#""max_chars":300"#.to_string(),
        ]
        .into_iter()
        .filter(|member| !member.starts_with(&format!("\"{drop}\"")))
        .collect();
        let supplied = format!("{{{}}}", kept.join(","));
        assert!(card_json(&supplied).is_err(), "accepted a request without {drop}: {supplied}");
    }
    // Wrong types, including a null title and a null display field.
    for wrong in [
        r#"{"id":1,"title":"Hello","body":"Alpha","query":"alpha","max_chars":300}"#,
        r#"{"id":"n1","title":null,"body":"Alpha","query":"alpha","max_chars":300}"#,
        r#"{"id":"n1","title":[],"body":"Alpha","query":"alpha","max_chars":300}"#,
        r#"{"id":"n1","title":"Hello","body":[],"query":"alpha","max_chars":300}"#,
        r#"{"id":"n1","title":"Hello","body":null,"query":"alpha","max_chars":300}"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":false,"max_chars":300}"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":null,"max_chars":300}"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":-1}"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":1.5}"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":"40"}"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":null}"#,
        r#"{"id":"n1","title":"Hello","body":"Alpha","query":"alpha","max_chars":true}"#,
    ] {
        assert!(card_json(wrong).is_err(), "accepted a wrong-typed request: {wrong}");
    }
}

#[test]
fn request_key_order_does_not_change_the_card() {
    let ordered = r#"{"id":"n1","title":"Hello","body":"Alpha beta.","query":"alpha","max_chars":300}"#;
    let shuffled = r#"{"max_chars":300,"query":"alpha","body":"Alpha beta.","title":"Hello","id":"n1"}"#;
    let first = card_json(ordered).expect("ordered request");
    assert_eq!(card_json(shuffled).expect("shuffled request"), first);
    assert_eq!(serde_json::to_string(&built("n1", "Hello", "Alpha beta.", "alpha", 300)).expect("card"), first);
}
