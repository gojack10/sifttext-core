// Repo-side tests for the vendored Leviathan relevance ordering and paging.
//
// These are worker-authored unit tests over the pinned upstream code and this crate's request
// boundary. The independent acceptance oracle stays the registered external check, which spawns the
// real native observer and compares its exact JSON output for its own specimens. Nothing below is a
// fixture answer or a copy of one: every expectation is read off the pinned upstream `src/query.rs`
// (`MAX_LIMIT`, the `Store::search` clamp, and the relevance branch of `Store::ranked`) that was
// extracted into `src/vendor/leviathan/relevance.rs`.
//
// This file is compiled as the test module of `src/bin/leviathan-relevance-observe.rs`, which owns
// the `#[path]` declaration of the vendored ordering, so `cargo test --locked` runs these tests
// without loading that source twice. When this frontier is registered in `src/lib.rs` and Cargo, the
// `use` lines below become `use sifttext_file_graph::vendor::leviathan::relevance::page;` and this
// file can move to its own `[[test]]` target unchanged.

use crate::page_json;
use crate::relevance::{Hit, MAX_LIMIT, page};
use serde_json::{Value, json};

fn hit(rowid: i64, bm25: f64, boost: f64, date: Option<&str>) -> Hit {
    Hit { rowid, bm25, boost, date: date.map(str::to_string) }
}

/// The helper's own boundary, so an expectation can be stated without the transport.
fn ranked(hits: &[Hit], limit: usize, offset: usize) -> Vec<(i64, usize, f64)> {
    page(hits, limit, offset).results.iter().map(|row| (row.rowid, row.rank, row.relevance)).collect()
}

fn overfetch(limit: u64, offset: u64) -> u64 {
    let supplied = json!({"hits": [], "limit": limit, "offset": offset}).to_string();
    paged(&supplied)["overfetch"].as_u64().expect("overfetch must be a nonnegative integer")
}

fn paged(supplied: &str) -> Value {
    let text = page_json(supplied).unwrap_or_else(|e| panic!("rejected a valid request: {e}"));
    serde_json::from_str(&text).expect("page_json must return exactly one JSON document")
}

fn request(hits: Value, limit: Value, offset: Value) -> String {
    json!({"hits": hits, "limit": limit, "offset": offset}).to_string()
}

fn one(rowid: i64, bm25: f64, date: Option<&str>) -> Value {
    json!({"rowid": rowid, "bm25": bm25, "boost": 1, "date": date})
}

#[test]
fn boost_multiplies_bm25_before_the_ordering() {
    // Upstream `score: bm25 * boost`, then ascending score: the weaker raw bm25 wins because its
    // configured boost makes its total smaller. Ordering on raw bm25 alone would swap these.
    assert_eq!(
        ranked(&[hit(1, -2.0, 1.0, None), hit(2, -1.0, 4.0, None)], 2, 0),
        vec![(2, 1, 4.0), (1, 2, 2.0)]
    );
    // A zero boost is a score of exactly zero, so it sorts last ascending, and a negative boost
    // flips the sign of the score. No positivity rule is invented here.
    assert_eq!(
        ranked(&[hit(1, -1.0, 0.0, None), hit(2, -1.0, 1.0, None)], 2, 0),
        vec![(2, 1, 1.0), (1, 2, 0.0)]
    );
    assert_eq!(
        ranked(&[hit(1, -2.0, -3.0, None), hit(2, 2.0, -3.0, None)], 2, 0),
        vec![(2, 1, 6.0), (1, 2, -6.0)]
    );
    // Fractional boosts are arithmetic, not a rounding input until display.
    assert_eq!(
        ranked(&[hit(1, -3.0, 0.5, None), hit(2, -0.5, 4.0, None)], 2, 0),
        vec![(2, 1, 2.0), (1, 2, 1.5)]
    );
}

