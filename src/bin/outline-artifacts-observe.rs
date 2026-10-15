// Thin native observation surface for pure generated-outline artifact assembly.
//
// One JSON request on stdin, one JSON `{full:{text,meta},shallow:{text,meta}}` document on stdout
// plus a final newline. A rejected request writes nothing to stdout and exits nonzero with the
// reason on stderr.
//
// This is a test/inspection surface: no Markdown parsing, no SQLite hydration, no index refresh, no
// file or cache writing, no publication, no cross-view race/recovery behaviour and no service or
// install target. The reusable assembly is the same pure module the later library registration
// will use.

#[path = "../lean_render.rs"]
mod lean_render;
#[path = "../outline_artifacts.rs"]
mod outline_artifacts;
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
    let request: outline_artifacts::Request = serde_json::from_str(&supplied)
        .map_err(|error| format!("malformed artifact request: {error}"))?;
    let artifacts = outline_artifacts::assemble(&request)?;
    let text = serde_json::to_string(&artifacts).map_err(|error| error.to_string())?;
    let handle = std::io::stdout();
    let mut stdout = handle.lock();
    writeln!(stdout, "{text}").map_err(|error| error.to_string())?;
    stdout.flush().map_err(|error| error.to_string())
}

// Registered through this crate until Cargo/lib test registration lands. This binary owns the one
// `#[path]` declaration of the assembly, so the test module uses it without loading it twice.
#[cfg(test)]
#[path = "../../tests/successor/outline_artifacts.rs"]
mod generated_outline_artifacts;
