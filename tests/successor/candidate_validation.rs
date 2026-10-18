use crate::{
    Policy, Store,
    candidate_validation::{self as gate, PauseBeforeAccept, Request},
    index::{self, Mode},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
fn id(n: usize) -> String {
    format!("40000000-0000-4000-8000-{n:012}")
}
fn file(root: &Path, n: usize) -> PathBuf {
    root.join(format!("nodes/{}/node.md", id(n)))
}
fn node(root: &Path, n: usize, fields: Value, body: &str) {
    let mut fields = fields.as_object().unwrap().clone();
    fields.insert("id".into(), json!(id(n)));
    let mut text = String::from("---\n");
    for (k, v) in fields {
        text.push_str(&format!("{k}: {v}\n"));
    }
    text.push_str("---\n");
    text.push_str(body);
    fs::write(file(root, n), text).unwrap();
}
fn fields() -> Value {
    json!({"name":"Node","micro_summary":"Summary","see_also":[],"status":"in_progress"})
}
fn fixture(root: &Path) {
    fs::create_dir_all(root.join("trees")).unwrap();
    for n in 1..=2 {
        let dir = root.join("nodes").join(id(n));
        fs::create_dir_all(dir.join("children")).unwrap();
        fs::write(dir.join("children.order"), "").unwrap();
        node(root, n, fields(), "Body");
        std::os::unix::fs::symlink(
            format!("../nodes/{}", id(n)),
            root.join("trees").join(id(n)),
        )
        .unwrap();
    }
}
fn request(root: &Path) -> Request {
    Request {
        store: root.into(),
        before: None,
        explicit: BTreeSet::new(),
        expected_version: None,
        pause_before_accept: None,
    }
}
fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}
fn passed(root: &Path) -> gate::Report {
    let r = gate::validate(&policy(), &request(root));
    assert_eq!(r.status, "current", "{}", r.lean().unwrap());
    r
}
fn rejected(root: &Path) -> gate::Report {
    let r = gate::validate(&policy(), &request(root));
    assert_ne!(r.status, "current");
    let output = r.lean().unwrap();
    assert!(output.contains("rule"));
    for field in [
        "graph_mode",
        "graph_reason",
        "graph_base_version",
        "graph_candidate_version",
    ] {
        assert!(
            !output.contains(field),
            "ordinary report leaked {field}: {output}"
        );
    }
    r
}

