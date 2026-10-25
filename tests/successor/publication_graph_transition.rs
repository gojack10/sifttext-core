//! Exact native publication tests, not serialized proof/fixture-answer tests.
use crate::{
    Policy, candidate_validation as gate, checked_publication as publish,
    graph_transition::Transition,
};
use std::{collections::BTreeSet, fs, path::Path, process::Command};

fn id(n: usize) -> String {
    format!("72000000-0000-4000-8000-{n:012}")
}
fn file(p: &Path, n: usize) -> std::path::PathBuf {
    p.join(format!("nodes/{}/node.md", id(n)))
}
fn node(p: &Path, n: usize, status: &str, body: &str) {
    fs::write(file(p,n), format!("---\nid: \"{}\"\nname: \"Node {n}\"\nmicro_summary: \"Summary\"\nstatus: \"{status}\"\nsee_also: []\n---\n{body}",id(n))).unwrap();
}
fn fixture(p: &Path) {
    fs::create_dir_all(p.join("trees")).unwrap();
    for n in 1..=2 {
        let dir = p.join("nodes").join(id(n));
        fs::create_dir_all(dir.join("children")).unwrap();
        fs::write(dir.join("children/.keep"), "").unwrap();
        fs::write(dir.join("children.order"), "").unwrap();
        node(p, n, "in_progress", "Body");
        std::os::unix::fs::symlink(format!("../nodes/{}", id(n)), p.join("trees").join(id(n)))
            .unwrap();
    }
    fs::write(p.join(".gitignore"), ".sifttext/\n*.lean\n*.meta.json\n").unwrap();
}
fn git(p: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(p)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn gate_request(p: &Path, before: Option<&Path>) -> gate::Request {
    gate::Request {
        store: p.into(),
        before: before.map(Into::into),
        explicit: BTreeSet::new(),
        expected_version: None,
        pause_before_accept: None,
    }
}
fn request(repo: &Path, draft: Option<&Path>, base: Option<&str>) -> publish::Request {
    publish::Request {
        repo: repo.into(),
        accepted_ref: "refs/heads/main".into(),
        draft: draft.map(Into::into),
        commit: draft.map(|d| git(d, &["rev-parse", "HEAD"])),
        base: base.map(Into::into),
        explicit: BTreeSet::new(),
        pause_before_publish: None,
        pause_after_publish: None,
    }
}
fn commit(p: &Path) {
    git(p, &["add", "nodes", "trees"]);
    git(p, &["commit", "-qm", "authored edit"]);
}

#[test]
fn publication_graph_transition_runs_on_exact_candidates_without_accepting_bad_bodies() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    fixture(&repo);
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "baseline"]);
    let base = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["checkout", "--detach", "-q", &base]);
    let draft = temp.path().join("draft");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "draft",
            draft.to_str().unwrap(),
            &base,
        ],
    );
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let recovery = publish::submit(&policy, &request(&repo, None, None));
    assert_eq!(recovery.status, "current", "{recovery:?}");
    assert!(recovery.graph_checks.iter().all(|g| g.mode == "full_law"));
    node(&draft, 1, "in_progress", &" a".repeat(5000));
    commit(&draft);
    let q = request(&repo, Some(&draft), Some(&base));
    let r = publish::submit(&policy, &q);
    assert_eq!(r.status, "current", "{r:?}");
    assert!(r.published);
    assert_eq!(r.graph_checks.len(), 1); // Only the actual initial execution.
    for g in &r.graph_checks {
        assert_eq!(g.mode, "preserved");
        assert_eq!(g.candidate_version, r.version);
        assert_eq!(g.base_version, recovery.version);
    }
    assert_eq!(
        git(&repo, &["rev-parse", "main"]),
        *r.candidate.as_ref().unwrap()
    );
    assert_eq!(git(&draft, &["rev-parse", "HEAD"]), q.commit.unwrap());
    assert_eq!(
        crate::outlines::inspect(&policy, r.view.as_ref().unwrap(), None)
            .unwrap()
            .version,
        r.version
    );
    let full = gate::validate(&policy, &gate_request(r.view.as_ref().unwrap(), None));
    assert_eq!(full.status, "current");
    assert_eq!(full.version, r.version);
    node(&draft, 1, "in_progress", &" a".repeat(5001));
    commit(&draft);
    let rejected = publish::submit(&policy, &request(&repo, Some(&draft), Some(&base)));
    assert!(!rejected.published);
    assert_eq!(rejected.graph_checks[0].mode, "preserved");
    assert!(
        rejected
            .validation
            .unwrap()
            .rows
            .iter()
            .any(|r| r.check == "content_over_limit" && r.tokens == Some(5001))
    );
    assert_eq!(
        git(&repo, &["rev-parse", "main"]),
        *r.candidate.as_ref().unwrap()
    );
    assert_eq!(git(&draft, &["status", "--porcelain"]), "");
    // A frontmatter/status change falls back, and still rejects a blank resolved body.
    node(&draft, 1, "resolved", " \n\t");
    commit(&draft);
    let rejected = publish::submit(&policy, &request(&repo, Some(&draft), Some(&base)));
    assert_eq!(rejected.graph_checks[0].mode, "full_law");
    assert!(!rejected.published);
    assert!(
        rejected
            .validation
            .unwrap()
            .rows
            .iter()
            .any(|r| r.check == "resolved_body_blank")
    );
    node(&draft, 1, "resolved", "Answer");
    commit(&draft);
    let mut q = request(&repo, Some(&draft), Some(&base));
    q.explicit.insert(id(1));
    let r = publish::submit(&policy, &q);
    assert_eq!(r.status, "current", "{r:?}");
    assert!(r.graph_checks.iter().all(|g| g.mode == "full_law"));
    // Same frontmatter: graph preservation is insufficient for resolved body acceptance.
    node(&draft, 1, "resolved", " \t");
    commit(&draft);
    let r = publish::submit(&policy, &request(&repo, Some(&draft), Some(&base)));
    assert_eq!(r.graph_checks[0].mode, "preserved");
    assert!(!r.published);
    assert!(
        r.validation
            .unwrap()
            .rows
            .iter()
            .any(|r| r.check == "resolved_body_blank")
    );
}

