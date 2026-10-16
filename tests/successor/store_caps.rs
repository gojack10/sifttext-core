use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

fn id(n: usize) -> String {
    format!("50000000-0000-4000-8000-{n:012x}")
}
fn node_path(root: &Path, n: usize) -> PathBuf {
    root.join("nodes").join(id(n))
}
fn fixture(count: usize, wide: bool, references: usize) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("nodes")).unwrap();
    fs::create_dir(temp.path().join("trees")).unwrap();
    for n in 1..=count {
        let dir = node_path(temp.path(), n);
        fs::create_dir(&dir).unwrap();
        fs::create_dir(dir.join("children")).unwrap();
        let refs: Vec<_> = if n == 1 {
            (2..references + 2)
                .map(|n| Reference {
                    target: id(n),
                    reason: format!("Synthetic reference {n}"),
                })
                .collect()
        } else {
            vec![]
        };
        fs::write(dir.join("node.md"), format!(
            "---\nid: {:?}\nname: \"Synthetic node {n}\"\nmicro_summary: \"Synthetic public-safe summary\"\nstatus: \"in_progress\"\nsee_also: {}\n---\nSynthetic body {n}.\n",
            id(n), serde_json::to_string(&refs).unwrap())).unwrap();
        let children: Vec<_> = if wide && n == 1 {
            (2..=count).map(id).collect()
        } else {
            vec![]
        };
        fs::write(dir.join("children.order"), order_bytes(&children)).unwrap();
        for child in children {
            symlink(format!("../../{child}"), dir.join("children").join(child)).unwrap();
        }
        if !wide || n == 1 {
            symlink(
                format!("../nodes/{}", id(n)),
                temp.path().join("trees").join(id(n)),
            )
            .unwrap();
        }
    }
    temp
}
fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}
fn graph(store: &Store) -> Graph {
    encode(
        store,
        &store.nodes.keys().cloned().collect::<Vec<_>>(),
        &mut Tokens::default(),
    )
}
fn text(g: &Graph) -> String {
    graph_words(g)
        .unwrap()
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}
fn resource_result<T>(label: &str, result: Result<T>, start: Instant) -> Option<T> {
    match result {
        Ok(value) => {
            eprintln!("{label}: PASS elapsed={:?}", start.elapsed());
            Some(value)
        }
        Err(error) => {
            // This is a measured frozen transport/deadline boundary, NOT validation credit.
            let error = error.to_string();
            assert!(
                error == "policy process timed out"
                    || error == "policy process output exceeds limit",
                "{label}: unexpected failure: {error}"
            );
            eprintln!(
                "{label}: RESOURCE_LIMIT error={error:?} elapsed={:?}",
                start.elapsed()
            );
            None
        }
    }
}

#[test]
fn actual_policy_roundtrips_and_reports_frozen_limits() {
    let policy = policy();
    let temp = fixture(258, true, 257);
    let before = Store::read(temp.path()).unwrap();
    policy.validate(&before).unwrap();
    let renamed = apply(
        &policy,
        &before,
        &Operation::Rename {
            node: id(258),
            name: "Checked synthetic rename".into(),
        },
    )
    .unwrap();
    assert_eq!(renamed.nodes[&id(258)].name, "Checked synthetic rename");
    assert_eq!(renamed.children, before.children);
    assert_eq!(renamed.roots, before.roots);
    assert_eq!(renamed.nodes.len(), before.nodes.len());

    let roots_temp = fixture(257, false, 0);
    policy
        .validate(&Store::read(roots_temp.path()).unwrap())
        .expect("257 roots must validate");

    // Exercise the actual frozen helper with IDs AND opaque symbols above 8192.
    let high = Graph {
        assets: 9001,
        roots: vec![9002],
        nodes: vec![WireNode {
            id: 9002,
            payload: 9003,
            children: vec![],
            refs: vec![],
        }],
    };
    let start = Instant::now();
    let output = resource_result(
        "high-ID helper rename",
        policy.call(3, 9002, 9004, &high),
        start,
    )
    .expect("high-ID and symbol operation must pass");
    let mut expected = high.clone();
    expected.nodes[0].payload = 9004;
    let body = output
        .strip_prefix("STG1 OK ")
        .and_then(|s| s.strip_suffix('\n'))
        .unwrap();
    assert_eq!(parse_graph(body, &high).unwrap(), expected);
}

