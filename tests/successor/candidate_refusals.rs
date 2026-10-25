//! Synthetic same-call failure controls; no historical-failure diagnosis.
use crate::{
    Policy,
    candidate_validation::{self as gate, Failure, PauseBeforeAccept, Request},
};
use std::{
    collections::BTreeSet,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

const ID: &str = "81000000-0000-4000-8000-000000000001";
const SECRET: &str = "PRIVATE_CANDIDATE_SENTINEL";
fn file(p: &Path) -> PathBuf {
    p.join(format!("nodes/{ID}/node.md"))
}
fn node(p: &Path, status: &str, body: &str) {
    fs::write(file(p), format!("---\nid: \"{ID}\"\nname: \"Node\"\nmicro_summary: \"Summary\"\nstatus: \"{status}\"\nsee_also: []\n---\n{body}")).unwrap();
}
fn fixture(p: &Path) {
    fs::create_dir_all(p.join("trees")).unwrap();
    fs::create_dir_all(p.join(format!("nodes/{ID}/children"))).unwrap();
    fs::write(p.join(format!("nodes/{ID}/children.order")), "").unwrap();
    node(p, "in_progress", SECRET);
    std::os::unix::fs::symlink(format!("../nodes/{ID}"), p.join("trees").join(ID)).unwrap();
}
fn request(p: &Path) -> Request {
    Request {
        store: p.into(),
        before: None,
        explicit: BTreeSet::new(),
        expected_version: None,
        pause_before_accept: None,
    }
}
fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}
fn refused(r: &gate::Report, stage: &'static str, kind: &'static str) {
    assert_ne!(r.status, "current", "{r:?}");
    assert_eq!(r.failure, Some(Failure { stage, kind }), "{r:?}");
    for text in [serde_json::to_string(r).unwrap(), r.lean().unwrap()] {
        assert!(!text.contains(SECRET));
        assert!(!text.contains("private-path"));
    }
}
// Controlled stamped helper projects exercise the actual checked helper path.
// They never replace the real project's compiled Bend or grant production authority.
fn helper_project(p: &Path, body: &str) -> Policy {
    fs::create_dir_all(p.join("build")).unwrap();
    let mut manifest = String::new();
    for name in ["Cargo.toml", "Cargo.lock", "README.md", ".gitignore"] {
        fs::write(p.join(name), "synthetic control\n").unwrap();
        manifest.push_str(&format!(
            "{}  {name}\n",
            crate::sha256(b"synthetic control\n")
        ));
    }
    fs::write(p.join("build/candidate.sha256"), manifest).unwrap();
    let script = format!("#!/bin/sh\n{body}\n");
    let executable = p.join("build/graph-policy");
    fs::write(&executable, &script).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        p.join("build/graph-policy.sha256"),
        format!("{}  build/graph-policy\n", crate::sha256(script.as_bytes())),
    )
    .unwrap();
    Policy::checked(p).unwrap()
}
#[test]
fn candidate_refusals_valid_and_content_rejections_preserve_exact_source() {
    let t = tempfile::tempdir().unwrap();
    fixture(t.path());
    let q = request(t.path());
    let p = policy();
    let valid = gate::validate(&p, &q);
    assert_eq!(valid.status, "current");
    assert!(valid.failure.is_none());
    assert!(
        serde_json::to_value(&valid)
            .unwrap()
            .get("failure")
            .is_none()
    );
    assert!(!valid.lean().unwrap().contains("failure_kind"));
    for (status, body, check) in [
        ("resolved", " ", "resolved_body_blank"),
        ("stuck", SECRET, "status_transition_or_reason"),
        ("bogus", SECRET, "status_or_reason_type"),
    ] {
        node(t.path(), status, body);
        let bytes = fs::read(file(t.path())).unwrap();
        let r = gate::validate(&p, &q);
        refused(&r, "candidate", "invalid_candidate");
        assert!(r.rows.iter().any(|r| r.check == check));
        assert_eq!(fs::read(file(t.path())).unwrap(), bytes);
    }
    node(t.path(), "in_progress", &" a".repeat(5000));
    let r = gate::validate(&p, &q);
    assert_eq!(r.status, "current");
    assert_eq!(r.rows[0].tokens, Some(5000));
    node(t.path(), "in_progress", &" a".repeat(5001));
    let r = gate::validate(&p, &q);
    refused(&r, "candidate", "invalid_candidate");
    assert!(
        r.rows
            .iter()
            .any(|r| r.check == "content_over_limit" && r.tokens == Some(5001))
    );
    node(t.path(), "in_progress", SECRET);
    // A complete capture with no placement is rejected by actual compiled Bend.
    fs::remove_file(t.path().join("trees").join(ID)).unwrap();
    refused(&gate::validate(&p, &q), "candidate", "invalid_candidate");
    std::os::unix::fs::symlink(format!("../nodes/{ID}"), t.path().join("trees").join(ID)).unwrap();
    assert_eq!(gate::validate(&p, &q).version, valid.version);
}
#[test]
fn candidate_refusals_capture_missing_invalid_and_unknown_are_not_timeouts() {
    let t = tempfile::tempdir().unwrap();
    let p = policy();
    let absent = t.path().join("private-path");
    refused(
        &gate::validate(&p, &request(&absent)),
        "candidate_capture",
        "not_found",
    );
    fixture(&absent);
    let saved = fs::read(file(&absent)).unwrap();
    fs::write(file(&absent), format!("{SECRET}\n")).unwrap();
    refused(
        &gate::validate(&p, &request(&absent)),
        "candidate_capture",
        "invalid_capture",
    );
    fs::write(file(&absent), &saved).unwrap();
    fs::hard_link(file(&absent), t.path().join("hard-link")).unwrap();
    refused(
        &gate::validate(&p, &request(&absent)),
        "candidate_capture",
        "unsafe_filesystem",
    );
    fs::remove_file(t.path().join("hard-link")).unwrap();
    let mut q = request(&absent);
    q.before = Some(t.path().join("missing-before"));
    refused(&gate::validate(&p, &q), "before_capture", "not_found");
    assert_eq!(gate::validate(&p, &request(&absent)).status, "current");
    assert_eq!(fs::read(file(&absent)).unwrap(), saved);
}
#[test]
fn candidate_refusals_checked_helper_nonzero_truncated_forged_and_missing() {
    let t = tempfile::tempdir().unwrap();
    let store = t.path().join("private-path");
    fixture(&store);
    let original = fs::read(file(&store)).unwrap();
    for (n, script, kind) in [
        (
            0,
            "printf PRIVATE_CANDIDATE_SENTINEL >&2; exit 7",
            "helper_nonzero",
        ),
        (1, "printf 'STG1 VAL'", "invalid_helper_response"),
        (
            2,
            "printf 'policy process timed out'",
            "invalid_helper_response",
        ),
        (3, "printf 'STG1 VALID\\nextra'", "invalid_helper_response"),
        (
            4,
            "printf 'STG1 VALID\\n'; printf PRIVATE_CANDIDATE_SENTINEL >&2",
            "invalid_helper_response",
        ),
        (5, "printf '\\377'", "invalid_helper_response"),
    ] {
        let project = t.path().join(format!("helper-{n}"));
        let p = helper_project(&project, script);
        let (r, capture) = gate::validate_with_capture(&p, &request(&store), None);
        refused(&r, "graph_policy", kind);
        assert!(
            capture.is_none(),
            "a helper refusal must not mint authority"
        );
        assert_eq!(r.rows.len(), 1);
        assert!(r.rows[0].id.is_none());
        assert_eq!(
            r.version,
            Some(gate::version(&crate::Store::read(&store).unwrap()))
        );
        assert_eq!(r.nodes, 1);
        assert_eq!(fs::read(file(&store)).unwrap(), original);
    }
    let project = t.path().join("missing-helper");
    let p = helper_project(&project, "exit 9");
    fs::remove_file(project.join("build/graph-policy")).unwrap();
    refused(
        &gate::validate(&p, &request(&store)),
        "graph_policy",
        "not_found",
    );
    assert_eq!(
        gate::validate(&policy(), &request(&store)).status,
        "current"
    );
}
#[test]
fn candidate_validation_defers_one_inventory_but_retains_final_asset_barrier() {
    let t = tempfile::tempdir().unwrap();
    fixture(t.path());
    crate::coherent_capture::take_work();
    let (report, capture) = gate::validate_with_capture(&policy(), &request(t.path()), None);
    assert_eq!(report.status, "current");
    assert!(capture.is_some());
    assert_eq!(crate::coherent_capture::take_work().inventories, 2);

    // The initial identity check does not cover unrelated assets. The mandatory
    // complete final inventory still must: mutate one while the policy runs.
    let asset = t.path().join("asset.txt");
    fs::write(&asset, "before").unwrap();
    let helper = helper_project(
        &t.path().join("helper"),
        &format!(
            "printf after > '{}'; printf 'STG1 VALID\\n'",
            asset.display()
        ),
    );
    let (report, capture) = gate::validate_with_capture(&helper, &request(t.path()), None);
    refused(&report, "source_recheck", "source_changed");
    assert_eq!(report.status, "stale");
    assert!(capture.is_none());
}