#[test]
fn publication_recovery_without_transition_still_rejects_invalid_accepted_knowledge() {
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    for mutation in ["graph", "tokens", "status", "blank"] {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fixture(&repo);
        match mutation {
            "graph" => fs::remove_file(repo.join("trees").join(id(1))).unwrap(),
            "tokens" => node(&repo, 1, "in_progress", &" a".repeat(5001)),
            "status" => node(&repo, 1, "unknown", "Body"),
            "blank" => node(&repo, 1, "resolved", " \t"),
            _ => unreachable!(),
        }
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "invalid accepted fixture"]);
        let base = git(&repo, &["rev-parse", "HEAD"]);
        git(&repo, &["checkout", "--detach", "-q", &base]);
        let r = publish::submit(&policy, &request(&repo, None, None));
        assert_eq!(r.status, "rejected", "{mutation}: {r:?}");
        assert_eq!(r.check, "candidate_validation_rejected", "{r:?}");
        let expected = match mutation {
            "graph" => "placement_count",
            "tokens" => "content_over_limit",
            "status" => "status_or_reason_type",
            "blank" => "resolved_body_blank",
            _ => unreachable!(),
        };
        assert!(r.validation.as_ref().unwrap().rows.iter().any(|row| row.check == expected), "{r:?}");
        assert!(!r.published, "{mutation}: {r:?}");
        assert!(r.view.is_none());
        assert_eq!(r.graph_checks.len(), 1);
        assert_eq!(r.graph_checks[0].mode, "full_law");
        assert_eq!(git(&repo, &["rev-parse", "main"]), base);
        assert_eq!(git(&repo, &["status", "--porcelain"]), "");
    }
}