#[test]
fn impossible_or_malformed_output_fails_before_allocation() {
    let temp = fixture(3, true, 1);
    let request = graph(&Store::read(temp.path()).unwrap());
    let wire = text(&request);
    for bad in [
        String::new(),
        format!("{wire} 0"),
        wire.rsplit_once(' ').unwrap().0.into(),
        wire.replacen(' ', "  ", 1),
        wire.replacen(' ', "\t", 1),
    ] {
        assert!(parse_graph(&bad, &request).is_err(), "accepted {bad:?}");
    }
    let mut words = graph_words(&request).unwrap();
    // roots, nodes, children and refs count fields may not dictate allocations.
    let nn = 2 + request.roots.len();
    let nc = nn + 3;
    let nr = nc + 1 + request.nodes[0].children.len();
    for index in [1, nn, nc, nr] {
        for bad in [
            usize::MAX.to_string(),
            "184467440737095516160".into(),
            "-1".into(),
            "x".into(),
        ] {
            let mut fields = words.iter().map(usize::to_string).collect::<Vec<_>>();
            fields[index] = bad;
            assert!(parse_graph(&fields.join(" "), &request).is_err());
        }
    }
    words[nc] = 1; // inconsistent aggregate edge budget, not merely a huge count
    assert!(
        parse_graph(
            &words
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(" "),
            &request
        )
        .is_err()
    );
    assert!(reserved::<usize>(usize::MAX).is_err());
}

#[test]
fn forged_policy_identities_payloads_edges_and_references_fail_closed() {
    let temp = fixture(3, true, 1);
    let before = Store::read(temp.path()).unwrap();
    let request = graph(&before);
    let operation = Operation::Rename {
        node: id(3),
        name: "Rename".into(),
    };
    let mut tokens = Tokens::default();
    encode(
        &before,
        &before.nodes.keys().cloned().collect::<Vec<_>>(),
        &mut tokens,
    );
    let mut valid = request.clone();
    let original = match &before.inventory[&PathBuf::from(format!("nodes/{}/node.md", id(3)))] {
        Entry::File(b) => b,
        _ => panic!(),
    };
    let mut renamed = String::from_utf8(original.clone()).unwrap();
    let range = markdown(&renamed).unwrap().name_range;
    renamed.replace_range(range, "\"Rename\"");
    valid.nodes[2].payload = tokens.intern(renamed.as_bytes());
    let mut cases = vec![];
    let mut bad = valid.clone();
    bad.assets += 1;
    cases.push(bad);
    let mut bad = valid.clone();
    bad.roots[0] = 2;
    cases.push(bad);
    let mut bad = valid.clone();
    bad.nodes[1].id = 1;
    cases.push(bad);
    let mut bad = valid.clone();
    bad.nodes[0].payload += 1;
    cases.push(bad);
    let mut bad = valid.clone();
    bad.nodes[0].refs[0].0 = 3;
    cases.push(bad);
    let mut bad = valid.clone();
    bad.nodes[0].refs[0].1 += 1;
    cases.push(bad);
    for child in [0, 1, 2, usize::MAX] {
        let mut bad = valid.clone();
        bad.nodes[0].children[1] = child;
        cases.push(bad);
    }
    let helper = temp.path().join("helper");
    for bad in cases {
        fs::write(
            &helper,
            format!("#!/bin/sh\nprintf '%s\\n' 'STG1 OK {}'\n", text(&bad)),
        )
        .unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let fake = Policy {
            project: policy().project,
            executable: helper.clone(),
        };
        assert!(plan(&fake, &before, &operation).is_err());
        assert_eq!(Store::read(temp.path()).unwrap().nodes, before.nodes);
    }
}

