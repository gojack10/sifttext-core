// Thin native observation surface for the vendored Leviathan fixed title/body result card.
//
// One JSON request `{id, title, body, query, max_chars}` on stdin, one JSON document on stdout plus
// a final newline. The result is the card the pinned upstream `Card::build` returns for the fixed
// mapping `title = [title]`, `text = [title, body]`: `id` exactly as supplied, an optional clipped
// one-line `title` and an optional `snippet`. Nothing else is ever emitted: there is no `fields`
// map, no `other_group`, and no `group`, `group_name`, `date`, `rank` or `relevance` key, because
// this fixed mapping has no display, group or date paths and this surface does not rank.
//
// All five keys are required and their types are fixed: four strings and one nonnegative integer.
// `title` and `body` are the already-projected display strings, so they may be empty here; that is a
// codec-only probe of this display transport and not acceptance of a blank authored name or
// micro-summary. A caller that validates authored stores must reject a blank authored field before
// asking for a card. Optional fields elsewhere in a whole store do not make a transport key
// optional: an absent `title` is a rejected request, not an omitted title.
//
// A rejected request writes nothing to stdout and exits nonzero with the reason on stderr. That
// covers malformed, truncated, empty or trailing-document JSON, a non-object document, a missing or
// unknown key, a non-string `id`/`title`/`body`/`query`, a null `title`, and a negative, fractional,
// string or null `max_chars`.
//
// This is a test/inspection surface only: no SQLite, no FTS5 execution, no candidate retrieval, no
// ranking, score, boost, tie or offset behavior, no node identification, ancestry, access or scope
// inference, no record or `Mapping` extraction, no field/group/date projection, no configured
// placeholder list, no provenance, no filesystem read or write, no persistence and no source
// mutation. The card is built by the same reusable helper a later real search must consume.

// The accepted query module is vendored whole and unchanged; this surface consumes only its
// `highlight` output, so the preparation methods it does not call stay quiet here.
#[allow(dead_code)]
#[path = "../vendor/leviathan/query_text.rs"]
mod query_text;
#[path = "../vendor/leviathan/snippet.rs"]
mod snippet;
#[path = "../vendor/leviathan/card.rs"]
mod card;

use serde::Deserialize;
use std::io::{Read, Write};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: String,
    title: String,
    body: String,
    query: String,
    max_chars: u64,
}

fn main() {
    let mut supplied = String::new();
    // ponytail: whole-request read, no size cap. This observer is a local test surface driven by
    // the external gate; cap the read if it ever becomes an untrusted input path.
    if let Err(error) = std::io::stdin().read_to_string(&mut supplied) {
        eprintln!("rejected: unreadable request: {error}");
        std::process::exit(1);
    }
    match card_json(&supplied) {
        Ok(text) => {
            let handle = std::io::stdout();
            let mut stdout = handle.lock();
            if let Err(error) = writeln!(stdout, "{text}").and_then(|()| stdout.flush()) {
                eprintln!("rejected: unwritable result: {error}");
                std::process::exit(1);
            }
        }
        Err(reason) => {
            eprintln!("rejected: {reason}");
            std::process::exit(1);
        }
    }
}

/// The whole accepted boundary: parse one request, take the real pinned upstream highlight terms and
/// serialize exactly the card this fixed mapping produces. Public to this crate's test module only.
fn card_json(supplied: &str) -> Result<String, String> {
    let request: Request =
        serde_json::from_str(supplied).map_err(|error| format!("malformed card request: {error}"))?;
    let highlight = query_text::Query::parse(&request.query).highlight();
    let built = card::build(
        &request.id,
        &request.title,
        &request.body,
        &highlight,
        request.max_chars as usize,
    );
    serde_json::to_string(&built).map_err(|error| error.to_string())
}

// Registered through this crate until Cargo/lib test registration lands. This binary owns the one
// `#[path]` declaration of the vendored card, so the test module uses it without loading that source
// twice.
#[cfg(test)]
#[path = "../../tests/successor/leviathan_card.rs"]
mod leviathan_card_construction;