#[test]
fn publication_graph_transition_absent_mismatched_or_mutated_capabilities_never_skip_checks() {
    let temp = tempfile::tempdir().unwrap();
    let before = temp.path().join("before");
    let after = temp.path().join("after");
    fixture(&before);
    fixture(&after);
    let project = temp.path().join("policy");
    copy_policy(&project);
    let policy = Policy::checked(&project).unwrap();
    let transition = Transition::checked(&policy).unwrap();
    let base = transition.prove_base(&before).unwrap();
    node(&after, 1, "in_progress", "Edited");
    let q = gate_request(&after, Some(&before));
    assert_eq!(gate::validate(&policy, &q).graph.mode, "full_law");
    let proved = gate::validate_with_transition(&policy, &q, Some((&transition, &base)));
    assert_eq!(proved.status, "current");
    assert_eq!(proved.graph.mode, "preserved");
    let other = temp.path().join("other");
    fixture(&other);
    node(&other, 2, "in_progress", "Another exact base");
    let mismatched = transition.prove_base(&other).unwrap();
    let fallback = gate::validate_with_transition(&policy, &q, Some((&transition, &mismatched)));
    assert_eq!(fallback.status, "current");
    assert_eq!(fallback.graph.mode, "full_law");
    assert_eq!(fallback.graph.reason, "base version mismatch");
    let mut forged = gate_request(&after, Some(&before));
    forged.expected_version = Some("forged".into());
    assert_eq!(
        gate::validate_with_transition(&policy, &forged, Some((&transition, &base))).status,
        "rejected"
    );
    // Mutating the receipt's original source cannot be disguised by presenting
    // a different immutable before store with identical bytes/version.
    let clone = temp.path().join("clone");
    fixture(&clone);
    node(&before, 2, "in_progress", "Raced proof source");
    let stale = gate::validate_with_transition(
        &policy,
        &gate_request(&after, Some(&clone)),
        Some((&transition, &base)),
    );
    assert_eq!(stale.status, "rejected");
    assert_eq!(stale.graph.mode, "transition_error");
    // Artifact corruption is an error, not an unsupported-class fallback.
    let clean = transition.prove_base(&clone).unwrap();
    fs::write(project.join("build/graph-transition"), b"forged executable").unwrap();
    let error = gate::validate_with_transition(
        &policy,
        &gate_request(&after, Some(&clone)),
        Some((&transition, &clean)),
    );
    assert_eq!(error.status, "rejected");
    assert_eq!(error.graph.mode, "transition_error");
}

#[test]
fn publication_graph_transition_not_examined_activation_remains_enforced() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    fixture(&repo);
    node(&repo, 1, "not_examined", "Initial");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "baseline"]);
    let base = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["checkout", "--detach", "-q", &base]);
    let draft = temp.path().join("draft");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "draft",
            draft.to_str().unwrap(),
            &base,
        ],
    );
    node(&draft, 1, "not_examined", "Edited");
    commit(&draft);
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let r = publish::submit(&policy, &request(&repo, Some(&draft), Some(&base)));
    assert_eq!(r.graph_checks[0].mode, "preserved");
    assert!(!r.published);
    assert!(
        r.validation
            .unwrap()
            .rows
            .iter()
            .any(|r| r.check == "status_transition_or_reason")
    );
    assert_eq!(git(&repo, &["rev-parse", "main"]), base);
}

#[test]
fn publication_graph_transition_policy_race_rejects_exact_submission_and_preserves_draft() {
    publication_asset_race(false);
}

#[test]
fn publication_graph_transition_bundled_helper_race_preserves_draft() {
    publication_asset_race(true);
}

fn copy_policy(project: &Path) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = crate::candidate_files(source).unwrap();
    files.extend([
        "build/candidate.sha256".into(),
        "build/graph-policy.sha256".into(),
        "build/graph-policy".into(),
        "build/graph-transition.sha256".into(),
        "build/graph-transition".into(),
    ]);
    for file in files {
        let target = project.join(&file);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source.join(file), target).unwrap();
    }
}

#[test]
fn publication_graph_transition_rejects_unsafe_bundled_stamps() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("policy");
    copy_policy(&project);
    let policy = Policy::checked(&project).unwrap();
    let stamp = project.join("build/graph-transition.sha256");
    let original = fs::read(&stamp).unwrap();
    let hash = crate::sha256(&fs::read(project.join("build/graph-transition")).unwrap());
    assert!(Transition::checked(&policy).is_ok());
    for bad in [
        String::new(),
        format!("{hash} build/graph-transition\n"),
        format!("{hash}  ../build/graph-transition\n"),
        format!("{hash}  /build/graph-transition\n"),
        format!("{hash}  build/graph-policy\n"),
        format!("{hash}  build/graph-transition\n{hash}  build/graph-transition\n"),
        format!("{}  build/graph-transition\n", "x".repeat(64)),
        "0  build/graph-transition\n".into(),
        format!("{}  build/graph-transition\n", "0".repeat(64)),
    ] {
        fs::write(&stamp, &bad).unwrap();
        assert!(
            Transition::checked(&policy).is_err(),
            "accepted stamp {bad:?}"
        );
    }
    fs::write(&stamp, &original).unwrap();
    assert!(Transition::checked(&policy).is_ok());
    for asset in [stamp, project.join("build/graph-transition")] {
        let backup = temp.path().join("backup");
        fs::rename(&asset, &backup).unwrap();
        assert!(
            Transition::checked(&policy).is_err(),
            "accepted missing asset"
        );
        std::os::unix::fs::symlink(&backup, &asset).unwrap();
        assert!(
            Transition::checked(&policy).is_err(),
            "accepted asset symlink"
        );
        fs::remove_file(&asset).unwrap();
        fs::rename(&backup, &asset).unwrap();
        assert!(Transition::checked(&policy).is_ok());
    }
}

