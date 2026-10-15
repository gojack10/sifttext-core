// Thin native observation surface for the pure SiftText-to-searchable-document projection.
//
// One JSON request on stdin, one JSON document on stdout plus a final newline. The result is
// exactly `{title, body}`: `title` is the authored `name` and `body` is the authored
// `micro_summary`, `scope` and `crystallization` joined by LF in that order, every character of all
// four preserved exactly.
//
// The request is exactly four required string keys: `name`, `micro_summary`, `scope` and
// `crystallization`. Key order does not matter. `name` and `micro_summary` must be nonblank, where
// blank means empty or entirely Unicode whitespace; nothing is trimmed to make it nonblank. `scope`
// and `crystallization` may be empty strings.
//
// A rejected request writes nothing to stdout and exits nonzero with the reason on stderr. That
// covers malformed, truncated, empty or trailing-document JSON, a non-object document, a missing or
// unknown key, a non-string value at any of the four keys, and a blank authored `name` or
// `micro_summary`. Unknown keys are rejected so generated context, status, reasons, relationships or
// any other authored field can never enter a searchable document through this surface by accident.
//
// This is a test/inspection surface only, and the projection it exposes is deliberately not a
// complete authored-store validator: it does not take a UUID, a tree, a status or any reason field,
// it does not decide whether `crystallization` is valid for the node's status, and accepting an
// empty `crystallization` here represents a possibly unresolved record rather than approving a
// resolved store. There is no graph loading, Markdown parsing, SQLite hydration, index refresh,
// candidate retrieval, ranking, paging, snippet, card, scope, access or working-version behavior, no
// token computation, no status boost, no persistence, no file access and no service or install
// target. A later real search consumes this same helper from its own already-validating caller.

#[path = "../search_document.rs"]
mod search_document;

use serde::Deserialize;
use std::io::{Read, Write};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    name: String,
    micro_summary: String,
    scope: String,
    crystallization: String,
}

fn main() {
    let mut supplied = String::new();
    // ponytail: whole-request read, no size cap. This observer is a local test surface driven by
    // the external gate; cap the read if it ever becomes an untrusted input path.
    if let Err(error) = std::io::stdin().read_to_string(&mut supplied) {
        eprintln!("rejected: unreadable request: {error}");
        std::process::exit(1);
    }
    match document_json(&supplied) {
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

/// The whole accepted boundary: parse one four-field request and serialize exactly
/// `{title, body}`. Public to this crate's test module only.
fn document_json(supplied: &str) -> Result<String, String> {
    let request: Request = serde_json::from_str(supplied)
        .map_err(|error| format!("malformed search document request: {error}"))?;
    let document = search_document::project(search_document::Fields {
        name: &request.name,
        micro_summary: &request.micro_summary,
        scope: &request.scope,
        crystallization: &request.crystallization,
    })?;
    serde_json::to_string(&document).map_err(|error| error.to_string())
}

// Registered through this crate until Cargo/lib test registration lands. This binary owns the one
// `#[path]` declaration of the projection, so the test module uses it without loading it twice.
#[cfg(test)]
#[path = "../../tests/successor/search_document.rs"]
mod search_document_projection;
