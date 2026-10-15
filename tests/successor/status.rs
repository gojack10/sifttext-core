//! Independent finite expectations, pure-model/native agreement, and byte/race controls.
//! These are not kernel proofs or an unmanaged multi-writer transaction claim.
use crate::{
    Entry, Policy, Store,
    status::{self, PauseBeforeAccept, State, Status},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

const STATUSES: [Status; 6] = [
    Status::NotExamined,
    Status::InProgress,
    Status::Stuck,
    Status::Deferred,
    Status::Discarded,
    Status::Resolved,
];
fn id(n: usize) -> String {
    format!("30000000-0000-4000-8000-{n:012}")
}
fn reason(s: Status) -> Option<&'static str> {
    match s {
        Status::Stuck => Some("blocker"),
        Status::Deferred => Some("deferred_reason"),
        Status::Discarded => Some("discard_reason"),
        _ => None,
    }
}
fn state(s: Status) -> State {
    State {
        status: s,
        reasons: reason(s)
            .map(|k| (k.to_owned(), "Reason; resume after check. Ω".to_owned()))
            .into_iter()
            .collect(),
    }
}
fn file(root: &Path, n: usize) -> PathBuf {
    root.join(format!("nodes/{}/node.md", id(n)))
}
fn text(n: usize, state: &State, edit: bool) -> String {
    let mut text = format!(
        "---\r\nid: \"{}\"\r\nname: \"Node {n}\"\r\nmicro_summary: \"Stable summary Ω\"\r\nsee_also: []\r\ncustom: {{ \"nested\" : [1, true] }}\r\nstatus:   {}  \r\n",
        id(n),
        serde_json::to_string(&state.status).unwrap()
    );
    for (key, value) in &state.reasons {
        text.push_str(&format!(
            "{key}: {}\r\n",
            serde_json::to_string(value).unwrap()
        ));
    }
    text.push_str("---\r\nBody stays exact: \"quotes\" Ω\r\n---\r\nstatus: body, not metadata");
    if edit {
        text.push(' ');
    }
    text
}
fn fixture(root: &Path, states: &[State]) {
    fs::create_dir_all(root.join("trees")).unwrap();
    for (i, state) in states.iter().enumerate() {
        let n = i + 1;
        let dir = root.join("nodes").join(id(n));
        fs::create_dir_all(dir.join("children")).unwrap();
        fs::write(file(root, n), text(n, state, false)).unwrap();
        let child = if n < states.len() {
            id(n + 1) + "\n"
        } else {
            String::new()
        };
        fs::write(dir.join("children.order"), child).unwrap();
        if n < states.len() {
            std::os::unix::fs::symlink(
                format!("../../{}", id(n + 1)),
                dir.join("children").join(id(n + 1)),
            )
            .unwrap();
        }
    }
    std::os::unix::fs::symlink(
        format!("../nodes/{}", id(1)),
        root.join("trees").join(id(1)),
    )
    .unwrap();
    fs::write(root.join("sentinel.bin"), [0, 255, 128, 10]).unwrap();
}
fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}
fn inventory(root: &Path) -> BTreeMap<PathBuf, Entry> {
    Store::read(root).unwrap().inventory
}
fn native_state(root: &Path, n: usize) -> State {
    let text = fs::read_to_string(file(root, n)).unwrap();
    let parsed = crate::markdown(&text).unwrap();
    let status = serde_json::from_str(parsed.fields["status"]).unwrap();
    let reasons = ["blocker", "deferred_reason", "discard_reason"]
        .into_iter()
        .filter_map(|k| {
            parsed
                .fields
                .get(k)
                .map(|v| (k.into(), serde_json::from_str(v).unwrap()))
        })
        .collect();
    State { status, reasons }
}

// Independent expected state table: non-explicit edits preserve each of the five
// already-examined states; only first examination activates. No heuristic on edits.
fn expected(old: &State, requested: &State, changed: bool, explicit: bool) -> Option<State> {
    let selected = if explicit {
        requested.status
    } else {
        match old.status {
            Status::NotExamined if changed => Status::InProgress,
            s => s,
        }
    };
    if !explicit && requested.status != old.status && requested.status != selected {
        return None;
    }
    let mut reasons = requested.reasons.clone();
    if selected != old.status
        && let Some(key) = reason(old.status)
    {
        reasons.remove(key);
    }
    if let Some(key) = reason(selected)
        && reasons.get(key).is_none_or(|r| r.trim().is_empty())
    {
        return None;
    }
    Some(State {
        status: selected,
        reasons,
    })
}

