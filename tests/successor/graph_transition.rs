use super::*;
use crate::{Authored, Reference, Store};
use std::{os::unix::fs::symlink, time::Instant};

fn id(n: usize) -> String {
    format!("71000000-0000-4000-8000-{n:012x}")
}
fn fixture(path: &Path, count: usize, deep: bool) -> Store {
    fs::create_dir_all(path.join("nodes")).unwrap();
    fs::create_dir_all(path.join("trees")).unwrap();
    for n in 1..=count {
        let dir = path.join("nodes").join(id(n));
        fs::create_dir_all(dir.join("children")).unwrap();
        let children = if deep && n < count {
            vec![n + 1]
        } else if !deep && n == 1 {
            (2..=count).collect()
        } else {
            vec![]
        };
        fs::write(
            dir.join("children.order"),
            children
                .iter()
                .map(|n| format!("{}\n", id(*n)))
                .collect::<String>(),
        )
        .unwrap();
        for child in children {
            symlink(
                format!("../../{}", id(child)),
                dir.join("children").join(id(child)),
            )
            .unwrap();
        }
        let node = Authored {
            id: id(n),
            name: format!("Node {n}"),
            micro_summary: "Synthetic summary".into(),
            content: format!("Body {n}\r\n雪"),
            see_also: if n == 1 && count > 1 {
                vec![Reference {
                    target: id(count),
                    reason: "Synthetic reason".into(),
                }]
            } else {
                vec![]
            },
        };
        fs::write(dir.join("node.md"), format!("---\nid: {}\nname: {}\nmicro_summary: {}\nsee_also: {}\nstatus: \"in_progress\"\n---\n{}", serde_json::to_string(&node.id).unwrap(), serde_json::to_string(&node.name).unwrap(), serde_json::to_string(&node.micro_summary).unwrap(), serde_json::to_string(&node.see_also).unwrap(), node.content)).unwrap();
    }
    symlink(
        format!("../nodes/{}", id(1)),
        path.join("trees").join(id(1)),
    )
    .unwrap();
    Store::read(path).unwrap()
}
fn append(path: &Path, n: usize, body: &str) {
    let path = path.join("nodes").join(id(n)).join("node.md");
    let mut bytes = fs::read(&path).unwrap();
    bytes.extend(body.as_bytes());
    fs::write(path, bytes).unwrap();
}
fn patch(path: &Path, n: usize, old: &str, new: &str) {
    let path = path.join("nodes").join(id(n)).join("node.md");
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains(old));
    fs::write(path, text.replace(old, new)).unwrap();
}
fn preserved(p: Preservation) -> bool {
    matches!(p, Preservation::Preserved { .. })
}

