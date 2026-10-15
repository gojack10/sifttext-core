use sifttext_file_graph::{Operation, Policy, Result, Store, apply, validate_draft};
use std::path::Path;

fn operation(s: &Store, args: &[String]) -> Result<Operation> {
    match args {
        [kind, node, arg] => {
            let node = s.resolve(node)?;
            match kind.as_str() {
                "move" => Ok(Operation::Move {
                    node,
                    parent: s.resolve(arg)?,
                }),
                "reorder" => Ok(Operation::Reorder {
                    node,
                    after: s.resolve(arg)?,
                }),
                "rename" => Ok(Operation::Rename {
                    node,
                    name: arg.clone(),
                }),
                _ => Err("expected move, reorder or rename".into()),
            }
        }
        _ => Err("operation requires KIND NODE ARG".into()),
    }
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR")))?;
    match args.as_slice() {
        [kind, request] if kind == "validate-candidate" => {
            let request: sifttext_file_graph::candidate_validation::Request =
                serde_json::from_slice(&std::fs::read(request)?)?;
            let report = sifttext_file_graph::candidate_validation::validate(&policy, &request);
            println!("{}", report.lean()?);
            if report.status != "current" { return Err("candidate validation did not pass".into()); }
        }
        [kind, path] if kind == "validate" || kind == "inspect" => {
            let store = Store::read(Path::new(path))?;
            policy.validate(&store)?;
            if kind == "inspect" { println!("{}", serde_json::to_string_pretty(&store.view())?); }
            else { println!("valid private draft (not publication approval)"); }
        }
        [kind, path, node, arg] if ["move", "reorder", "rename"].contains(&kind.as_str()) => {
            let before = Store::read(Path::new(path))?;
            let op = operation(&before, &[kind.clone(), node.clone(), arg.clone()])?;
            let after = apply(&policy, &before, &op)?;
            println!("{}", serde_json::to_string_pretty(&after.view())?);
        }
        [cmd, before, after, tail @ ..] if cmd == "validate-draft" => {
            let before = Store::read(Path::new(before))?;
            let after = Store::read(Path::new(after))?;
            let op = operation(&before, tail)?;
            validate_draft(&policy, &before, &after, &op)?;
            println!("native draft exactly matches checked operation");
        }
        _ => return Err("usage: sifttext-file-graph validate-candidate REQUEST.json; validate|inspect STORE (graph-only); move|reorder|rename STORE NODE ARG; validate-draft BEFORE AFTER KIND NODE ARG".into()),
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("rejected: {e}");
        std::process::exit(1);
    }
}