#[test]
fn exact_score_ties_are_broken_by_recency_then_by_supplied_order() {
    // Identical scores, so only `b.date.cmp(&a.date)` decides: newest first, and the absent date
    // becomes the empty string, which is the smallest string and therefore sorts last.
    assert_eq!(
        ranked(
            &[hit(1, -1.0, 1.0, Some("2020")), hit(2, -1.0, 1.0, None), hit(3, -1.0, 1.0, Some("2024"))],
            3,
            0
        ),
        vec![(3, 1, 1.0), (1, 2, 1.0), (2, 3, 1.0)]
    );
    // `null` and `""` are the same string after the upstream default, so the stable sort keeps the
    // supplied order. That is this vector's order, not an upstream SQL tie guarantee.
    assert_eq!(
        ranked(&[hit(3, -1.0, 1.0, None), hit(2, -1.0, 1.0, Some(""))], 2, 0),
        vec![(3, 1, 1.0), (2, 2, 1.0)]
    );
    // Equal score and equal date: the earlier supplied element stays first, forwards and backwards.
    assert_eq!(
        ranked(&[hit(2, -1.0, 1.0, Some("2024")), hit(1, -1.0, 1.0, Some("2024"))], 2, 0),
        vec![(2, 1, 1.0), (1, 2, 1.0)]
    );
    // Date comparison is byte-wise on the stored string, so `9` beats `10`: upstream stores dates
    // as strings and never parses them here.
    assert_eq!(
        ranked(&[hit(1, -1.0, 1.0, Some("9")), hit(2, -1.0, 1.0, Some("10"))], 2, 0),
        vec![(1, 1, 1.0), (2, 2, 1.0)]
    );
}

#[test]
fn score_beats_recency_because_date_only_breaks_ties() {
    // The worse score stays worse even with the much newer date: recency is a tie break, never the
    // primary key, and it never reverses the ascending score order.
    assert_eq!(
        ranked(&[hit(1, -2.0, 1.0, Some("1900")), hit(2, -1.0, 1.0, Some("2099"))], 2, 0),
        vec![(1, 1, 2.0), (2, 2, 1.0)]
    );
}

#[test]
fn paging_follows_sorting_and_numbers_with_the_global_offset() {
    let hits = [hit(1, -1.0, 1.0, None), hit(2, -3.0, 1.0, None), hit(3, -2.0, 1.0, None)];
    // Sorted: 2, 3, 1. Page 1 of 1 is the second sorted element, and its rank is the global
    // `offset + i + 1`, not a position inside the page.
    assert_eq!(ranked(&hits, 1, 1), vec![(3, 2, 2.0)]);
    assert_eq!(ranked(&hits, 1, 2), vec![(1, 3, 1.0)]);
    assert_eq!(ranked(&hits, 2, 0), vec![(2, 1, 3.0), (3, 2, 2.0)]);
    // A page past the end is empty, not an error and not a wrap-around.
    assert_eq!(ranked(&hits, 1, 3), vec![]);
    // Selecting before sorting would hand back the first supplied element instead of the best.
    assert_eq!(ranked(&hits, 1, 0), vec![(2, 1, 3.0)]);
}

#[test]
fn the_request_limit_is_clamped_between_one_and_fifty() {
    assert_eq!(MAX_LIMIT, 50);
    // `req.limit.clamp(1, MAX_LIMIT)`: zero still yields exactly one row, the best one.
    assert_eq!(ranked(&[hit(1, -1.0, 1.0, None), hit(2, -2.0, 1.0, None)], 0, 0), vec![(2, 1, 2.0)]);
    // Above the ceiling the page is capped at 50 rows, in stable sorted order.
    let many: Vec<Hit> = (1..=60).map(|rowid| hit(rowid, -1.0, 1.0, None)).collect();
    let capped = page(&many, 60, 0);
    assert_eq!(capped.results.len(), 50);
    assert_eq!(capped.results.first().map(|row| (row.rowid, row.rank)), Some((1, 1)));
    assert_eq!(capped.results.last().map(|row| (row.rowid, row.rank)), Some((50, 50)));
    // The clamp happens before the offset arithmetic, so `want` uses the clamped limit.
    assert_eq!(overfetch(51, 2), 520);
    assert_eq!(overfetch(100, 0), 500);
}

