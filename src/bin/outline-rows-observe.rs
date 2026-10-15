// Thin native observation surface for the pure graph-to-outline-row projection.
//
// One JSON request on stdin, one JSON metadata-plus-rows document on stdout plus a final newline.
// A rejected request writes nothing to stdout and exits nonzero with the reason on stderr.
// This is a test/inspection surface: no graph loading, Markdown parsing, SQLite hydration, index
// refresh, file publication, LEAN encoding or token computation, and no service or install target.

#[path = "../outline_rows.rs"]
mod outline_rows;

use std::io::{Read, Write};

fn main() {
    if let Err(error) = run() {
        eprintln!("rejected: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut supplied = String::new();
    // ponytail: whole-document read, no size cap. This observer is a local test surface driven by
    // the external gate; cap the read if it ever becomes an untrusted input path.
    std::io::stdin()
        .read_to_string(&mut supplied)
        .map_err(|error| error.to_string())?;
    let request: outline_rows::Request = serde_json::from_str(&supplied)
        .map_err(|error| format!("malformed row request: {error}"))?;
    let response = outline_rows::project(&request)?;
    let text = serde_json::to_string(&response).map_err(|error| error.to_string())?;
    let handle = std::io::stdout();
    let mut stdout = handle.lock();
    writeln!(stdout, "{text}").map_err(|error| error.to_string())?;
    stdout.flush().map_err(|error| error.to_string())
}

// Registered through this crate until Cargo/lib test registration lands. This binary owns the one
// `#[path]` declaration of the projection, so the test module uses it without loading it twice.
#[cfg(test)]
#[path = "../../tests/successor/outline_rows.rs"]
mod outline_row_projection;
