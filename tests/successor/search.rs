use crate::{
    Policy, Store, index,
    search::{self, Ranking, Request},
};
use serde_json::json;
use std::{collections::BTreeSet, fs, path::Path};

fn id(n: usize) -> String {
    format!("40000000-0000-4000-8000-{n:012}")
}
fn fixture() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir(d.path().join("trees")).unwrap();
    for n in 1..=4 {
        let p = d.path().join("nodes").join(id(n));
        fs::create_dir_all(p.join("children")).unwrap();
        let fields = json!({"id":id(n),"name":format!("Login {n}"),"micro_summary":"Reset guidance",
            "status":if n==3 {"stuck"} else {"in_progress"},"blocker":"Exact reason Ω",
            "warnings":"Exact warning\nΩ","see_also":[],"scope":"Reset scope"});
        fs::write(
            p.join("node.md"),
            format!(
                "---\n{}---\nLogin reset. Exact body.\n",
                fields
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}\n"))
                    .collect::<String>()
            ),
        )
        .unwrap();
        let children = match n {
            1 => vec![2, 3],
            2 => vec![4],
            _ => vec![],
        };
        fs::write(
            p.join("children.order"),
            children.iter().map(|n| id(*n) + "\n").collect::<String>(),
        )
        .unwrap();
        for c in children {
            std::os::unix::fs::symlink(format!("../../{}", id(c)), p.join("children").join(id(c)))
                .unwrap();
        }
    }
    std::os::unix::fs::symlink(
        format!("../nodes/{}", id(1)),
        d.path().join("trees").join(id(1)),
    )
    .unwrap();
    d
}
fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}
fn run(p: &Path, query: &str, scope: Option<&str>, readable: &BTreeSet<String>) -> search::Outcome {
    search::search(
        &policy(),
        &Request {
            store: p,
            base_store: None,
            query,
            under_node_id: scope,
            readable_ids: readable,
            limit: 50,
            offset: 0,
            ranking: &Ranking::default(),
            pause_before_accept: None,
        },
    )
    .unwrap()
}
#[test]
fn public_hits_use_same_version_guardrails_and_canonical_provenance() {
    let d = fixture();
    let p = d.path();
    let readable = (1..=4).map(id).collect();
    let authored = Store::read(p).unwrap();
    let out = run(p, "login", Some(&id(3)), &readable);
    assert_eq!(out.status, "current");
    assert_eq!(out.total_matches, 1);
    let h = &out.hits[0];
    assert_eq!(h.id, id(3));
    assert_eq!(h.name, "Login 3");
    assert_eq!(h.micro_summary, "Reset guidance");
    assert_eq!(h.active_reason.as_deref(), Some("Exact reason Ω"));
    assert_eq!(h.warnings.as_deref(), Some("Exact warning\nΩ"));
    assert_eq!(
        h.path,
        fs::canonicalize(p)
            .unwrap()
            .join("nodes")
            .join(id(3))
            .join("node.md")
    );
    assert!(h.matching_excerpt.is_some());
    let v = serde_json::to_value(h).unwrap();
    assert!(v.get("ruled_out").is_none());
    assert_eq!(
        Store::read(p)
            .unwrap()
            .inventory
            .iter()
            .filter(|(k, _)| !k.starts_with(".sifttext"))
            .collect::<Vec<_>>(),
        authored
            .inventory
            .iter()
            .filter(|(k, _)| !k.starts_with(".sifttext"))
            .collect::<Vec<_>>()
    );
    let view = index::refresh(&policy(), p, None, index::Mode::Incremental, None).unwrap();
    assert_eq!(view.version, out.version);
    let source = p.join("nodes").join(id(3)).join("node.md");
    let original_text = fs::read_to_string(&source).unwrap();
    for (status, key) in [
        ("deferred", "deferred_reason"),
        ("discarded", "discard_reason"),
    ] {
        let text = original_text
            .replace("status: \"stuck\"", &format!("status: \"{status}\""))
            .replace("blocker:", &format!("{key}:"));
        fs::write(&source, text).unwrap();
        let updated = run(p, "login", Some(&id(3)), &readable);
        assert_eq!(updated.hits[0].status, status);
        assert_eq!(
            updated.hits[0].active_reason.as_deref(),
            Some("Exact reason Ω")
        );
    }
    fs::write(&source, original_text).unwrap();
    assert!(run(p, "missing", Some(&id(3)), &readable).hits.is_empty());
    assert_eq!(run(p, "login", Some(&id(2)), &readable).total_matches, 2);
    assert!(run(p, "login", None, &BTreeSet::new()).hits.is_empty());
    assert!(
        run(p, "login", None, &readable)
            .hits
            .iter()
            .filter(|h| h.status != "stuck")
            .all(|h| h.active_reason.is_none())
    );
}
#[test]
fn permission_precedes_offset_and_invalid_input_never_reuses_old_hits() {
    let d = fixture();
    let p = d.path();
    let readable = BTreeSet::from([id(4)]);
    assert_eq!(run(p, "login", None, &readable).hits[0].id, id(4));
    let out = search::search(
        &policy(),
        &Request {
            store: p,
            base_store: None,
            query: "login",
            under_node_id: None,
            readable_ids: &readable,
            limit: 1,
            offset: 1,
            ranking: &Ranking::default(),
            pause_before_accept: None,
        },
    )
    .unwrap();
    assert_eq!(out.total_matches, 1);
    assert!(out.hits.is_empty());
    let f = p.join("nodes").join(id(4)).join("node.md");
    let text = fs::read(&f).unwrap();
    fs::write(&f, "invalid draft").unwrap();
    let bad = run(p, "login", None, &readable);
    assert_eq!(bad.status, "error");
    assert!(bad.hits.is_empty());
    assert!(bad.version.is_none());
    fs::write(&f, text).unwrap();
    assert_eq!(run(p, "login", None, &readable).status, "current");
    assert!(
        search::search(
            &policy(),
            &Request {
                store: p,
                base_store: None,
                query: "login",
                under_node_id: None,
                readable_ids: &readable,
                limit: 1,
                offset: usize::MAX,
                ranking: &Ranking::default(),
                pause_before_accept: None
            }
        )
        .is_err()
    );
}
