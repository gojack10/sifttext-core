//! Differential closure/refusal ordering, including cyclic/shared-child inputs.
use crate::status::{State, Status, child_gate};
use std::collections::{BTreeMap, BTreeSet};

type States = BTreeMap<String, State>;
type Children = BTreeMap<String, Vec<String>>;

fn wave_model(states: &mut States, children: &Children, explicit: &BTreeSet<String>) -> bool {
    loop {
        let blocked: Vec<_> = states
            .iter()
            .filter(|(id, s)| {
                s.status == Status::Resolved
                    && children[*id].iter().any(|c| states[c].status.blocks())
            })
            .map(|(id, _)| id.clone())
            .collect();
        if blocked.is_empty() {
            return true;
        }
        for id in blocked {
            if explicit.contains(&id) {
                return false;
            }
            states.get_mut(&id).unwrap().status = Status::InProgress;
        }
    }
}

fn state(status: Status) -> State {
    State {
        status,
        reasons: [("sentinel".into(), "preserve".into())].into(),
    }
}

#[test]
fn child_gate_matches_wave_model() {
    // All directed graphs on three nodes (self/cycle/shared edges included),
    // all blocking/resolved/nonblocking placements and all explicit subsets.
    let choices = [Status::InProgress, Status::Resolved, Status::Deferred];
    for edges in 0..512 {
        let children: Children = (0..3)
            .map(|i| {
                (
                    i.to_string(),
                    (0..3)
                        .filter(|j| edges & (1 << (i * 3 + j)) != 0)
                        .map(|j| j.to_string())
                        .collect(),
                )
            })
            .collect();
        for placement in 0..27 {
            let states: States = (0..3)
                .map(|i| {
                    (
                        i.to_string(),
                        state(choices[(placement / 3usize.pow(i)) % 3]),
                    )
                })
                .collect();
            for mask in 0..8 {
                let explicit = (0..3)
                    .filter(|i| mask & (1 << i) != 0)
                    .map(|i| i.to_string())
                    .collect();
                let mut expected = states.clone();
                let mut actual = states.clone();
                let ok = wave_model(&mut expected, &children, &explicit);
                let result = child_gate(&mut actual, &children, &explicit);
                assert_eq!(
                    result.is_ok(),
                    ok,
                    "edges={edges} placement={placement} explicit={mask}"
                );
                if let Err(error) = result {
                    assert_eq!(error.to_string(), "active direct child blocks resolution");
                }
                assert_eq!(
                    actual, expected,
                    "partial states differ: edges={edges} placement={placement} explicit={mask}"
                );
            }
        }
    }
    // Exercise every authored status, not just the three behavior classes above.
    for status in [
        Status::NotExamined,
        Status::InProgress,
        Status::Stuck,
        Status::Deferred,
        Status::Discarded,
        Status::Resolved,
    ] {
        let mut states = [
            ("p".into(), state(Status::Resolved)),
            ("c".into(), state(status)),
        ]
        .into();
        let children = [("p".into(), vec!["c".into()]), ("c".into(), vec![])].into();
        child_gate(&mut states, &children, &BTreeSet::new()).unwrap();
        assert_eq!(
            states["p"].status,
            if status.blocks() {
                Status::InProgress
            } else {
                Status::Resolved
            }
        );
    }
}

#[test]
#[ignore = "bounded pure-function scaling probe, not complete-command acceptance"]
fn child_gate_chain_scaling() {
    for n in [1000, 2000, 4000] {
        let states: States = (0..n)
            .map(|i| {
                (
                    format!("{i:08}"),
                    state(if i + 1 == n {
                        Status::InProgress
                    } else {
                        Status::Resolved
                    }),
                )
            })
            .collect();
        let children: Children = (0..n)
            .map(|i| {
                (
                    format!("{i:08}"),
                    if i + 1 == n {
                        vec![]
                    } else {
                        vec![format!("{:08}", i + 1)]
                    },
                )
            })
            .collect();
        let mut old = states.clone();
        let start = std::time::Instant::now();
        assert!(wave_model(&mut old, &children, &BTreeSet::new()));
        let old_time = start.elapsed();
        let mut new = states.clone();
        let start = std::time::Instant::now();
        child_gate(&mut new, &children, &BTreeSet::new()).unwrap();
        let new_time = start.elapsed();
        assert_eq!(new, old);
        assert!(new.values().all(|s| s.status == Status::InProgress));
        eprintln!(
            "child_gate chain={n} model_us={} worklist_us={}",
            old_time.as_micros(),
            new_time.as_micros()
        );
    }
}
