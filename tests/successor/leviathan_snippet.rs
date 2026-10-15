// Repo-side tests for the vendored Leviathan matched-sentence selection and character clipping.
//
// These are worker-authored unit tests over the pinned upstream code and this crate's request
// boundary. The independent acceptance oracle stays the registered external check, which spawns
// the real native observer and compares its exact JSON output for its own specimens. Nothing here
// is a fixture-specific answer or a copy of them: every expectation below is read off the pinned
// upstream `src/card.rs` (`one_line`, `stem`, `words`, `snippet`) and `src/text.rs` (`truncate`)
// source that was copied into `src/vendor/leviathan/snippet.rs`.
//
// This file is compiled as the test module of `src/bin/leviathan-snippet-observe.rs`, which owns
// the `#[path]` declarations of both vendored modules, so `cargo test --locked` runs these tests
// without loading either source twice. When this frontier is registered in `src/lib.rs` and Cargo,
// the `use` lines below become `use sifttext_file_graph::vendor::leviathan::snippet::select;` and
// this file can move to its own `[[test]]` target unchanged.

use crate::excerpt;
use crate::query_text::Query;
use crate::snippet::select;
use serde_json::{Value, json};

fn texts(items: &[&str]) -> Vec<String> {
    items.iter().map(|t| (*t).to_string()).collect()
}

fn excerpt_of(items: &[&str], shown: &[&str], query: &str, max_chars: u64) -> Value {
    let supplied = json!({
        "texts": items,
        "shown": shown,
        "query": query,
        "max_chars": max_chars,
    })
    .to_string();
    let text =
        excerpt(&supplied).unwrap_or_else(|error| panic!("rejected a valid request {supplied}: {error}"));
    serde_json::from_str(&text).expect("excerpt must return exactly one JSON document")
}

/// The helper's own boundary, so an expectation can be stated without the transport.
fn selected(items: &[&str], highlight: &[&str], shown: &[&str], max_chars: usize) -> Option<String> {
    select(
        &texts(items),
        &texts(highlight),
        &texts(shown),
        max_chars,
    )
}

#[test]
fn upstream_whitespace_flattening_and_sentence_splitting_are_preserved() {
    // `one_line` collapses every whitespace run; `sentences` splits only after `.!?;` when
    // whitespace follows, so a hostname, a version and a semicolon-terminated segment survive.
    assert_eq!(
        selected(&["\n Alpha\t beta \r\n stays.  "], &["alpha"], &[], 300).as_deref(),
        Some("Alpha beta stays.")
    );
    assert_eq!(
        selected(&["Visit alpha.dev v1.2 now; Beta follows."], &["alpha"], &[], 300).as_deref(),
        Some("Visit alpha.dev v1.2 now;")
    );
    // No whitespace after `!`, so `Alpha!Beta?` is one segment; the `?` does split.
    assert_eq!(
        selected(&["Alpha!Beta? Gamma;"], &["beta"], &[], 300).as_deref(),
        Some("Alpha!Beta?")
    );
}

#[test]
fn distinct_stem_hits_win_and_the_earliest_segment_ties() {
    // Two distinct highlight stems beat one, even from the later sentence.
    assert_eq!(
        selected(&["Alpha alpha alpha. Alpha beta wins."], &["alpha", "beta"], &[], 300).as_deref(),
        Some("Alpha beta wins.")
    );
    // Equal hit counts never replace the earlier segment, nor an earlier text.
    assert_eq!(
        selected(&["Alpha first! Alpha second?"], &["alpha"], &[], 300).as_deref(),
        Some("Alpha first!")
    );
    assert_eq!(
        selected(&["Alpha first.", "Alpha second."], &["alpha"], &[], 300).as_deref(),
        Some("Alpha first.")
    );
    // Overlapping stems score once: `prin` shadows `print`, so the two-stem segment still wins.
    assert_eq!(
        selected(&["Printing printer. Prin beta."], &["print", "printer", "beta"], &[], 300).as_deref(),
        Some("Prin beta.")
    );
    // One word records one hit at the first matching stem position, so two stems that both match
    // the same word still score one and the earliest segment keeps the tie.
    assert_eq!(
        selected(&["Alpha alpha. Beta alone."], &["alpha", "al"], &[], 300).as_deref(),
        Some("Alpha alpha.")
    );
}

#[test]
fn prefix_stemming_is_anchored_to_alphanumeric_word_runs() {
    // Upstream stems `printer` to `print` and matches by prefix on each alphanumeric run.
    assert_eq!(
        selected(&["Printing prints plainly."], &["printer"], &[], 300).as_deref(),
        Some("Printing prints plainly.")
    );
    // A stem in the middle of a word is not a match.
    assert_eq!(selected(&["Xalpha is not an initial stem."], &["alpha"], &[], 300), None);
    // The highlight term is not re-split on punctuation, so `alpha-beta` stems to `alpha-be`
    // and matches neither `alpha` nor `beta`. Upstream behavior, not a repaired hyphen.
    assert_eq!(selected(&["Alpha-beta is present."], &["alpha-beta"], &[], 300), None);
    // Highlight terms are matched case-insensitively against the lowercased segment while the
    // returned segment keeps its authored case.
    assert_eq!(
        selected(&["Café 東京 works."], &["café", "東京"], &[], 300).as_deref(),
        Some("Café 東京 works.")
    );
}