#[test]
fn candidate_fields_references_and_graph_negatives_preserve_drafts() {
    let t = tempfile::tempdir().unwrap();
    fixture(t.path());
    passed(t.path());
    let cases = [
        json!({"name":" "}),
        json!({"micro_summary":"\t"}),
        json!({"status":"bogus"}),
        json!({"status":"stuck"}),
        json!({"status":"stuck","blocker":" "}),
        json!({"status":"deferred","deferred_reason":null}),
        json!({"status":"discarded"}),
        json!({"ruled_out":""}),
        json!({"ruled_out":null}),
        json!({"scope":4}),
        json!({"see_also":[{"target":id(1),"reason":"Self"}]}),
        json!({"see_also":[{"target":id(2),"reason":"A"},{"target":id(2),"reason":"B"}]}),
        json!({"see_also":[{"target":id(2),"reason":" "}]}),
        json!({"see_also":[{"target":id(3),"reason":"Missing"}]}),
    ];
    for patch in cases {
        let mut f = fields();
        for (k, v) in patch.as_object().unwrap() {
            f[k] = v.clone();
        }
        node(t.path(), 1, f, "Body");
        let before = fs::read(file(t.path(), 1)).unwrap();
        rejected(t.path());
        assert_eq!(fs::read(file(t.path(), 1)).unwrap(), before);
    }
    let mut f = fields();
    f["status"] = json!("resolved");
    node(t.path(), 1, f.clone(), " \n\t");
    rejected(t.path());
    node(t.path(), 1, f, "Answer");
    passed(t.path());
    for (s, k) in [
        ("stuck", "blocker"),
        ("deferred", "deferred_reason"),
        ("discarded", "discard_reason"),
    ] {
        let mut f = fields();
        f["status"] = json!(s);
        f[k] = json!("Reason");
        node(t.path(), 1, f, "");
        passed(t.path());
    }
    node(t.path(), 1, fields(), "Body");
    fs::write(
        t.path().join(format!("nodes/{}/children.order", id(1))),
        format!("{}\n", id(2)),
    )
    .unwrap();
    rejected(t.path());
    std::os::unix::fs::symlink(
        format!("../../{}", id(2)),
        t.path().join(format!("nodes/{}/children/{}", id(1), id(2))),
    )
    .unwrap();
    rejected(t.path()); // both root and child
    fs::remove_file(t.path().join("trees").join(id(2))).unwrap();
    passed(t.path());
    let mut f = fields();
    f["status"] = json!("resolved");
    node(t.path(), 1, f, "Answer");
    rejected(t.path());
}
#[test]
fn candidate_ceiling_is_body_only_complete_and_index_version_compatible() {
    let t = tempfile::tempdir().unwrap();
    fixture(t.path());
    // Independent application fixture expectation: N repetitions of ordinary " a"
    // are N o200k tokens. Never derive the boundary from candidate measurements.
    node(t.path(), 1, fields(), &" a".repeat(5000));
    let r = passed(t.path());
    assert_eq!(r.rows[0].tokens, Some(5000));
    node(t.path(), 1, fields(), &" a".repeat(5001));
    let r = rejected(t.path());
    assert!(
        r.rows
            .iter()
            .any(|r| r.check == "crystallization_over_limit"
                && r.tokens == Some(5001)
                && r.id == Some(id(1)))
    );
    node(t.path(), 2, fields(), "Ordinary edit elsewhere");
    rejected(t.path()); // unchanged offender
    let mut f = fields();
    f["scope"] = json!(" a".repeat(6000));
    f["warnings"] = json!(" a".repeat(6000));
    node(t.path(), 1, f, "Body");
    let r = passed(t.path());
    fs::write(
        t.path().join(format!("nodes/{}/node.meta.json", id(1))),
        " a".repeat(6000),
    )
    .unwrap();
    assert_eq!(passed(t.path()).version, r.version);
    let view = index::refresh(&policy(), t.path(), None, Mode::Full, None).unwrap();
    assert_eq!(view.version, r.version);
    assert_eq!(passed(t.path()).version, r.version);
}
#[test]
fn candidate_transition_noop_edit_activation_explicit_and_departing_reason() {
    let t = tempfile::tempdir().unwrap();
    let b = t.path().join("before");
    let a = t.path().join("after");
    fixture(&b);
    fixture(&a);
    let mut f = fields();
    f["status"] = json!("not_examined");
    node(&b, 1, f.clone(), "Body");
    node(&a, 1, f.clone(), "Body");
    let mut q = request(&a);
    q.before = Some(b.clone());
    assert_eq!(gate::validate(&policy(), &q).status, "current");
    node(&a, 1, f.clone(), "Body edited");
    assert_eq!(gate::validate(&policy(), &q).status, "rejected");
    f["status"] = json!("in_progress");
    node(&a, 1, f.clone(), "Body edited");
    assert_eq!(gate::validate(&policy(), &q).status, "current");
    f["status"] = json!("resolved");
    node(&a, 1, f, "Answer");
    assert_eq!(gate::validate(&policy(), &q).status, "rejected");
    q.explicit.insert(id(1));
    assert_eq!(gate::validate(&policy(), &q).status, "current");
    let mut f = fields();
    f["status"] = json!("stuck");
    f["blocker"] = json!("Blocked");
    node(&b, 1, f.clone(), "Body");
    f["status"] = json!("in_progress");
    node(&a, 1, f, "Body");
    assert_eq!(gate::validate(&policy(), &q).status, "rejected");
    node(&a, 1, fields(), "Body");
    assert_eq!(gate::validate(&policy(), &q).status, "current");
}
#[test]
fn candidate_receipts_change_and_deterministic_source_candidate_races_reject() {
    let t = tempfile::tempdir().unwrap();
    let b = t.path().join("before");
    let a = t.path().join("after");
    fixture(&b);
    fixture(&a);
    let version = passed(&a).version;
    let mut q = request(&a);
    q.expected_version = version;
    assert_eq!(gate::validate(&policy(), &q).status, "current");
    node(&a, 2, fields(), "New");
    assert_eq!(gate::validate(&policy(), &q).status, "rejected");
    for (i, which) in [&a, &b].into_iter().enumerate() {
        let mut q = request(&a);
        q.before = Some(b.clone());
        let ready = t.path().join(format!("ready-{i}"));
        let release = t.path().join(format!("release-{i}"));
        q.pause_before_accept = Some(PauseBeforeAccept {
            ready: ready.clone(),
            release: release.clone(),
        });
        let target = file(which, 1);
        let h = thread::spawn(move || gate::validate(&policy(), &q));
        let start = Instant::now();
        while !ready.exists() {
            assert!(start.elapsed() < Duration::from_secs(10));
            thread::sleep(Duration::from_millis(5));
        }
        // Another native consumer must remain usable while acceptance is paused.
        assert_eq!(crate::outline_artifacts::cost("Body").1, 1);
        let bytes = fs::read(&target).unwrap();
        let temp = target.with_extension("replacement");
        fs::write(&temp, &bytes).unwrap();
        fs::rename(&temp, &target).unwrap();
        fs::write(release, b"release").unwrap();
        assert_eq!(h.join().unwrap().status, "stale");
    }
    let s = Store::read(&a).unwrap();
    assert_eq!(gate::version(&s), passed(&a).version.unwrap());
}

#[test]
#[ignore = "timing probe; run alone with --ignored --exact --nocapture"]
fn repeated_candidate_validation_timing() {
    let t = tempfile::tempdir().unwrap();
    fixture(t.path());
    node(t.path(), 1, fields(), &" a".repeat(5000));
    let p = policy();
    let q = request(t.path());
    let mut samples = Vec::new();
    let mut observation = None;
    for _ in 0..9 {
        let start = Instant::now();
        let r = gate::validate(&p, &q);
        samples.push(start.elapsed().as_secs_f64());
        assert_eq!(r.status, "current", "{}", r.lean().unwrap());
        assert_eq!(r.nodes, 2);
        assert_eq!(r.rows[0].tokens, Some(5000));
        assert_eq!(r.rows[1].tokens, Some(1));
        let value = serde_json::to_value(&r).unwrap();
        if let Some(previous) = &observation {
            assert_eq!(previous, &value);
        }
        observation = Some(value);
    }
    println!(
        "TIMING {}",
        json!({"seconds":samples, "observation":observation})
    );
}