#[test]
fn overfetch_is_ten_times_want_with_a_floor_of_one_hundred() {
    // `want = offset + limit` after the clamp, then `(want * 10).max(100)`.
    assert_eq!(overfetch(1, 0), 100);
    assert_eq!(overfetch(9, 0), 100);
    assert_eq!(overfetch(10, 0), 100);
    assert_eq!(overfetch(11, 0), 110);
    assert_eq!(overfetch(1, 10), 110);
    assert_eq!(overfetch(1, 20), 210);
    // Widened, not a 32-bit wrap: want = 4294967295 + 50 = 4294967345.
    assert_eq!(overfetch(4294967295, 4294967295), 42_949_673_450);
}

#[test]
fn supplied_hits_are_never_truncated_to_the_overfetch() {
    // Upstream applies `overfetch` to its SQL LIMIT, not to the rows it already has. The best hit
    // is deliberately last in a vector longer than the overfetch, so any input truncation loses it.
    let mut hits: Vec<Hit> = (1..=120).map(|rowid| hit(rowid, -1.0, 1.0, None)).collect();
    hits.push(hit(999, -50.0, 1.0, None));
    let page = page(&hits, 2, 0);
    assert_eq!(page.overfetch, 100);
    assert_eq!(page.results.iter().map(|row| row.rowid).collect::<Vec<_>>(), vec![999, 1]);
}

#[test]
fn displayed_relevance_rounds_half_away_from_zero_in_both_signs() {
    // `(-score * 100.0).round() / 100.0` is Rust's `f64::round`: half away from zero, in either
    // sign. Ties-to-even would display 0.12 and -0.12 instead.
    assert_eq!(
        ranked(&[hit(1, -0.125, 1.0, None), hit(2, 0.125, 1.0, None)], 2, 0),
        vec![(1, 1, 0.13), (2, 2, -0.13)]
    );
    assert_eq!(
        ranked(&[hit(1, -0.005, 1.0, None), hit(2, 0.005, 1.0, None)], 2, 0),
        vec![(1, 1, 0.01), (2, 2, -0.01)]
    );
    // Below a half cent the display collapses to zero while the ordering still uses the real score.
    assert_eq!(
        ranked(&[hit(1, -0.004, 1.0, Some("1900")), hit(2, -0.003, 1.0, Some("2099"))], 2, 0),
        vec![(1, 1, 0.0), (2, 2, 0.0)]
    );
    // A whole score stays whole, and a negative score displays negative.
    assert_eq!(ranked(&[hit(1, 2.0, 1.0, None)], 1, 0), vec![(1, 1, -2.0)]);
}

#[test]
fn the_helper_is_a_pure_function_of_the_supplied_vector() {
    let hits = [hit(1, -1.0, 1.0, None), hit(2, -3.0, 1.0, None)];
    let first = ranked(&hits, 2, 0);
    assert_eq!(first, ranked(&hits, 2, 0));
    // The caller's slice keeps its own order: sorting happens on a copy, so a later SQL-ordered
    // caller cannot be silently reordered by asking for a page.
    assert_eq!(hits[0].rowid, 1);
    assert_eq!(hits[1].rowid, 2);
    assert_eq!(first, vec![(2, 1, 3.0), (1, 2, 1.0)]);
}

