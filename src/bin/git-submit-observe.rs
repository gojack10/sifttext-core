//! One local submission/recovery request; no model, daemon or remote transport.
use sifttext_file_graph::{
    Policy, Result,
    checked_publication::{self, Request},
};
use std::{
    io::{self, Read, Write},
    path::Path,
};
fn run() -> Result<bool> {
    let mut input = Vec::new();
    io::stdin().take(1_048_577).read_to_end(&mut input)?;
    if input.len() > 1_048_576 {
        return Err("request resource limit".into());
    }
    let request: Request = serde_json::from_slice(&input)?;
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR")))?;
    let receipt = checked_publication::submit(&policy, &request);
    // Candidate diagnostics remain the real gate's pinned-LEAN report, not a
    // second graph-error implementation or a semantic PASS.
    if let Some(report) = &receipt.validation
        && report.status != "current"
    {
        print!("{}", report.lean()?);
    } else if receipt.status != "current" {
        print!("{}", receipt.lean()?);
    } else {
        serde_json::to_writer(io::stdout().lock(), &receipt)?;
        println!();
    }
    io::stdout().flush()?;
    Ok(receipt.status == "current")
}
fn main() {
    match run() {
        Ok(true) => (),
        Ok(false) => std::process::exit(1),
        Err(_) => {
            eprintln!("git submission unavailable: invalid request, repository or checked policy");
            std::process::exit(1);
        }
    }
}
