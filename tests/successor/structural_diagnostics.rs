use crate::{
    Policy, Store,
    candidate_validation::{self as gate, Request},
};
use serde_json::json;
use std::{collections::BTreeSet, fs, path::Path};

fn id(n: usize) -> String {
    format!("61000000-0000-4000-8000-{n:012}")
}
fn fixture(root: &Path) {
    fs::create_dir_all(root.join("trees")).unwrap();
    for n in 1..=3 {
        let dir = root.join("nodes").join(id(n));
        fs::create_dir_all(dir.join("children")).unwrap();
        fs::write(dir.join("children.order"), "").unwrap();
        write(root, n, None);
        std::os::unix::fs::symlink(
            format!("../nodes/{}", id(n)),
            root.join("trees").join(id(n)),
        )
        .unwrap();
    }
}
fn write(root: &Path, n: usize, target: Option<usize>) {
    let refs = target
        .map(|t| vec![json!({"target":id(t),"reason":"PRIVATE reason"})])
        .unwrap_or_default();
    let text = format!(
        "---\nid: {}\nname: \"PRIVATE name\"\nmicro_summary: \"Summary\"\nstatus: \"in_progress\"\nsee_also: {}\n---\nPRIVATE body",
        json!(id(n)),
        json!(refs)
    );
    fs::write(root.join(format!("nodes/{}/node.md", id(n))), text).unwrap();
}
fn edge(root: &Path, parent: usize, child: usize) {
    std::os::unix::fs::symlink(
        format!("../../{}", id(child)),
        root.join(format!("nodes/{}/children/{}", id(parent), id(child))),
    )
    .unwrap();
    fs::write(
        root.join(format!("nodes/{}/children.order", id(parent))),
        format!("{}\n", id(child)),
    )
    .unwrap();
}
fn report(root: &Path) -> gate::Report {
    gate::validate(
        &Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap(),
        &Request {
            store: root.into(),
            before: None,
            explicit: BTreeSet::new(),
            expected_version: None,
            pause_before_accept: None,
        },
    )
}
fn issue(r: &gate::Report, source: Option<usize>, target: Option<usize>, law: &str) {
    assert_eq!(r.status, "rejected");
    assert!(
        r.rows.iter().any(|row| row.source == source.map(id)
            && row.target == target.map(id)
            && row.law == Some(law)),
        "{}",
        r.lean().unwrap()
    );
    let lean = r.lean().unwrap();
    assert!(!lean.contains("PRIVATE"));
}
#[test]
fn dangling_multiple_sources_are_real_bend_rejections_with_exact_identities() {
    let t = tempfile::tempdir().unwrap();
    fixture(t.path());
    write(t.path(), 1, Some(2));
    write(t.path(), 3, Some(2));
    assert_eq!(report(t.path()).status, "current");
    fs::remove_file(t.path().join("trees").join(id(2))).unwrap();
    fs::remove_dir_all(t.path().join("nodes").join(id(2))).unwrap();
    let store = Store::read(t.path()).unwrap();
    assert!(
        !Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR")))
            .unwrap()
            .graph_valid(&store)
            .unwrap()
    );
    let r = report(t.path());
    issue(&r, Some(1), Some(2), "laws/Spec.bend:refs_valid");
    issue(&r, Some(3), Some(2), "laws/Spec.bend:refs_valid");
    assert_eq!(r.rows.len(), 2);
}
#[test]
fn placement_cycle_and_native_order_guards_name_only_established_identities() {
    let t = tempfile::tempdir().unwrap();
    fixture(t.path());
    edge(t.path(), 1, 2);
    issue(
        &report(t.path()),
        Some(1),
        Some(2),
        "laws/Spec.bend:nodes_valid",
    );
    fs::remove_file(t.path().join("trees").join(id(2))).unwrap();
    assert_eq!(report(t.path()).status, "current");
    edge(t.path(), 3, 2);
    issue(
        &report(t.path()),
        Some(3),
        Some(2),
        "laws/Spec.bend:nodes_valid",
    );
    fs::remove_file(t.path().join(format!("nodes/{}/children/{}", id(3), id(2)))).unwrap();
    fs::write(t.path().join(format!("nodes/{}/children.order", id(3))), "").unwrap();
    fs::remove_file(t.path().join("trees").join(id(1))).unwrap();
    edge(t.path(), 2, 1);
    issue(&report(t.path()), Some(1), Some(2), "laws/Spec.bend:rooted");
    fs::write(
        t.path().join(format!("nodes/{}/children.order", id(1))),
        format!("{}\n{}\n", id(2), id(2)),
    )
    .unwrap();
    issue(
        &report(t.path()),
        Some(1),
        Some(2),
        "src/lib.rs:Store.read_view/order_membership",
    );
    fs::write(
        t.path().join(format!("nodes/{}/children.order", id(1))),
        "secret-not-an-id\n",
    )
    .unwrap();
    issue(
        &report(t.path()),
        Some(1),
        None,
        "src/lib.rs:Store.read_view/order_uuid",
    );
}
#[test]
fn absent_roots_and_children_have_honest_native_not_bend_predicates() {
    let t = tempfile::tempdir().unwrap();
    fixture(t.path());
    edge(t.path(), 1, 4);
    issue(
        &report(t.path()),
        Some(1),
        Some(4),
        "src/lib.rs:Store.read_view/child_membership",
    );
    fs::remove_file(t.path().join(format!("nodes/{}/children/{}", id(1), id(4)))).unwrap();
    fs::write(t.path().join(format!("nodes/{}/children.order", id(1))), "").unwrap();
    std::os::unix::fs::symlink(
        format!("../nodes/{}", id(4)),
        t.path().join("trees").join(id(4)),
    )
    .unwrap();
    issue(
        &report(t.path()),
        None,
        Some(4),
        "src/lib.rs:Store.read_view/root_membership",
    );
}
#[test]
fn unknown_malformed_bytes_do_not_invent_target_or_law() {
    let t = tempfile::tempdir().unwrap();
    fixture(t.path());
    fs::write(
        t.path().join(format!("nodes/{}/node.md", id(1))),
        "PRIVATE malformed secret",
    )
    .unwrap();
    let r = report(t.path());
    assert_eq!(r.status, "rejected");
    assert!(
        r.rows
            .iter()
            .all(|r| r.source.is_none() && r.target.is_none() && r.law.is_none())
    );
    assert!(!r.lean().unwrap().contains("PRIVATE"));
}