#[test]
fn a_valid_response_is_exactly_overfetch_and_results() {
    let supplied = request(json!([one(1, -1.0, None)]), json!(1), json!(0));
    let document = paged(&supplied);
    let keys: Vec<&String> = document.as_object().expect("object").keys().collect();
    assert_eq!(keys, vec!["overfetch", "results"]);
    assert_eq!(
        document,
        json!({"overfetch": 100, "results": [{"rowid": 1, "rank": 1, "relevance": 1.0}]})
    );
    let row = document["results"][0].as_object().expect("row object");
    assert_eq!(row.len(), 3);
    assert!(row.contains_key("rowid") && row.contains_key("rank") && row.contains_key("relevance"));
    // An empty `hits` vector is a legal codec-only probe of this pure surface, not a store.
    assert_eq!(paged(&request(json!([]), json!(1), json!(0))), json!({"overfetch": 100, "results": []}));
}

#[test]
fn malformed_raw_json_is_rejected() {
    for supplied in [
        "",
        "{",
        "[]",
        "null",
        "17",
        "not JSON",
        r#"{"hits":[],"limit":1,"offset":0} trailing"#,
        r#"{"hits":[{"rowid":1,"bm25":-1,"boost":1,"date":null}],"limit":1,"offset":0,}"#,
        r#"{"hits":[{"rowid":1,"bm25":-1,"boost":1""#,
        r#"{"hits":[{"rowid":1,"bm25":-1,"boost":1,"date":null}"#,
    ] {
        assert!(page_json(supplied).is_err(), "accepted malformed request {supplied:?}");
    }
}

#[test]
fn missing_wrong_typed_and_unknown_fields_are_rejected() {
    let good = one(1, -1.0, None);
    for supplied in [
        "{}".to_string(),
        "null".to_string(),
        "[1,2,3]".to_string(),
        r#"{"limit":1,"offset":0}"#.to_string(),
        r#"{"hits":[],"limit":1}"#.to_string(),
        r#"{"hits":[],"offset":0}"#.to_string(),
        r#"{"hits":[],"limit":1,"offset":0,"fixture":true}"#.to_string(),
        r#"{"hits":{"rowid":1},"limit":1,"offset":0}"#.to_string(),
        r#"{"hits":[null],"limit":1,"offset":0}"#.to_string(),
        request(json!([{"bm25": -1.0, "boost": 1.0, "date": null}]), json!(1), json!(0)),
        request(json!([{"rowid": 1, "boost": 1.0, "date": null}]), json!(1), json!(0)),
        request(json!([{"rowid": 1, "bm25": -1.0, "date": null}]), json!(1), json!(0)),
        request(json!([{"rowid": 1, "bm25": -1.0, "boost": 1.0}]), json!(1), json!(0)),
        request(json!([{"rowid": 1.5, "bm25": -1.0, "boost": 1.0, "date": null}]), json!(1), json!(0)),
        request(json!([{"rowid": 1, "bm25": "-1", "boost": 1.0, "date": null}]), json!(1), json!(0)),
        request(json!([{"rowid": 1, "bm25": -1.0, "boost": "2", "date": null}]), json!(1), json!(0)),
        request(json!([{"rowid": 1, "bm25": -1.0, "boost": 1.0, "date": 2024}]), json!(1), json!(0)),
        request(json!([{"rowid": 1, "bm25": -1.0, "boost": 1.0, "date": ["2024"]}]), json!(1), json!(0)),
        request(json!([{"rowid":1,"bm25":-1.0,"boost":1.0,"date":null,"fixture":true}]), json!(1), json!(0)),
        // A hit that omits any other required key, and a hit that is not an object at all.
        request(json!([{"rowid": 1, "bm25": -1.0}]), json!(1), json!(0)),
        r#"{"hits":[],"limit":1,"offset":0,"fixture":true}"#.to_string(),
        request(json!([]), json!(-1), json!(0)),
        request(json!([]), json!(1), json!(-1)),
        request(json!([]), json!(0.5), json!(0)),
        request(json!([]), json!(1), json!(0.5)),
        request(json!([]), json!(true), json!(0)),
        request(json!([]), json!("1"), json!(0)),
        request(json!([]), json!(null), json!(0)),
        request(json!([]), json!(4294967296_u64), json!(0)),
        request(json!([]), json!(1), json!(4294967296_u64)),
    ] {
        assert!(page_json(&supplied).is_err(), "accepted an invalid request: {supplied}");
    }
    // `good` is only here to keep the helper next to the shape it produces.
    assert!(page_json(&request(Value::Array(vec![good]), json!(1), json!(0))).is_ok());
}

