// Thin native observation surface for the pure LEAN outline renderer.
//
// One JSON document on stdin, only its UTF-8 LEAN representation on stdout plus a final newline.
// Rejected input writes nothing to stdout and exits nonzero with the reason on stderr.
// This is a test/inspection surface: no graph loading, persistence, index refresh, token
// computation or generated-file publication, and no separate service or install target.

#[path = "../lean_render.rs"]
mod lean_render;

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
    let document: serde_json::Value =
        serde_json::from_str(&supplied).map_err(|error| format!("malformed JSON: {error}"))?;
    let rendered = lean_render::render(&document)?;
    let handle = std::io::stdout();
    let mut stdout = handle.lock();
    writeln!(stdout, "{rendered}").map_err(|error| error.to_string())?;
    stdout.flush().map_err(|error| error.to_string())
}

// Registered through this crate until Cargo/lib test registration lands. This binary owns the one
// `#[path]` declaration of the renderer, so the test module uses it without loading it twice.
#[cfg(test)]
#[path = "../../tests/successor/lean_render.rs"]
mod rendered_outline;
