use crate::outline_rows::{self, Coverage};
use serde_json::{Value, json};

fn id(n: usize) -> String {
    format!("81000000-0000-4000-8000-{n:012}")
}
fn chain(n: usize) -> Vec<Value> {
    (0..n)
        .rev()
        .map(|i| {
            json!({"id":id(i),"name":format!("Node {i}"),"micro_summary":format!("Summary {i}"),
        "status":"in_progress","children": if i + 1 < n {vec![id(i+1)]} else {vec![]}})
        })
        .collect()
}
#[test]
fn deep_projection_uses_exact_preorder_ranges_without_recursion() {
    let nodes = serde_json::from_value(json!(chain(10_000))).unwrap();
    let request = outline_rows::Request {
        owner: id(0),
        coverage: Coverage::Full,
        source_version: "deep".into(),
        nodes,
    };
    let full = outline_rows::project(&request).unwrap();
    assert_eq!(full.rows.len(), 10_000);
    for (i, row) in full.rows.iter().enumerate() {
        assert_eq!(row.id, id(i));
        assert_eq!(row.depth, i);
        assert_eq!(row.descendants, 9_999 - i);
    }
    let request = outline_rows::Request {
        coverage: Coverage::Shallow,
        ..request
    };
    let shallow = outline_rows::project(&request).unwrap();
    assert_eq!(shallow.rows.len(), 2);
    assert_eq!(shallow.rows[1].descendants, 9_998);
}
#[test]
fn closed_stage_paths_still_reject_same_bytes_replacement() {
    use std::{collections::BTreeMap, fs};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join(".sifttext")).unwrap();
    let store = crate::Store {
        path: root.to_owned(),
        nodes: BTreeMap::new(),
        children: BTreeMap::new(),
        roots: vec![],
        inventory: BTreeMap::new(),
    };
    let publication = crate::outlines::Publication::prepare(&store, &chain(1), "v").unwrap();
    publication.unchanged(root).unwrap();
    let staged = fs::read_dir(root.join(".sifttext/outline-stage"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let bytes = fs::read(&staged).unwrap();
    let replacement = staged.with_extension("replacement");
    fs::write(&replacement, bytes).unwrap();
    fs::rename(replacement, staged).unwrap();
    assert!(publication.unchanged(root).is_err());
    assert!(publication.publish(root).is_err());
}