#[test]
fn candidate_refusals_capture_race_keeps_known_provenance() {
    let t = tempfile::tempdir().unwrap();
    fixture(t.path());
    let path = file(t.path());
    crate::coherent_capture::on_inventory(move || {
        fs::write(&path, fs::read(&path).unwrap()).unwrap();
    });
    refused(
        &gate::validate(&policy(), &request(t.path())),
        "candidate_capture",
        "source_changed",
    );
    assert_eq!(
        gate::validate(&policy(), &request(t.path())).status,
        "current"
    );
}
#[test]
fn candidate_refusals_source_and_helper_identity_races_remain_fail_closed() {
    for helper_race in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let store = t.path().join("private-path");
        fixture(&store);
        let project = t.path().join("helper");
        let p = if helper_race {
            helper_project(&project, "printf 'STG1 VALID\\n'")
        } else {
            policy()
        };
        let ready = t.path().join("ready");
        let release = t.path().join("release");
        let mut q = request(&store);
        q.pause_before_accept = Some(PauseBeforeAccept {
            ready: ready.clone(),
            release: release.clone(),
        });
        let h = thread::spawn(move || gate::validate(&p, &q));
        let start = Instant::now();
        while !ready.exists() {
            assert!(start.elapsed() < Duration::from_secs(20));
            thread::sleep(Duration::from_millis(5));
        }
        if helper_race {
            fs::write(
                project.join("build/graph-policy"),
                "changed PRIVATE_CANDIDATE_SENTINEL",
            )
            .unwrap();
        } else {
            node(&store, "in_progress", "Changed PRIVATE_CANDIDATE_SENTINEL");
        }
        fs::write(release, "go").unwrap();
        let r = h.join().unwrap();
        if helper_race {
            refused(&r, "policy_identity", "identity_changed");
            assert_eq!(r.status, "rejected");
        } else {
            refused(&r, "source_recheck", "source_changed");
            assert_eq!(r.status, "stale");
        }
        assert_eq!(
            gate::validate(&policy(), &request(&store)).status,
            "current"
        );
    }
}
