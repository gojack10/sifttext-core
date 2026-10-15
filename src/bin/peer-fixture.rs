// tests/peer_fixture.rs exercises this binary; do not duplicate its native demo here.
#![cfg(not(test))]

#[path = "../../fixtures/check_fixture.rs"]
mod fixture_contract;
#[path = "../../fixtures/peer_demo.rs"]
mod peer_demo;

use serde_json::Value;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn expected(project: &Path, name: &str) -> Value {
    serde_json::from_slice(
        &fs::read(
            project
                .join("fixtures/expected")
                .join(format!("{name}.json")),
        )
        .expect("read expected fixture"),
    )
    .expect("parse expected fixture")
}

// Construct authored draft bytes without calling Core's mutation API or using
// the checker's expected-result file. The existing materializer fences creation.
fn manual_draft(project: &Path, store: &Path, case: &str) {
    let mut fixture: Value =
        serde_json::from_slice(&fs::read(project.join("fixtures/fixture-contract.json")).unwrap())
            .unwrap();
    let labels = fixture["labels"].clone();
    for node in fixture["nodes"].as_array_mut().unwrap() {
        match case {
            "move-b-under-a" if node["id"] == labels["R"] => {
                node["children"] = serde_json::json!([labels["A"], labels["C"]]);
            }
            "move-b-under-a" if node["id"] == labels["A"] => {
                node["children"] = serde_json::json!([labels["B"]]);
            }
            "rename-b" if node["id"] == labels["B"] => {
                node["name"] = serde_json::json!("Fixture Node B Renamed");
            }
            _ => {}
        }
    }
    peer_demo::materialize(store, &fixture);
}

fn run() {
    let mut args = env::args_os().skip(1);
    let command = args.next().and_then(|s| s.into_string().ok());
    let store = args.next().map(PathBuf::from);
    let case = args.next().and_then(|s| s.into_string().ok());
    assert!(args.next().is_none(), "unexpected arguments");

    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fixture_contract::check_fixture(&project);
    let store = store.expect("usage: peer-fixture materialize NEW_STORE | check STORE CASE");
    let store = env::current_dir().unwrap().join(store);
    let bin = env::var_os("SIFTTEXT_PROJECT")
        .map(|p| PathBuf::from(p).join("target/debug/sifttext-file-graph"))
        .unwrap_or_else(|| {
            env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .join("sifttext-file-graph")
        });

    match command.as_deref() {
        Some("materialize") if case.is_none() => {
            let fixture: Value = serde_json::from_slice(
                &fs::read(project.join("fixtures/fixture-contract.json")).expect("read fixture"),
            )
            .expect("parse fixture");
            peer_demo::materialize(&store, &fixture);
            let store = store.canonicalize().unwrap();
            peer_demo::check(&bin, &store, &expected(&project, "before"));
            println!("Materialized and checked: {}", store.display());
        }
        Some("check")
            if case.as_deref().is_some_and(|n| {
                matches!(
                    n,
                    "before" | "move-b-under-a" | "reorder-b-after-c" | "rename-b"
                )
            }) =>
        {
            let case = case.unwrap();
            let store = store.canonicalize().unwrap();
            peer_demo::check(&bin, &store, &expected(&project, &case));
            println!("PASS {case}: {}", store.display());
        }
        Some("manual-draft") if matches!(case.as_deref(), Some("move-b-under-a" | "rename-b")) => {
            manual_draft(&project, &store, case.as_deref().unwrap());
            println!("Manual draft {}: {}", case.unwrap(), store.display());
        }
        _ => panic!(
            "usage: peer-fixture materialize NEW_STORE | check STORE CASE | manual-draft NEW_STORE move-b-under-a|rename-b"
        ),
    }
}

fn main() {
    run();
}