#[test]
fn already_shown_strings_suppress_their_text_and_segment() {
    // A shown string that contains the whole flattened text removes the text.
    assert_eq!(
        selected(&["Alpha beta."], &["alpha"], &["Earlier Alpha beta. Later"], 300),
        None
    );
    // A shown segment is skipped and the next matching segment wins.
    assert_eq!(
        selected(&["Alpha visible. Beta hidden."], &["alpha", "beta"], &["Alpha visible."], 300)
            .as_deref(),
        Some("Beta hidden.")
    );
    // Containment is case-sensitive and shown strings are not whitespace-flattened, both exactly
    // as upstream: neither of these suppresses anything.
    assert_eq!(
        selected(&["Alpha one. Alpha two."], &["alpha"], &["alpha one."], 300).as_deref(),
        Some("Alpha one.")
    );
    assert_eq!(
        selected(&["Alpha\tone."], &["alpha"], &["Alpha\tone."], 300).as_deref(),
        Some("Alpha one.")
    );
}

#[test]
fn card_cap_clamps_to_forty_and_clips_on_char_offsets() {
    let long = "alpha xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string();
    let clipped = "alpha xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\u{2026}";
    // `Card::build` clamps the requested cap to at least 40, so 0, 10 and 40 agree.
    for requested in [0usize, 10, 40] {
        assert_eq!(
            selected(&[long.as_str()], &["alpha"], &[], requested).as_deref(),
            Some(clipped),
            "cap {requested} must clamp to 40"
        );
    }
    // Exactly 40 characters is returned whole; 41 is cut to 39 plus the ellipsis.
    let whole = "Alpha 0123456789 0123456789 0123456789xy";
    let over = "Alpha 0123456789 0123456789 0123456789xyz";
    assert_eq!(selected(&[whole], &["alpha"], &[], 40).as_deref(), Some(whole));
    assert_eq!(
        selected(&[over], &["alpha"], &[], 40).as_deref(),
        Some("Alpha 0123456789 0123456789 0123456789x\u{2026}")
    );

    // The leading window keeps the 40 characters before the first match.
    let far = "0123456789 0123456789 0123456789 0123456789 0123456789 alpha tail".to_string();
    assert_eq!(
        selected(&[far.as_str()], &["alpha"], &[], 60).as_deref(),
        Some("\u{2026}456789 0123456789 0123456789 0123456789 alpha tail")
    );
    // A cap of 40 cuts the 50-character window and may drop the match entirely, as upstream does.
    assert_eq!(
        selected(&[far.as_str()], &["alpha"], &[], 40).as_deref(),
        Some("\u{2026}456789 0123456789 0123456789 0123456789\u{2026}")
    );
}

#[test]
fn clipping_counts_characters_not_bytes() {
    // Five 10-character accented runs: the first match is at character 55, so the window starts
    // at character 15 and keeps six characters of the second run. A byte offset would cut inside
    // a two-byte `é` and produce different text.
    let run = "\u{e9}".repeat(10);
    let far = format!("{run} {run} {run} {run} {run} alpha fin");
    let expected = format!("\u{2026}{} {run} {run} {run} alpha fin", "\u{e9}".repeat(6));
    let window = selected(&[far.as_str()], &["alpha"], &[], 60);
    assert_eq!(window.as_deref(), Some(expected.as_str()));
    // The window is counted in characters, not bytes: 49 characters plus the leading ellipsis.
    assert_eq!(window.as_ref().map(|w| w.chars().count()), Some(50));
    // Non-Latin text is returned whole when it fits the cap.
    assert_eq!(
        excerpt_of(&["Café 東京 works."], &[], "café 東京", 300)["snippet"],
        "Café 東京 works."
    );
}

#[test]
fn empty_highlight_and_empty_input_yield_no_snippet() {
    // `Card::build` produces no snippet at all when there is nothing to highlight, whatever the
    // text contains.
    assert_eq!(selected(&["Alpha beta."], &[], &[], 300), None);
    // Stopword-only and exclusion-only queries highlight nothing upstream.
    assert_eq!(excerpt_of(&["Alpha beta."], &[], "the and", 300)["snippet"], Value::Null);
    assert_eq!(excerpt_of(&["Alpha beta."], &[], "-alpha", 300)["snippet"], Value::Null);
    assert_eq!(excerpt_of(&["Alpha beta."], &[], "", 300)["snippet"], Value::Null);
    // No searched text, and searched text with no words at all.
    assert_eq!(excerpt_of(&[], &[], "alpha", 300)["snippet"], Value::Null);
    assert_eq!(excerpt_of(&[" \n\t "], &[], "alpha", 300)["snippet"], Value::Null);
}

