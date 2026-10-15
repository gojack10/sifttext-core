// Vendored upstream result-card construction, extracted from Leviathan.
//
// Pinned source: revision 702ea92dc4695de00c488a66b43d75419bbb78e8, files `src/card.rs`
// (SHA-256 ae608eab99f6c0a82c8be976d2945951d6acd00a6e007df3bc9788431cd9beda), `src/fields.rs`
// (SHA-256 8e3d7f494ea6a13ab16a5065ac8d5f12b05511cca83964fb3c9422cef753a139) and `src/text.rs`
// (SHA-256 1999633b4ad97903849d63c6654910b645470bd444c7e6a6126df50f499dc570).
// License Apache-2.0; the unmodified upstream `LICENSE` and `NOTICE` sit beside this file and
// `THIRD_PARTY.md` records the vendored query region. This file is an extracted and modified
// upstream region, not a whole-file copy.
//
// Copied verbatim from the pinned `src/card.rs`: `one_line`, the `Card::build` cap clamp
// `opts.max_chars.max(40)`, the title projection
// `mapping.first(..).map(|t| truncate(&one_line(&t), cap.min(240)))`, the `shown` seed
// `title.iter().cloned().collect()`, the `id` passthrough and the `Option::is_none` omission rules.
// Copied verbatim from the pinned `src/fields.rs`: `placeholder_key` and the blank half of
// `Mapping::is_empty_value`. Reused instead of re-ported: `snippet::select` and `snippet::truncate`
// from the already accepted snippet contribution, and the caller's real `Query::highlight` output
// from the already accepted query contribution. The empty-highlight rule and the at-least-40 cap
// are not duplicated here: `select` already reproduces both `Card::build` lines.
//
// Modified, and this is the whole adaptation: the fixed SiftText mapping is applied directly
// instead of being configured through `Mapping`. `title = [title]` and `text = [title, body]` are
// the two already-projected strings the caller hands in, there are no display, group or date paths
// and no configured placeholder values, so `FieldPath` walking, `push_text`, `Mapping::new`,
// `group_of`, `date_of`, `display_date`, `label`, the `fields` map and every other `Card` member
// are absent by construction rather than omitted by a rule invented here. Because the placeholder
// set is empty, `is_empty_value` keeps only its emptiness test.
//
// Nothing here parses a query, retrieves or ranks a candidate, opens a database, reads a file,
// extracts a record, decides access or scope, or chooses what the final public hit surface shows.

use crate::snippet::{select, truncate};
use serde::Serialize;

/// One result card: exactly the pinned upstream `Card` members this fixed mapping can ever set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Card {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

/// Upstream `fields::placeholder_key`, verbatim: trim, strip terminal `.` and `!`, trim, lowercase.
fn placeholder_key(value: &str) -> String {
    value.trim().trim_end_matches(['.', '!']).trim().to_lowercase()
}

/// Upstream `Mapping::is_empty_value` with the configured placeholder set left empty: a value is
/// empty exactly when its placeholder key is empty. Arbitrary text such as `n/a` is not empty, and
/// a `?` is not a stripped terminal.
fn is_empty_value(value: &str) -> bool {
    placeholder_key(value).is_empty()
}

/// Upstream `card::one_line`, verbatim: every whitespace run becomes one space.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The card `Card::build` produces for the fixed `title = [title]`, `text = [title, body]` mapping:
/// the `id` exactly as supplied, the trimmed one-line title clipped to `min(cap, 240)` characters
/// when it is not an empty value, and the upstream snippet taken from the searched title and body
/// that is not already displayed. `highlight` is the real `Query::highlight` output and `cap` is
/// `max_chars.max(40)`; an empty highlight list yields no snippet through the reused selection.
pub fn build(id: &str, title: &str, body: &str, highlight: &[String], max_chars: usize) -> Card {
    let cap = max_chars.max(40);
    let shown_title =
        (!is_empty_value(title)).then(|| truncate(&one_line(title.trim()), cap.min(240)));
    let shown: Vec<String> = shown_title.iter().cloned().collect();
    let texts: Vec<String> =
        [title, body].into_iter().filter(|t| !is_empty_value(t)).map(str::to_string).collect();
    let snippet = select(&texts, highlight, &shown, cap);
    Card { id: id.to_string(), title: shown_title, snippet }
}