#[test]
fn transition_policy_binding_rejects_foreign_and_restamped_policies_without_execution() {
    fn policy_at(project: &Path) -> Policy {
        fs::create_dir_all(project.join("build")).unwrap();
        for file in ["Cargo.toml", "Cargo.lock", "README.md", ".gitignore"] {
            fs::write(project.join(file), "fixture").unwrap();
        }
        let manifest = crate::candidate_files(project)
            .unwrap()
            .into_iter()
            .map(|file| {
                format!("{}  {file}\n", crate::sha256(&fs::read(project.join(&file)).unwrap()))
            })
            .collect::<String>();
        fs::write(project.join("build/candidate.sha256"), manifest).unwrap();
        for helper in ["graph-policy", "graph-transition"] {
            let bytes = b"not executable; identity check must not launch this";
            fs::write(project.join("build").join(helper), bytes).unwrap();
            fs::write(
                project.join("build").join(format!("{helper}.sha256")),
                format!("{}  build/{helper}\n", crate::sha256(bytes)),
            ).unwrap();
        }
        Policy::checked(project).unwrap()
    }
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("policy");
    let policy = policy_at(&project);
    let transition = Transition::checked(&policy).unwrap();
    assert!(transition.check_policy(&policy).is_ok());
    assert!(transition.check_policy(&Policy::checked(&project).unwrap()).is_ok());
    let foreign = policy_at(&temp.path().join("foreign"));
    assert!(transition.check_policy(&foreign).is_err());
    set_graph_helper(&project, "#!/bin/sh\nexit 7\n");
    assert!(Policy::checked(&project).is_ok());
    assert!(transition.check_policy(&policy).is_err());
}

#[test]
fn retained_validation_rejects_a_foreign_preservation_policy_without_minting() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("policy");
    let foreign = temp.path().join("foreign");
    copy_policy(&project);
    copy_policy(&foreign);
    set_graph_helper(&foreign, "#!/bin/sh\nexit 7\n");
    let policy = Policy::checked(&project).unwrap();
    let rejecting_policy = Policy::checked(&foreign).unwrap();
    let before = temp.path().join("before");
    let after = temp.path().join("after");
    fixture(&before);
    fixture(&after);
    node(&after, 1, "in_progress", "Edited body");
    let transition = Transition::checked(&policy).unwrap();
    let base = transition.prove_base(&before).unwrap();
    let request = gate_request(&after, Some(&before));
    assert_eq!(gate::validate(&rejecting_policy, &request).status, "rejected");
    let (report, capture) = gate::validate_with_capture(
        &rejecting_policy, &request, Some((&transition, &base)),
    );
    assert_eq!(report.status, "rejected", "{report:?}");
    assert_eq!(report.graph.mode, "transition_error");
    assert_eq!(report.failure.unwrap().stage, "graph_transition");
    assert!(capture.is_none());
}

