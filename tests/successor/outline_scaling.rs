use crate::{
    outline_artifacts::{self, ForestRequest, Request},
    outline_rows::{self, Coverage},
};
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
fn shared_full_costs_equal_individual_assembly_and_reject_off_owner_invalidity() {
    let nodes = chain(12);
    let forest = outline_artifacts::assemble_forest(&ForestRequest {
        source_version: "golden-v1".into(),
        nodes: nodes.clone(),
    })
    .unwrap();
    for i in 0..12 {
        let single = outline_artifacts::assemble(&Request {
            owner: id(i),
            source_version: "golden-v1".into(),
            nodes: nodes.clone(),
        })
        .unwrap();
        assert_eq!(forest["owners"][id(i)], single);
        for j in (i + 1)..=(i + 1).min(11) {
            assert_eq!(
                single["shallow"]["meta"]["branch_full_tokens"][id(j)],
                forest["owners"][id(j)]["full"]["meta"]["tokens"]
            );
        }
    }
    let mut bad = nodes.clone();
    bad.push(json!({"id":id(90),"name":"Off owner","micro_summary":" ","status":"in_progress","children":[]}));
    assert!(
        outline_artifacts::assemble(&Request {
            owner: id(0),
            source_version: "v".into(),
            nodes: bad
        })
        .is_err()
    );
    let mut cycle = nodes;
    cycle.last_mut().unwrap()["children"] = json!([id(0)]);
    assert!(
        outline_artifacts::assemble_forest(&ForestRequest {
            source_version: "v".into(),
            nodes: cycle
        })
        .is_err()
    );
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