#[test]
fn the_observer_uses_the_real_upstream_query_parser() {
    // A phrase and an exclusion go through the accepted `Query::parse`/`highlight` path: the
    // phrase contributes both of its terms and the exclusion never becomes a highlight term.
    assert_eq!(
        excerpt_of(&["Alpha alone. Alpha beta paired."], &[], "\"alpha beta\" -gamma", 300)["snippet"],
        "Alpha beta paired."
    );
    // `-gamma` never becomes a highlight term and upstream never filters a segment for it: the
    // earliest one-hit segment still wins over the later `beta` hit.
    assert_eq!(
        excerpt_of(&["Alpha gamma. Beta alone."], &[], "\"alpha beta\" -gamma", 300)["snippet"],
        "Alpha gamma."
    );
    // The same terms handed to the helper directly select the same segment.
    let query = Query::parse("\"alpha beta\" -gamma");
    assert_eq!(query.highlight(), ["alpha", "beta"]);
    assert_eq!(
        selected(&["Alpha alone. Alpha beta paired."], &["alpha", "beta"], &[], 300).as_deref(),
        Some("Alpha beta paired.")
    );
}

#[test]
fn observer_result_is_exactly_one_snippet_field() {
    let value = excerpt_of(&["Alpha beta."], &[], "alpha", 300);
    let keys: Vec<String> = value.as_object().expect("object result").keys().cloned().collect();
    assert_eq!(keys, ["snippet"], "the result carries exactly the snippet key");
    assert_eq!(value["snippet"], "Alpha beta.");
    // A null result keeps the one key too, rather than omitting it.
    let empty = excerpt_of(&["Alpha beta."], &[], "omega", 300);
    let keys: Vec<String> = empty.as_object().expect("object result").keys().cloned().collect();
    assert_eq!(keys, ["snippet"]);
    assert!(empty["snippet"].is_null());
}

#[test]
fn raw_malformed_json_is_rejected() {
    // The registered packet carries serialized transport negatives. These are the malformed-byte
    // classes it cannot express: empty input, truncation, garbage and trailing content.
    for supplied in [
        "",
        "{",
        "[]",
        "null",
        "17",
        "not JSON",
        r#"{"query":"x"} trailing"#,
        r#"{"texts":["Alpha"],"shown":[],"query":"alpha","max_chars":300,}"#,
        r#"{"texts":["Alpha"],"shown":[],"query":"alpha""#,
        r#"{"texts":["Alpha"],"shown":[],"query":"alpha","max_chars":300"#,
    ] {
        assert!(excerpt(supplied).is_err(), "accepted malformed request {supplied:?}");
    }
}

#[test]
fn missing_wrong_typed_and_out_of_range_fields_are_rejected() {
    for supplied in [
        "{}",
        "null",
        r#"[{"texts":[],"shown":[],"query":"a","max_chars":1}]"#,
        // Each required field alone, and each with the wrong type.
        r#"{"shown":[],"query":"alpha","max_chars":300}"#,
        r#"{"texts":"Alpha","shown":[],"query":"alpha","max_chars":300}"#,
        r#"{"texts":[5],"shown":[],"query":"alpha","max_chars":300}"#,
        r#"{"texts":["Alpha"],"query":"alpha","max_chars":300}"#,
        r#"{"texts":["Alpha"],"shown":null,"query":"alpha","max_chars":300}"#,
        r#"{"texts":["Alpha"],"shown":[false],"query":"alpha","max_chars":300}"#,
        r#"{"texts":["Alpha"],"shown":[],"max_chars":300}"#,
        r#"{"texts":["Alpha"],"shown":[],"query":1,"max_chars":300}"#,
        r#"{"texts":["Alpha"],"shown":[],"query":"alpha"}"#,
        r#"{"texts":["Alpha"],"shown":[],"query":"alpha","max_chars":-1}"#,
        r#"{"texts":["Alpha"],"shown":[],"query":"alpha","max_chars":1.5}"#,
        r#"{"texts":["Alpha"],"shown":[],"query":"alpha","max_chars":"40"}"#,
        r#"{"texts":["Alpha"],"shown":[],"query":"alpha","max_chars":null}"#,
        // An unknown field is not part of this request shape.
        r#"{"texts":["Alpha"],"shown":[],"query":"alpha","max_chars":300,"limit":5}"#,
    ] {
        assert!(excerpt(supplied).is_err(), "accepted an invalid request: {supplied}");
    }
}

#[test]
fn selection_is_a_pure_function_of_the_request() {
    // Same request, same bytes: no cache, no file and no store state can change a result.
    let first = excerpt_of(&["Alpha beta. Gamma."], &["Alpha beta."], "alpha gamma", 300);
    assert_eq!(first, excerpt_of(&["Alpha beta. Gamma."], &["Alpha beta."], "alpha gamma", 300));
    assert_eq!(first["snippet"], "Gamma.");

    // SQL and FTS5 syntax are ordinary text here: nothing is built, matched or executed.
    assert_eq!(
        excerpt_of(&["select login from nodes where id"], &[], "select", 300)["snippet"],
        "select login from nodes where id"
    );
    assert_eq!(
        excerpt_of(&["login OR logout"], &[], "login OR logout", 300)["snippet"],
        "login OR logout"
    );
}
