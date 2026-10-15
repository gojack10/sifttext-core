// Thin native observation surface for the vendored Leviathan lexical query preparation.
//
// One JSON request `{query}` on stdin, one JSON document on stdout plus a final newline. The
// result is exactly `words`, `phrases`, `excluded`, `fts`, `not_fts`, `highlight` and `is_empty`:
// every array and every `Option` expression is the real pinned upstream `Query` output, so the
// external gate observes upstream preparation rather than a second local spelling.
//
// A rejected request writes nothing to stdout and exits nonzero with the reason on stderr.
// This is a test/inspection surface only: no SQLite, no FTS5 execution, no candidate retrieval,
// ranking, snippets, ancestry/access/version adaptation, persistence or file mutation, and no
// separate service or install target.

#[path = "../vendor/leviathan/query_text.rs"]
mod query_text;

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    query: String,
}

#[derive(Serialize)]
struct Preparation<'a> {
    words: &'a [String],
    phrases: &'a [String],
    excluded: &'a [String],
    fts: Option<String>,
    not_fts: Option<String>,
    highlight: Vec<String>,
    is_empty: bool,
}

fn main() {
    let mut supplied = String::new();
    // ponytail: whole-request read, no size cap. This observer is a local test surface driven by
    // the external gate; cap the read if it ever becomes an untrusted input path.
    if let Err(error) = std::io::stdin().read_to_string(&mut supplied) {
        eprintln!("rejected: unreadable request: {error}");
        std::process::exit(1);
    }
    match prepare(&supplied) {
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

/// The whole accepted boundary: parse one request, prepare it with the pinned upstream `Query`
/// and serialize the exact seven-key result. Public to this crate's test module only.
fn prepare(supplied: &str) -> Result<String, String> {
    let request: Request =
        serde_json::from_str(supplied).map_err(|error| format!("malformed query request: {error}"))?;
    let query = query_text::Query::parse(&request.query);
    let preparation = Preparation {
        words: &query.words,
        phrases: &query.phrases,
        excluded: &query.excluded,
        fts: query.fts(),
        not_fts: query.not_fts(),
        highlight: query.highlight(),
        is_empty: query.is_empty(),
    };
    serde_json::to_string(&preparation).map_err(|error| error.to_string())
}

// Registered through this crate until Cargo/lib test registration lands. This binary owns the one
// `#[path]` declaration of the vendored preparation, so the test module uses it without loading
// that source twice.
#[cfg(test)]
#[path = "../../tests/successor/leviathan_query.rs"]
mod leviathan_query_preparation;