fn raw_observation(
    executable: &Path,
    words: Vec<usize>,
    evidence: &Path,
    serial: &std::cell::Cell<usize>,
) -> String {
    use std::{
        collections::HashSet,
        os::unix::process::ExitStatusExt,
        process::{Command, Stdio},
        thread,
        time::Duration,
    };
    serial.set(serial.get() + 1);
    let prefix = evidence.join(serial.get().to_string());
    let wire = words
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    assert!(wire.len() < crate::LIMIT);
    fs::write(prefix.with_extension("wire"), &wire).unwrap();
    let start = Instant::now();
    let mut child = Command::new(executable)
        .args(["--threads", "1", "--gpu", "off"])
        .env("SIFTTEXT_WIRE", prefix.with_extension("wire"))
        .stdin(Stdio::null())
        .stdout(fs::File::create(prefix.with_extension("stdout")).unwrap())
        .stderr(fs::File::create(prefix.with_extension("stderr")).unwrap())
        .spawn()
        .unwrap();
    let mut peak = 0;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() >= Duration::from_secs(10) {
            child.kill().unwrap();
            break child.wait().unwrap();
        }
        if let Ok(ps) = Command::new("/bin/ps")
            .args(["-axo", "pid=,ppid=,rss="])
            .output()
            && ps.status.success()
        {
            let rows = String::from_utf8_lossy(&ps.stdout)
                .lines()
                .filter_map(|line| {
                    let row = line
                        .split_whitespace()
                        .map(str::parse::<u64>)
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .ok()?;
                    (row.len() == 3).then(|| (row[0], row[1], row[2]))
                })
                .collect::<Vec<_>>();
            let mut pids = HashSet::from([u64::from(child.id())]);
            loop {
                let previous = pids.len();
                for &(pid, parent, _) in &rows {
                    if pids.contains(&parent) {
                        pids.insert(pid);
                    }
                }
                if pids.len() == previous {
                    break;
                }
            }
            peak = peak.max(
                rows.iter()
                    .filter(|(pid, _, _)| pids.contains(pid))
                    .map(|(_, _, rss)| rss * 1024)
                    .sum::<u64>(),
            );
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(prefix.with_extension("stdout")).unwrap();
    let stderr = fs::read_to_string(prefix.with_extension("stderr")).unwrap();
    let receipt = serde_json::json!({"executable":executable,"code":status.code(),"signal":status.signal(),
        "elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"sampled_aggregate_helper_rss_bytes":peak,"wire_bytes":wire.len(),
        "limits":"10ms process-tree samples can miss peaks/short helpers and perturb timing; raw predicate is conditional, not base/candidate validity"});
    fs::write(
        prefix.with_extension("json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    assert!(status.success(), "{receipt}: {stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert!(stdout.len() <= crate::LIMIT);
    stdout
}

// Differential expectations are independent of the preservation predicate.
// Raw equality is conditional: even a self-equal invalid graph is preserved,
// so these observations do not mint a ProvenBase or accept a candidate.
#[test]
fn compiled_transition_full_law_oracle() {
    use crate::WireNode;
    let project = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::tempdir().unwrap();
    let policy = Policy::checked(project).unwrap();
    let transition = Transition::compile(&policy, &temp.path().join("compiled")).unwrap();
    let original = Graph {
        assets: 1,
        roots: vec![1],
        nodes: vec![
            WireNode {
                id: 1,
                payload: 2,
                children: vec![2, 3],
                refs: vec![(3, 7)],
            },
            WireNode {
                id: 2,
                payload: 3,
                children: vec![],
                refs: vec![],
            },
            WireNode {
                id: 3,
                payload: 4,
                children: vec![],
                refs: vec![],
            },
        ],
    };
    let evidence = std::env::var_os("SIFTTEXT_TRANSITION_ORACLE_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("observations"));
    fs::create_dir(&evidence).unwrap();
    let serial = std::cell::Cell::new(0);
    let mut observations = 0;
    let mut full = |graph: &Graph, valid: bool| {
        policy.verify_stamp().unwrap();
        let wire = [vec![1, 0, 0, 0], graph_words(graph).unwrap()].concat();
        let output = raw_observation(&policy.executable, wire, &evidence, &serial);
        policy.verify_stamp().unwrap();
        assert_eq!(
            output,
            if valid {
                "STG1 VALID\n"
            } else {
                "STG1 REJECT\n"
            }
        );
        observations += 1;
    };
    let mut transitions = 0;
    let mut preserved = |before: &Graph, after: &Graph, expected| {
        transition.verify().unwrap();
        let before = graph_words(before).unwrap();
        let wire = [vec![1, before.len()], before, graph_words(after).unwrap()].concat();
        let output = raw_observation(&transition.executable, wire, &evidence, &serial);
        transition.verify().unwrap();
        assert_eq!(
            output,
            if expected {
                "STGT1 PRESERVED\n"
            } else {
                "STGT1 FALLBACK\n"
            }
        );
        transitions += 1;
    };
    full(&original, true);
    preserved(&original, &original, true);
    let mut body = original.clone();
    body.nodes[1].payload = 99;
    preserved(&original, &body, true);
    full(&body, true);
    for (label, valid) in [
        ("insert", true),
        ("delete-reference-conflict", false),
        ("cycle", false),
        ("dangling-child", false),
        ("duplicate-child", false),
        ("duplicate-root", false),
        ("detached", false),
        ("dangling-reference", false),
        ("zero-reason", false),
        ("move", true),
        ("reorder", true),
        ("reason", true),
        ("identity", false),
        ("asset", true),
    ] {
        let mut next = original.clone();
        match label {
            "insert" => {
                next.nodes.push(WireNode {
                    id: 4,
                    payload: 8,
                    children: vec![],
                    refs: vec![],
                });
                next.nodes[0].children.push(4);
            }
            "delete-reference-conflict" => {
                next.nodes.pop();
                next.nodes[0].children.pop();
            }
            "cycle" => next.nodes[1].children.push(1),
            "dangling-child" => next.nodes[0].children.push(99),
            "duplicate-child" => next.nodes[0].children.push(2),
            "duplicate-root" => next.roots.push(1),
            "detached" => {
                next.nodes[0].children.pop();
            }
            "dangling-reference" => next.nodes[0].refs[0].0 = 99,
            "zero-reason" => next.nodes[0].refs[0].1 = 0,
            "move" => {
                next.nodes[0].children.pop();
                next.nodes[1].children.push(3);
            }
            "reorder" => next.nodes[0].children.reverse(),
            "reason" => next.nodes[0].refs[0].1 = 8,
            "identity" => next.nodes[2].id = 99,
            "asset" => next.assets += 1,
            _ => unreachable!(),
        }
        eprintln!("TRANSITION_ORACLE {label}");
        preserved(&original, &next, false);
        full(&next, valid);
        if !valid {
            preserved(&next, &next, true);
        }
    }
    for (count, deep) in [(100, true), (1000, true), (1698, false), (4096, false)] {
        let graph = Graph {
            assets: 1,
            roots: vec![1],
            nodes: (1..=count)
                .map(|id| WireNode {
                    id,
                    payload: id + 1,
                    children: if deep && id < count {
                        vec![id + 1]
                    } else if !deep && id == 1 {
                        (2..=count).collect()
                    } else {
                        vec![]
                    },
                    refs: vec![],
                })
                .collect(),
        };
        preserved(&graph, &graph, true);
        let mut next = graph.clone();
        next.nodes.last_mut().unwrap().children.push(1);
        preserved(&graph, &next, false);
    }
    assert_eq!(observations + transitions, 49);
    eprintln!("TRANSITION_ORACLE {observations} full-law + {transitions} conditional = 49");
}

#[test]
fn compiled_native_transition_contract() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::tempdir().unwrap();
    let build = std::env::var_os("SIFTTEXT_TRANSITION_BUILD")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("compiled"));
    let policy = Policy::checked(project).unwrap();
    let transition = Transition::compile(&policy, &build).unwrap();
    let before = temp.path().join("base");
    fixture(&before, 8, false);
    let base = transition.prove_base(&before).unwrap();
    let after = temp.path().join("after");
    fixture(&after, 8, false);
    assert!(preserved(
        transition
            .check(Some(&base), base.version(), &after)
            .unwrap()
    ));
    append(&after, 2, "\nDifferent body with UUID-shaped prose only");
    assert!(preserved(
        transition
            .check(Some(&base), base.version(), &after)
            .unwrap()
    ));
    policy.validate(&Store::read(&after).unwrap()).unwrap();
    assert_eq!(
        transition.check(None, base.version(), &after).unwrap(),
        Preservation::FullLawRequired("unproven base")
    );
    assert!(transition.check(Some(&base), "forged", &after).is_err());

    // Every graph/metadata change is rejected or requires the unchanged fallback.
    for change in [
        "insert",
        "delete",
        "move",
        "reorder",
        "reference",
        "reason",
        "status",
        "name",
        "asset",
        "merged-conflict",
    ] {
        let path = temp.path().join(change);
        fixture(&path, if change == "insert" { 9 } else { 8 }, false);
        match change {
            "insert" => (),
            "delete" | "merged-conflict" => {
                fs::remove_dir_all(path.join("nodes").join(id(8))).unwrap();
                fs::remove_file(path.join("nodes").join(id(1)).join("children").join(id(8)))
                    .unwrap();
                let order = path.join("nodes").join(id(1)).join("children.order");
                fs::write(
                    &order,
                    fs::read_to_string(&order)
                        .unwrap()
                        .replace(&format!("{}\n", id(8)), ""),
                )
                .unwrap();
                // Retained source-1 reference is a cross-file merge conflict.
            }
            "move" => {
                let root = path.join("nodes").join(id(1));
                let parent = path.join("nodes").join(id(2));
                fs::rename(
                    root.join("children").join(id(3)),
                    parent.join("children").join(id(3)),
                )
                .unwrap();
                let order = root.join("children.order");
                fs::write(
                    &order,
                    fs::read_to_string(&order)
                        .unwrap()
                        .replace(&format!("{}\n", id(3)), ""),
                )
                .unwrap();
                fs::write(parent.join("children.order"), format!("{}\n", id(3))).unwrap();
            }
            "reorder" => fs::write(
                path.join("nodes").join(id(1)).join("children.order"),
                (2..=8)
                    .rev()
                    .map(|n| format!("{}\n", id(n)))
                    .collect::<String>(),
            )
            .unwrap(),
            "reference" => patch(&path, 1, &id(8), &id(7)),
            "reason" => patch(&path, 1, "Synthetic reason", "Changed reason"),
            "status" => patch(&path, 1, "in_progress", "not_examined"),
            "name" => patch(&path, 1, "Node 1", "Renamed"),
            "asset" => fs::write(path.join("extra.txt"), "new asset").unwrap(),
            _ => unreachable!(),
        }
        assert!(
            !preserved(
                transition
                    .check(Some(&base), base.version(), &path)
                    .unwrap()
            ),
            "{change}"
        );
        if matches!(change, "delete" | "merged-conflict") {
            assert!(transition.prove_base(&path).is_err());
            assert!(policy.validate(&Store::read(&path).unwrap()).is_err());
        }
    }
    // Invalid/unproven base is never minted, even if its candidate is identical.
    let invalid = temp.path().join("invalid");
    fixture(&invalid, 8, true);
    symlink(
        format!("../../{}", id(1)),
        invalid
            .join("nodes")
            .join(id(8))
            .join("children")
            .join(id(1)),
    )
    .unwrap();
    fs::write(
        invalid.join("nodes").join(id(8)).join("children.order"),
        format!("{}\n", id(1)),
    )
    .unwrap();
    assert!(transition.prove_base(&invalid).is_err());

    // Invariant equality is actually checked by compiled Bend, not just Rust's
    // edit-class guard. Exercise each projected field, and payload-only positive.
    let s = Store::read(&before).unwrap();
    let ids = s.nodes.keys().cloned().collect::<Vec<_>>();
    let graph = encode(&s, &ids, &mut Tokens::default());
    assert!(transition.run(&graph, &graph).unwrap());
    for mutation in 0..9 {
        let mut next = graph.clone();
        match mutation {
            0 => next.assets += 1,
            1 => next.roots.push(1),
            2 => {
                next.nodes.remove(1);
            }
            3 => {
                next.nodes[0].id = 99;
            }
            4 => next.nodes[0].children.reverse(),
            5 => next.nodes[0].children.push(99),
            6 => next.nodes[0].refs[0].0 = 99,
            7 => next.nodes[0].refs[0].1 = 0,
            _ => next.nodes[0].refs.push((1, 1)),
        };
        assert!(
            !transition.run(&graph, &next).unwrap(),
            "Bend mutation {mutation}"
        );
    }
    let mut payload = graph.clone();
    payload.nodes[0].payload += 999;
    assert!(transition.run(&graph, &payload).unwrap());

    // Capture-before-accept races: candidate edit, same-byte inode replacement,
    // and base change. No test-only path can bypass the real final barriers.
    assert!(
        transition
            .check_before_accept(Some(&base), base.version(), &after, || append(
                &after, 2, "raced"
            ))
            .is_err()
    );
    assert!(
        transition
            .check_before_accept(Some(&base), base.version(), &after, || {
                let p = after.join("nodes").join(id(2)).join("node.md");
                let bytes = fs::read(&p).unwrap();
                fs::remove_file(&p).unwrap();
                fs::write(p, bytes).unwrap();
            })
            .is_err()
    );
    assert!(
        transition
            .check_before_accept(Some(&base), base.version(), &after, || append(
                &before,
                2,
                "base raced"
            ))
            .is_err()
    );
    assert!(
        transition
            .check(Some(&base), base.version(), &after)
            .is_err()
    );

    // Graph preservation never asserts token/body/status acceptance.
    let clean = temp.path().join("clean");
    fixture(&clean, 2, false);
    let proven = transition.prove_base(&clean).unwrap();
    let over = temp.path().join("over");
    fixture(&over, 2, false);
    append(&over, 2, &"word ".repeat(6000));
    assert!(preserved(
        transition
            .check(Some(&proven), proven.version(), &over)
            .unwrap()
    ));
    let report = crate::candidate_validation::validate(
        &policy,
        &crate::candidate_validation::Request {
            store: over,
            before: Some(clean),
            explicit: Default::default(),
            expected_version: None,
            pause_before_accept: None,
        },
    );
    assert_eq!(report.status, "rejected");
    assert!(
        report
            .rows
            .iter()
            .any(|r| r.check == "crystallization_over_limit")
    );

    // A compiled artifact changed after compilation cannot provide a verdict.
    let original = fs::read(&transition.executable).unwrap();
    fs::write(&transition.executable, b"forged").unwrap();
    assert!(transition.check(None, "", &after).is_err());
    fs::write(&transition.executable, original).unwrap();

    for (count, deep) in [(100, true), (1000, true), (1698, false), (4096, false)] {
        let path = temp.path().join(format!("shape-{count}"));
        let s = fixture(&path, count, deep);
        let start = Instant::now();
        let proof = transition.prove_base(&path);
        let full_ms = start.elapsed().as_millis();
        let ids = s.nodes.keys().cloned().collect::<Vec<_>>();
        let mut tokens = Tokens::default();
        let graph = encode(&s, &ids, &mut tokens);
        let start = Instant::now();
        assert!(transition.run(&graph, &graph).unwrap());
        let bend_ms = start.elapsed().as_millis();
        match proof {
            Ok(base) => {
                append(&path, 2, "edited base"); // stale even on a large shape
                assert!(
                    transition
                        .check(Some(&base), base.version(), &path)
                        .is_err()
                );
                eprintln!(
                    "TRANSITION count={count} deep={deep} full_valid=true full_ms={full_ms} bend_ms={bend_ms}"
                );
            }
            Err(e) => {
                assert_eq!(e.to_string(), "policy process timed out");
                eprintln!(
                    "TRANSITION count={count} deep={deep} FULL_BASE_NOT_ESTABLISHED full_ms={full_ms} bend_ms={bend_ms} error={e}"
                );
            }
        }
    }
    // Persist no private source; build location is a synthetic test artifact.
    eprintln!("TRANSITION_COMPILED {}", transition.executable.display());
}
