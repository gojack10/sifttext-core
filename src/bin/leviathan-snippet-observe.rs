// Thin native observation surface for the vendored Leviathan snippet selection.
//
// One JSON request `{texts, shown, query, max_chars}` on stdin, one JSON document on stdout plus a
// final newline. The result is exactly `{snippet}`: the real upstream `Query::highlight` terms
// scored against the supplied ordered texts by the pinned upstream selection in
// `src/vendor/leviathan/snippet.rs`, or `null` when upstream would produce no snippet.
//
// A rejected request writes nothing to stdout and exits nonzero with the reason on stderr. That
// covers malformed or truncated JSON, a non-object document, a missing or wrong-typed field, a
// non-string element of `texts`/`shown`, and a negative or non-integer `max_chars`.
//
// This is a test/inspection surface only: no SQLite, no FTS5 execution, no candidate retrieval,
// ranking, scope or access inference, no record/`Mapping` extraction, no card or public result
// construction, no filesystem read or write, no persistence and no source mutation. `texts` and
// `shown` may be empty for codec-only probes; that is not valid authored-store generation input.

// The accepted query module is vendored whole and unchanged; this surface consumes only its
// `highlight` output, so the preparation methods it does not call stay quiet here.
#[allow(dead_code)]
#[path = "../vendor/leviathan/query_text.rs"]
mod query_text;
#[path = "../vendor/leviathan/snippet.rs"]
mod snippet;

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    texts: Vec<String>,
    shown: Vec<String>,
    query: String,
    max_chars: u64,
}

#[derive(Serialize)]
struct Excerpt {
    snippet: Option<String>,
}

fn main() {
    let mut supplied = String::new();
    // ponytail: whole-request read, no size cap. This observer is a local test surface driven by
    // the external gate; cap the read if it ever becomes an untrusted input path.
    if let Err(error) = std::io::stdin().read_to_string(&mut supplied) {
        eprintln!("rejected: unreadable request: {error}");
        std::process::exit(1);
    }
    match excerpt(&supplied) {
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

/// The whole accepted boundary: parse one request, take the real pinned upstream highlight terms
/// and serialize exactly `{snippet}`. Public to this crate's test module only.
fn excerpt(supplied: &str) -> Result<String, String> {
    let request: Request =
        serde_json::from_str(supplied).map_err(|error| format!("malformed snippet request: {error}"))?;
    let highlight = query_text::Query::parse(&request.query).highlight();
    let chosen = snippet::select(&request.texts, &highlight, &request.shown, request.max_chars as usize);
    let excerpt = Excerpt { snippet: chosen };
    serde_json::to_string(&excerpt).map_err(|error| error.to_string())
}

// Registered through this crate until Cargo/lib test registration lands. This binary owns the one
// `#[path]` declaration of the vendored selection, so the test module uses it without loading that
// source twice.
#[cfg(test)]
#[path = "../../tests/successor/leviathan_snippet.rs"]
mod leviathan_snippet_selection;