fn check_transitions(policy: &Policy, worker: usize, workers: usize) -> (usize, usize) {
    let mut count = 0;
    let mut checked = 0;
    let mut accepted = 0;
    for old_status in STATUSES {
        for requested_status in STATUSES {
            for explicit in [false, true] {
                for minor in [false, true] {
                    for reason_case in ["missing", "blank", "valid"] {
                        if count % workers != worker {
                            count += 1;
                            continue;
                        }
                        let before = tempfile::tempdir().unwrap();
                        let after = tempfile::tempdir().unwrap();
                        let old = state(old_status);
                        let mut requested = old.clone();
                        requested.status = requested_status;
                        if let Some(key) = reason(requested_status) {
                            match reason_case {
                                "missing" => {
                                    requested.reasons.remove(key);
                                }
                                "blank" => {
                                    requested.reasons.insert(key.into(), " \t\n".into());
                                }
                                _ => {
                                    requested
                                        .reasons
                                        .insert(key.into(), "Reason; resume after check. Ω".into());
                                }
                            }
                        }
                        fixture(before.path(), std::slice::from_ref(&old));
                        fixture(after.path(), std::slice::from_ref(&requested));
                        fs::write(file(after.path(), 1), text(1, &requested, minor)).unwrap();
                        let original = inventory(after.path());
                        let base = inventory(before.path());
                        let changed = fs::read(file(before.path(), 1)).unwrap()
                            != fs::read(file(after.path(), 1)).unwrap();
                        let want = expected(&old, &requested, changed, explicit);
                        let expected_accept = want.is_some();
                        let pure = status::transition(Some(&old), &requested, changed, explicit);
                        assert_eq!(
                            pure.ok(),
                            want,
                            "pure {old_status:?}/{requested_status:?}/{explicit}/{minor}/{reason_case}"
                        );
                        let ids = if explicit {
                            BTreeSet::from([id(1)])
                        } else {
                            BTreeSet::new()
                        };
                        let native =
                            status::normalize(policy, before.path(), after.path(), &ids, None);
                        assert_eq!(
                            native.is_ok(),
                            want.is_some(),
                            "native {old_status:?}/{requested_status:?}/{explicit}/{minor}/{reason_case}: {native:?}"
                        );
                        if let Some(want) = want {
                            accepted += 1;
                            assert_eq!(native_state(after.path(), 1), want);
                            let got = fs::read_to_string(file(after.path(), 1)).unwrap();
                            // Independent byte expectation: only the exact status value and
                            // the departed reason line can change. Unknown whitespace is frozen.
                            let mut want_bytes = text(1, &requested, minor).replace(
                                &format!(
                                    "status:   {}  \r\n",
                                    serde_json::to_string(&requested.status).unwrap()
                                ),
                                &format!(
                                    "status:   {}  \r\n",
                                    serde_json::to_string(&want.status).unwrap()
                                ),
                            );
                            if old_status != want.status
                                && let Some(key) = reason(old_status)
                                && let Some(value) = requested.reasons.get(key)
                            {
                                want_bytes = want_bytes.replace(
                                    &format!(
                                        "{key}: {}\r\n",
                                        serde_json::to_string(value).unwrap()
                                    ),
                                    "",
                                );
                            }
                            assert_eq!(got, want_bytes, "normalization changed unrelated bytes");
                            let mut expected_inventory = original.clone();
                            expected_inventory.insert(
                                PathBuf::from(format!("nodes/{}/node.md", id(1))),
                                Entry::File(want_bytes.into_bytes()),
                            );
                            assert_eq!(inventory(after.path()), expected_inventory);
                        } else {
                            assert_eq!(
                                inventory(after.path()),
                                original,
                                "rejected candidate mutated"
                            );
                        }
                        assert_eq!(inventory(before.path()), base, "before store mutated");
                        println!(
                            "STATUS CASE {count}: old={old_status:?} requested={requested_status:?} explicit={explicit} minor={minor} reason={reason_case} accepted={} before_sha={} candidate_sha={} result_sha={}",
                            expected_accept,
                            crate::sha256(text(1, &old, false).as_bytes()),
                            crate::sha256(text(1, &requested, minor).as_bytes()),
                            crate::sha256(&fs::read(file(after.path(), 1)).unwrap())
                        );
                        checked += 1;
                        count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(count, 432);
    (checked, accepted)
}

#[test]
fn finite_independent_transition_corpus_matches_pure_model_and_native_bytes() {
    let policy = policy();
    let started = Instant::now();
    // ponytail: at most four isolated native calls; tune only with measured contention.
    let workers = thread::available_parallelism().map_or(1, |n| n.get().min(4));
    let (count, accepted) = thread::scope(|scope| {
        let handles = (0..workers)
            .map(|worker| {
                let policy = &policy;
                scope.spawn(move || check_transitions(policy, worker, workers))
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .fold((0, 0), |(count, accepted), handle| {
                let (checked, passed) = handle.join().unwrap();
                (count + checked, accepted + passed)
            })
    });
    assert_eq!(count, 432);
    assert!(accepted > 0 && accepted < count);
    println!(
        "STATUS FINITE MODEL/NATIVE: {count} cases, {accepted} accepted, {} rejected, {:?}; no universal/kernel claim",
        count - accepted,
        started.elapsed()
    );
}

#[test]
fn direct_child_table_reopening_and_no_auto_resolution() {
    let policy = policy();
    for child in STATUSES {
        let before = tempfile::tempdir().unwrap();
        let after = tempfile::tempdir().unwrap();
        let old = [state(Status::InProgress), state(child)];
        fixture(before.path(), &old);
        fixture(after.path(), &[state(Status::Resolved), state(child)]);
        let original = inventory(after.path());
        let mut pure = BTreeMap::from([(id(1), state(Status::Resolved)), (id(2), state(child))]);
        let children = BTreeMap::from([(id(1), vec![id(2)]), (id(2), vec![])]);
        let deliberate = BTreeSet::from([id(1)]);
        let blocked = matches!(
            child,
            Status::NotExamined | Status::InProgress | Status::Stuck
        );
        assert_eq!(
            status::child_gate(&mut pure, &children, &deliberate).is_err(),
            blocked
        );
        assert_eq!(
            status::normalize(&policy, before.path(), after.path(), &deliberate, None).is_err(),
            blocked
        );
        assert_eq!(inventory(after.path()), original);
        fs::write(
            file(after.path(), 1),
            text(1, &state(Status::InProgress), false),
        )
        .unwrap();
        status::normalize(&policy, before.path(), after.path(), &BTreeSet::new(), None).unwrap();
        assert_eq!(
            native_state(after.path(), 1).status,
            Status::InProgress,
            "children must never auto-resolve"
        );
    }
    let before = tempfile::tempdir().unwrap();
    let after = tempfile::tempdir().unwrap();
    fixture(
        before.path(),
        &[
            state(Status::Resolved),
            state(Status::Resolved),
            state(Status::Resolved),
        ],
    );
    fixture(
        after.path(),
        &[
            state(Status::Resolved),
            state(Status::Resolved),
            state(Status::InProgress),
        ],
    );
    let result = status::normalize(
        &policy,
        before.path(),
        after.path(),
        &BTreeSet::from([id(3)]),
        None,
    )
    .unwrap();
    assert_eq!(result.normalized_ids, vec![id(1), id(2)]);
    for n in 1..=3 {
        assert_eq!(native_state(after.path(), n).status, Status::InProgress);
    }
}

#[test]
fn creation_derived_only_and_actual_helper_reference_cleanup() {
    let policy = policy();
    let before = tempfile::tempdir().unwrap();
    let after = tempfile::tempdir().unwrap();
    fixture(before.path(), &[state(Status::NotExamined)]);
    fixture(after.path(), &[state(Status::NotExamined)]);
    let original = inventory(after.path());
    status::normalize(&policy, before.path(), after.path(), &BTreeSet::new(), None).unwrap();
    assert_eq!(inventory(after.path()), original);
    fs::write(
        after
            .path()
            .join("nodes")
            .join(id(1))
            .join("node.meta.json"),
        b"{\"derived\":true}",
    )
    .unwrap();
    fs::create_dir(after.path().join(".sifttext")).unwrap();
    fs::write(after.path().join(".sifttext/index.sqlite"), b"derived-only").unwrap();
    fs::write(
        after.path().join("nodes").join(id(1)).join("outline.lean"),
        b"derived-only",
    )
    .unwrap();
    let derived = inventory(after.path());
    status::normalize(&policy, before.path(), after.path(), &BTreeSet::new(), None).unwrap();
    assert_eq!(inventory(after.path()), derived);
    assert_eq!(native_state(after.path(), 1).status, Status::NotExamined);
    // A real existing native helper edits authored metadata; status uses its result,
    // not an invented second rename/reference graph implementation.
    let captured = Store::read(after.path()).unwrap();
    crate::apply(
        &policy,
        &captured,
        &crate::Operation::Rename {
            node: id(1),
            name: "Minor helper rename".into(),
        },
    )
    .unwrap();
    status::normalize(&policy, before.path(), after.path(), &BTreeSet::new(), None).unwrap();
    assert_eq!(native_state(after.path(), 1).status, Status::InProgress);
    // New authored roots may remain not_examined.
    let empty = tempfile::tempdir().unwrap();
    fs::create_dir(empty.path().join("nodes")).unwrap();
    fs::create_dir(empty.path().join("trees")).unwrap();
    status::normalize(&policy, empty.path(), before.path(), &BTreeSet::new(), None).unwrap();
    assert_eq!(native_state(before.path(), 1).status, Status::NotExamined);
    // Cleanup surviving authored references must activate an unexamined source.
    let b = tempfile::tempdir().unwrap();
    let a = tempfile::tempdir().unwrap();
    fixture(
        b.path(),
        &[state(Status::InProgress), state(Status::NotExamined)],
    );
    fixture(
        a.path(),
        &[state(Status::InProgress), state(Status::NotExamined)],
    );
    let source = fs::read_to_string(file(b.path(), 2)).unwrap().replace(
        "see_also: []",
        &format!(
            "see_also: [{{\"target\":\"{}\",\"reason\":\"cleanup specimen\"}}]",
            id(1)
        ),
    );
    fs::write(file(b.path(), 2), source).unwrap();
    status::normalize(&policy, b.path(), a.path(), &BTreeSet::new(), None).unwrap();
    assert_eq!(native_state(a.path(), 2).status, Status::InProgress);
}

#[test]
fn unchanged_normalization_reuses_only_the_exact_validated_inventory() {
    let project = tempfile::tempdir().unwrap();
    let policy = crate::outlines_tests::counted_policy(project.path());
    let calls = project.path().join("build/calls");
    let before = tempfile::tempdir().unwrap();
    let after = tempfile::tempdir().unwrap();
    fixture(before.path(), &[state(Status::NotExamined)]);
    fixture(after.path(), &[state(Status::NotExamined)]);
    let original = inventory(after.path());
    let no_op =
        status::normalize(&policy, before.path(), after.path(), &BTreeSet::new(), None).unwrap();
    assert!(no_op.normalized_ids.is_empty());
    assert_eq!(inventory(after.path()), original);
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 2);

    fs::write(&calls, "").unwrap();
    fs::write(
        file(after.path(), 1),
        text(1, &state(Status::NotExamined), true),
    )
    .unwrap();
    let changed =
        status::normalize(&policy, before.path(), after.path(), &BTreeSet::new(), None).unwrap();
    assert_eq!(changed.normalized_ids, [id(1)]);
    assert_eq!(native_state(after.path(), 1).status, Status::InProgress);
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 3);

    // A no-op plan must not bypass the candidate's initial native rejection.
    let bad = text(1, &state(Status::InProgress), false).replace(
        "see_also: []",
        &format!(
            "see_also: [{{\"target\":\"{}\",\"reason\":\"missing\"}}]",
            id(99)
        ),
    );
    fs::write(file(after.path(), 1), &bad).unwrap();
    assert!(
        status::normalize(&policy, before.path(), after.path(), &BTreeSet::new(), None).is_err()
    );
    assert_eq!(fs::read_to_string(file(after.path(), 1)).unwrap(), bad);
}

#[test]
fn candidate_child_before_and_derived_races_reject_without_any_patch() {
    for change in [
        "child",
        "before",
        "derived",
        "none",
        "noop-child",
        "noop-none",
        "noop-policy",
    ] {
        let before = tempfile::tempdir().unwrap();
        let after = tempfile::tempdir().unwrap();
        let barrier = tempfile::tempdir().unwrap();
        fixture(
            before.path(),
            &[state(Status::NotExamined), state(Status::Resolved)],
        );
        fixture(
            after.path(),
            &[state(Status::NotExamined), state(Status::Resolved)],
        );
        if !change.starts_with("noop-") {
            fs::write(
                file(after.path(), 1),
                text(1, &state(Status::NotExamined), true),
            )
            .unwrap();
        }
        let original = inventory(after.path());
        let temporary_policy = tempfile::tempdir().unwrap();
        let policy = if change == "noop-policy" {
            crate::outlines_tests::counted_policy(temporary_policy.path())
        } else {
            Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
        };
        let ready = barrier.path().join("ready");
        let release = barrier.path().join("release");
        let ready_worker = ready.clone();
        let release_worker = release.clone();
        let target = if change == "noop-policy" {
            temporary_policy.path().join("build/graph-policy.sha256")
        } else if change == "before" {
            file(before.path(), 2)
        } else if change == "derived" {
            after
                .path()
                .join("nodes")
                .join(id(2))
                .join("node.meta.json")
        } else {
            file(after.path(), 2)
        };
        let new_bytes = if change == "derived" {
            "{\"newer\":true}".to_owned()
        } else {
            text(2, &state(Status::InProgress), false)
        };
        let target_worker = target.clone();
        let new_worker = new_bytes.clone();
        let handle = thread::spawn(move || {
            let started = Instant::now();
            while !ready_worker.exists() {
                assert!(started.elapsed() < Duration::from_secs(10));
                thread::sleep(Duration::from_millis(5));
            }
            if change != "none" && change != "noop-none" {
                fs::write(target_worker, new_worker).unwrap();
            }
            fs::write(release_worker, b"release").unwrap();
        });
        let result = status::normalize(
            &policy,
            before.path(),
            after.path(),
            &BTreeSet::new(),
            Some(&PauseBeforeAccept { ready, release }),
        );
        handle.join().unwrap();
        if change == "none" || change == "noop-none" {
            assert!(result.is_ok());
            assert_eq!(
                native_state(after.path(), 1).status,
                if change == "noop-none" {
                    Status::NotExamined
                } else {
                    Status::InProgress
                }
            );
        } else {
            assert!(result.is_err());
            assert_eq!(fs::read_to_string(target).unwrap(), new_bytes);
            assert_eq!(
                fs::read(file(after.path(), 1)).unwrap(),
                match &original[&PathBuf::from(format!("nodes/{}/node.md", id(1)))] {
                    Entry::File(b) => b.clone(),
                    _ => unreachable!(),
                },
                "rejection applied partial normalization"
            );
        }
    }
}

#[test]
fn unreleased_barrier_expires_without_applying_status_patches() {
    let before = tempfile::tempdir().unwrap();
    let after = tempfile::tempdir().unwrap();
    fixture(
        before.path(),
        &[state(Status::NotExamined), state(Status::Resolved)],
    );
    fixture(
        after.path(),
        &[state(Status::NotExamined), state(Status::Resolved)],
    );
    fs::write(
        file(after.path(), 1),
        text(1, &state(Status::NotExamined), true),
    )
    .unwrap();
    let original = inventory(after.path());
    let barrier = tempfile::tempdir().unwrap();
    let pause = PauseBeforeAccept {
        ready: barrier.path().join("ready"),
        release: barrier.path().join("release"),
    };
    let (done, waiting) = std::sync::mpsc::channel();
    let release = pause.release.clone();
    thread::scope(|scope| {
        scope.spawn(move || {
            if waiting.recv_timeout(Duration::from_secs(20)).is_err() {
                fs::write(release, "rescue").unwrap();
            }
        });
        let result = status::normalize(
            &policy(),
            before.path(),
            after.path(),
            &BTreeSet::new(),
            Some(&pause),
        );
        let _ = done.send(());
        assert_eq!(
            result.unwrap_err().to_string(),
            "inspection barrier expired"
        );
    });
    assert!(pause.ready.exists());
    assert!(!pause.release.exists());
    assert_eq!(inventory(after.path()), original);
    status::normalize(
        &policy(),
        before.path(),
        after.path(),
        &BTreeSet::new(),
        None,
    )
    .unwrap();
    assert_eq!(native_state(after.path(), 1).status, Status::InProgress);
}

#[test]
fn unknown_state_reason_types_and_explicit_identity_reject_nonmutating() {
    let policy = policy();
    let before = tempfile::tempdir().unwrap();
    fixture(before.path(), &[state(Status::InProgress)]);
    for replacement in [
        "status:   \"unknown\"",
        "status:   null",
        "status:   1",
        "status:   \"stuck\"",
        "blocker: null\r\nstatus:   \"stuck\"",
    ] {
        let after = tempfile::tempdir().unwrap();
        fixture(after.path(), &[state(Status::InProgress)]);
        let bad = text(1, &state(Status::InProgress), false)
            .replace("status:   \"in_progress\"", replacement);
        fs::write(file(after.path(), 1), &bad).unwrap();
        let original = inventory(after.path());
        assert!(
            status::normalize(
                &policy,
                before.path(),
                after.path(),
                &BTreeSet::from([id(1)]),
                None
            )
            .is_err()
        );
        assert_eq!(inventory(after.path()), original);
    }
    for explicit in [BTreeSet::from(["bad".into()]), BTreeSet::from([id(99)])] {
        let original = inventory(before.path());
        assert!(status::normalize(&policy, before.path(), before.path(), &explicit, None).is_err());
        assert_eq!(inventory(before.path()), original);
    }
}