#[test]
fn nonfinite_and_overflowing_arithmetic_is_rejected() {
    // One bad row rejects the whole request, even beside a good one.
    let mixed = request(
        json!([
            {"rowid": 1, "bm25": 1e308, "boost": 1e308, "date": null},
            {"rowid": 2, "bm25": -1, "boost": 1, "date": null},
        ]),
        json!(1),
        json!(0),
    );
    for supplied in [
        // A non-finite column value, whether it arrives as a float literal or a JSON constant.
        r#"{"hits":[{"rowid":1,"bm25":NaN,"boost":1,"date":null}],"limit":1,"offset":0}"#.to_string(),
        r#"{"hits":[{"rowid":1,"bm25":Infinity,"boost":1,"date":null}],"limit":1,"offset":0}"#.to_string(),
        r#"{"hits":[{"rowid":1,"bm25":-Infinity,"boost":1,"date":null}],"limit":1,"offset":0}"#.to_string(),
        r#"{"hits":[{"rowid":1,"bm25":null,"boost":1,"date":null}],"limit":1,"offset":0}"#.to_string(),
        // A finite pair whose upstream product `bm25 * boost` is not finite.
        r#"{"hits":[{"rowid":1,"bm25":-1e308,"boost":1e308,"date":null}],"limit":1,"offset":0}"#.to_string(),
        // A finite score whose display numerator `-score * 100.0` is not finite, either sign.
        r#"{"hits":[{"rowid":1,"bm25":-1e308,"boost":1,"date":null}],"limit":1,"offset":0}"#.to_string(),
        r#"{"hits":[{"rowid":1,"bm25":1e308,"boost":1,"date":null}],"limit":1,"offset":0}"#.to_string(),
        mixed,
    ] {
        assert!(page_json(&supplied).is_err(), "accepted non-finite request {supplied}");
    }
    // The largest finite score whose display still fits is accepted, so the bound is finiteness and
    // not a made-up magnitude cutoff.
    let ok = r#"{"hits":[{"rowid":1,"bm25":-1e306,"boost":1,"date":null}],"limit":1,"offset":0}"#;
    assert_eq!(paged(ok)["results"][0]["relevance"], json!(1e306));
    // Each finiteness bound names itself on stderr, so a rejected request stays diagnosable and
    // removing either check is visible instead of collapsing into the other message.
    let product = r#"{"hits":[{"rowid":1,"bm25":-1e308,"boost":1e308,"date":null}],"limit":1,"offset":0}"#;
    let display = r#"{"hits":[{"rowid":1,"bm25":-1e308,"boost":1,"date":null}],"limit":1,"offset":0}"#;
    let product_error = page_json(product).expect_err("a non-finite product must be rejected");
    let display_error = page_json(display).expect_err("a non-finite display numerator must be rejected");
    assert!(product_error.contains("non-finite score"), "{product_error}");
    assert!(!product_error.contains("display"), "{product_error}");
    assert!(display_error.contains("display score"), "{display_error}");
}

#[test]
fn an_empty_vector_still_reports_the_page_it_would_have_asked_for() {
    // The observer returns `overfetch` even with nothing to page, because upstream computes it
    // before it has any rows.
    assert_eq!(paged(&request(json!([]), json!(1), json!(0)))["results"].as_array().unwrap().len(), 0);
    // The largest `want` this transport can build, without a 32-bit wrap.
    assert_eq!(overfetch(2, 4294967293), 42_949_672_950);
}