#[test]
fn retained_validation_reuses_one_real_graph_call_and_preserves_transition() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("policy");
    copy_policy(&project);
    // Count invocations while delegating every verdict to the real compiled
    // Bend binary. Never substitute a fixture VALID answer.
    fs::rename(
        project.join("build/graph-policy"),
        project.join("build/graph-policy-real"),
    )
    .unwrap();
    set_graph_helper(
        &project,
        "#!/bin/sh\nd=$(dirname \"$0\")\nprintf 'call\\n' >> \"$d/calls\"\nexec \"$d/graph-policy-real\" \"$@\"\n",
    );
    let before = temp.path().join("before");
    let after = temp.path().join("after");
    fixture(&before);
    fixture(&after);
    node(&after, 1, "in_progress", "Edited body");
    let policy = Policy::checked(&project).unwrap();
    let transition = Transition::checked(&policy).unwrap();
    let old = gate::validate(&policy, &gate_request(&before, None));
    assert_eq!(old.status, "current");
    let old_base = transition.prove_base(&before).unwrap();
    assert_eq!(
        fs::read_to_string(project.join("build/calls"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    fs::write(project.join("build/calls"), "").unwrap();
    let (report, capture) =
        gate::validate_with_capture(&policy, &gate_request(&before, None), None);
    assert_eq!(report.status, "current");
    assert_eq!(
        serde_json::to_value(&report).unwrap(),
        serde_json::to_value(&old).unwrap()
    );
    let base = transition.prove_validated(capture.unwrap()).unwrap();
    assert_eq!(base.version(), old_base.version());
    assert_eq!(
        fs::read_to_string(project.join("build/calls"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert_eq!(
        transition
            .check(Some(&base), base.version(), &after)
            .unwrap(),
        transition
            .check(Some(&old_base), old_base.version(), &after)
            .unwrap()
    );
}

fn set_graph_helper(project: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    let helper = project.join("build/graph-policy");
    fs::write(&helper, body).unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        project.join("build/graph-policy.sha256"),
        format!("{}  build/graph-policy\n", crate::sha256(body.as_bytes())),
    )
    .unwrap();
}

#[test]
fn retained_validation_never_mints_rejected_or_failed_candidates() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("policy");
    copy_policy(&project);
    let policy = Policy::checked(&project).unwrap();
    for (i, status, body) in [
        (1, "resolved", " "),
        (2, "unknown", "Body"),
        (3, "in_progress", &" a".repeat(5001)),
    ] {
        let source = temp.path().join(format!("invalid-{i}"));
        fixture(&source);
        node(&source, 1, status, body);
        let (report, capture) =
            gate::validate_with_capture(&policy, &gate_request(&source, None), None);
        assert_ne!(report.status, "current", "{report:?}");
        assert!(capture.is_none());
    }
    let graph = temp.path().join("invalid-graph");
    fixture(&graph);
    fs::remove_file(graph.join("trees").join(id(1))).unwrap();
    let (report, capture) = gate::validate_with_capture(&policy, &gate_request(&graph, None), None);
    assert_eq!(report.status, "rejected");
    assert!(capture.is_none());
    let source = temp.path().join("source");
    fixture(&source);
    for helper in ["#!/bin/sh\nexit 7\n", "#!/bin/sh\nprintf 'malformed\\n'\n"] {
        set_graph_helper(&project, helper);
        let policy = Policy::checked(&project).unwrap();
        let (report, capture) =
            gate::validate_with_capture(&policy, &gate_request(&source, None), None);
        assert_eq!(report.status, "rejected");
        assert!(capture.is_none());
    }
}

#[test]
fn retained_validation_refuses_foreign_policy_and_same_byte_source_replacement() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("policy");
    let foreign = temp.path().join("foreign");
    copy_policy(&project);
    copy_policy(&foreign);
    let policy = Policy::checked(&project).unwrap();
    let other = Policy::checked(&foreign).unwrap();
    let source = temp.path().join("source");
    fixture(&source);
    let request = gate_request(&source, None);
    let (_, capture) = gate::validate_with_capture(&policy, &request, None);
    let capture = capture.unwrap();
    assert!(capture.checked(&policy).is_ok());
    assert!(capture.checked(&other).is_err());
    let file = file(&source, 1);
    let saved = temp.path().join("original-node");
    fs::rename(&file, &saved).unwrap();
    fs::copy(&saved, &file).unwrap();
    assert_eq!(fs::read(&file).unwrap(), fs::read(&saved).unwrap());
    assert!(capture.checked(&policy).is_err());
    let transition = Transition::checked(&policy).unwrap();
    assert!(transition.prove_validated(capture).is_err());
    let (_, capture) = gate::validate_with_capture(&policy, &request, None);
    let capture = capture.unwrap();
    // Even a newly valid, re-stamped policy cannot inherit the old capability.
    set_graph_helper(&project, "#!/bin/sh\nexit 7\n");
    assert!(Policy::checked(&project).is_ok());
    assert!(capture.checked(&policy).is_err());
    assert!(transition.prove_validated(capture).is_err());
}

#[test]
fn retained_validation_rechecks_both_complete_inventories_without_reparsing() {
    for side in ["candidate", "before"] {
        for action in ["body", "asset", "extra", "replacement"] {
            let temp = tempfile::tempdir().unwrap();
            let before = temp.path().join("before");
            let after = temp.path().join("after");
            fixture(&before);
            fixture(&after);
            for path in [&before, &after] {
                fs::write(path.join("asset.bin"), b"original asset").unwrap();
            }
            let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
            let (report, capture) =
                gate::validate_with_capture(&policy, &gate_request(&after, Some(&before)), None);
            assert_eq!(report.status, "current", "{report:?}");
            let capture = capture.unwrap();
            assert!(capture.checked(&policy).is_ok());
            let path = if side == "before" { &before } else { &after };
            match action {
                "body" => node(path, 1, "in_progress", "Changed body"),
                "asset" => fs::write(path.join("asset.bin"), b"changed asset").unwrap(),
                "extra" => fs::write(path.join("extra.bin"), b"extra").unwrap(),
                "replacement" => {
                    let file = file(path, 1);
                    let saved = temp.path().join("saved");
                    fs::rename(&file, &saved).unwrap();
                    fs::copy(&saved, &file).unwrap();
                }
                _ => unreachable!(),
            }
            assert!(capture.checked(&policy).is_err(), "{side}/{action}");
        }
    }
}

#[test]
fn retained_validation_never_mints_a_stale_accept_barrier_capture() {
    use std::{
        thread,
        time::{Duration, Instant},
    };
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fixture(&source);
    let ready = temp.path().join("ready");
    let release = temp.path().join("release");
    let mut request = gate_request(&source, None);
    request.pause_before_accept = Some(gate::PauseBeforeAccept {
        ready: ready.clone(),
        release: release.clone(),
    });
    let worker = thread::spawn(move || {
        let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        gate::validate_with_capture(&policy, &request, None)
    });
    let start = Instant::now();
    while !ready.exists() {
        assert!(!worker.is_finished(), "validator failed before the barrier");
        assert!(start.elapsed() < Duration::from_secs(30));
        thread::sleep(Duration::from_millis(5));
    }
    let file = file(&source, 1);
    let bytes = fs::read(&file).unwrap();
    fs::write(&file, b"changed").unwrap();
    fs::write(&file, bytes).unwrap();
    fs::write(release, b"go").unwrap();
    let (report, capture) = worker.join().unwrap();
    assert_eq!(report.status, "stale", "{report:?}");
    assert!(capture.is_none());
}

fn publication_asset_race(helper: bool) {
    use std::{
        thread,
        time::{Duration, Instant},
    };
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    fixture(&repo);
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "baseline"]);
    let base = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["checkout", "--detach", "-q", &base]);
    let draft = temp.path().join("draft");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "draft",
            draft.to_str().unwrap(),
            &base,
        ],
    );
    node(&draft, 1, "in_progress", "Edited");
    commit(&draft);
    let sha = git(&draft, &["rev-parse", "HEAD"]);
    // Copy only stamped native policy inputs into an owned disposable synthetic
    // policy. Never mutate the actual checkout while its tests are running.
    let project = temp.path().join("policy");
    copy_policy(&project);
    let ready = temp.path().join("ready");
    let release = temp.path().join("release");
    let mut q = request(&repo, Some(&draft), Some(&base));
    q.pause_before_publish = Some(publish::Barrier {
        ready: ready.clone(),
        release: release.clone(),
    });
    let own_project = project.clone();
    let worker = thread::spawn(move || {
        let policy = Policy::checked(&own_project).unwrap();
        publish::submit(&policy, &q)
    });
    let start = Instant::now();
    while !ready.exists() {
        assert!(start.elapsed() < Duration::from_secs(30));
        assert!(!worker.is_finished(), "submission failed before barrier");
        thread::sleep(Duration::from_millis(5));
    }
    if helper {
        fs::write(project.join("build/graph-transition"), "forged helper").unwrap();
    } else {
        fs::write(project.join("README.md"), "raced private policy input").unwrap();
    }
    fs::write(release, "go").unwrap();
    let r = worker.join().unwrap();
    assert_eq!(r.status, "rejected", "{r:?}");
    assert!(!r.published);
    assert_eq!(r.check, "candidate_recheck_rejected");
    assert_eq!(r.graph_checks.len(), 1);
    assert_eq!(r.graph_checks[0].mode, "preserved");
    assert_eq!(git(&repo, &["rev-parse", "main"]), base);
    assert_eq!(git(&draft, &["rev-parse", "HEAD"]), sha);
    assert_eq!(git(&draft, &["status", "--porcelain"]), "");
}