#[test]
fn graph_and_filesystem_safety_survive_cap_removal() {
    let policy = policy();
    // These remain genuine graph rejects by the frozen policy, not reader cap errors.
    let temp = fixture(3, true, 0);
    let valid = Store::read(temp.path()).unwrap();
    let mut bad = valid.clone();
    bad.children.get_mut(&id(1)).unwrap().pop();
    assert_eq!(
        policy.validate(&bad).unwrap_err().to_string(),
        "checked graph rejected or malformed verdict"
    ); // detached
    let mut bad = valid.clone();
    bad.children.get_mut(&id(2)).unwrap().push(id(3));
    assert_eq!(
        policy.validate(&bad).unwrap_err().to_string(),
        "checked graph rejected or malformed verdict"
    ); // multi-parent
    let mut bad = valid.clone();
    bad.children.get_mut(&id(2)).unwrap().push(id(1));
    assert_eq!(
        policy.validate(&bad).unwrap_err().to_string(),
        "checked graph rejected or malformed verdict"
    ); // cycle/root parent
    let mut bad = valid.clone();
    bad.nodes.get_mut(&id(1)).unwrap().see_also.push(Reference {
        target: id(999),
        reason: "Missing".into(),
    });
    assert_eq!(
        policy.validate(&bad).unwrap_err().to_string(),
        "checked graph rejected or malformed verdict"
    );
    fs::write(
        node_path(temp.path(), 1).join("children.order"),
        format!("{}\n{}\n", id(2), id(2)),
    )
    .unwrap();
    assert!(Store::read(temp.path()).is_err());
    fs::write(
        node_path(temp.path(), 1).join("children.order"),
        order_bytes(&valid.children[&id(1)]),
    )
    .unwrap();
    let md = node_path(temp.path(), 2).join("node.md");
    let original = fs::read(&md).unwrap();
    fs::write(
        &md,
        String::from_utf8(original.clone())
            .unwrap()
            .replacen(&id(2), &id(1), 1),
    )
    .unwrap();
    assert!(Store::read(temp.path()).is_err());
    fs::write(&md, &original).unwrap();
    fs::hard_link(&md, temp.path().join("hardlink")).unwrap();
    assert!(Store::read(temp.path()).is_err());
    fs::remove_file(temp.path().join("hardlink")).unwrap();
    symlink("/tmp", temp.path().join("escape")).unwrap();
    assert!(Store::read(temp.path()).is_err());
    fs::remove_file(temp.path().join("escape")).unwrap();
    fs::write(temp.path().join("oversized"), vec![0; LIMIT + 1]).unwrap();
    assert!(Store::read(temp.path()).is_err());
}

#[test]
fn sparse_composition_above_256_preserves_complete_base() {
    let base_temp = fixture(257, false, 0);
    let base = Store::read(base_temp.path()).unwrap();
    let request = graph(&base);
    assert_eq!(parse_graph(&text(&request), &request).unwrap(), request);
    let overlay = tempfile::tempdir().unwrap();
    fs::create_dir(overlay.path().join("nodes")).unwrap();
    fs::create_dir(overlay.path().join("trees")).unwrap();
    for n in 1..=257 {
        symlink(
            format!("../nodes/{}", id(n)),
            overlay.path().join("trees").join(id(n)),
        )
        .unwrap();
    }
    let view = Store::read_view(overlay.path(), Some(&base)).unwrap();
    assert_eq!(view.nodes, base.nodes);
    assert_eq!(view.children, base.children);
    assert_eq!(view.roots, base.roots);
    for (path, entry) in &base.inventory {
        if path.starts_with("nodes") {
            assert_eq!(view.inventory.get(path), Some(entry));
        }
    }
    assert_eq!(
        fs::read_dir(overlay.path().join("nodes")).unwrap().count(),
        0
    );
}
