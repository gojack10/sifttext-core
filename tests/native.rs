use sifttext_file_graph::{
    Authored, Operation, Policy, Reference, Store, apply, invoke, parse_markdown, sha256,
    validate_draft,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

fn id(n: usize) -> String {
    format!("00000000-0000-0000-0000-{n:012x}")
}
fn temp() -> TempDir {
    match std::env::var("SIFTTEXT_EVIDENCE_DIR") {
        Ok(p) => tempfile::Builder::new()
            .prefix("implementation-native-")
            .tempdir_in(p)
            .unwrap(),
        Err(_) => tempfile::tempdir().unwrap(),
    }
}
fn node_file(root: &Path, n: usize) -> PathBuf {
    root.join(format!("nodes/{}/node.md", id(n)))
}
fn order(root: &Path, n: usize, cs: &[usize]) {
    let text = cs.iter().map(|&n| id(n) + "\n").collect::<String>();
    fs::write(root.join(format!("nodes/{}/children.order", id(n))), text).unwrap();
}
fn link(root: &Path, parent: usize, child: usize) {
    std::os::unix::fs::symlink(
        format!("../../{}", id(child)),
        root.join(format!("nodes/{}/children/{}", id(parent), id(child))),
    )
    .unwrap();
}
fn fixture(root: &Path) {
    fs::create_dir_all(root.join("trees")).unwrap();
    for n in 1..=7 {
        fs::create_dir_all(root.join(format!("nodes/{}/children", id(n)))).unwrap();
        fs::write(root.join(format!("nodes/{}/children/.keep", id(n))), []).unwrap();
        let refs = match n {
            2 => vec![Reference {
                target: id(5),
                reason: "A follows D's evidence".into(),
            }],
            7 => vec![Reference {
                target: id(3),
                reason: "E depends on B".into(),
            }],
            _ => vec![],
        };
        let node = Authored {
            id: id(n),
            name: format!("Node {n}"),
            micro_summary: format!("Summary {n}"),
            content: format!("Body {n}: prose [[not-an-edge]]\n"),
            see_also: refs,
        };
        fs::write(node_file(root, n), authored_markdown(&node)).unwrap();
        order(root, n, &[]);
    }
    for (parent, cs) in [(1, vec![2, 3, 4]), (3, vec![5]), (6, vec![7])] {
        for &child in &cs {
            link(root, parent, child);
        }
        order(root, parent, &cs);
    }
    for r in [1, 6] {
        std::os::unix::fs::symlink(
            format!("../nodes/{}", id(r)),
            root.join(format!("trees/{}", id(r))),
        )
        .unwrap();
    }
    fs::create_dir_all(root.join(format!("nodes/{}/code", id(3)))).unwrap();
    fs::write(
        root.join(format!("nodes/{}/code/sentinel.bin", id(3))),
        [0, 255, 1, 128],
    )
    .unwrap();
    fs::write(root.join("evidence.txt"), "keep me exactly\n").unwrap();
}
fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}
fn get(root: &Path) -> Store {
    Store::read(root).unwrap()
}
fn authored_markdown(n: &Authored) -> String {
    format!(
        "---\nid: {}\nname: {}\nmicro_summary: {}\nsee_also: {}\n---\n{}",
        serde_json::to_string(&n.id).unwrap(),
        serde_json::to_string(&n.name).unwrap(),
        serde_json::to_string(&n.micro_summary).unwrap(),
        serde_json::to_string(&n.see_also).unwrap(),
        n.content,
    )
}
fn edit_node(root: &Path, n: usize, f: impl FnOnce(&mut Authored)) {
    let mut node = parse_markdown(&fs::read_to_string(node_file(root, n)).unwrap()).unwrap();
    f(&mut node);
    fs::write(node_file(root, n), authored_markdown(&node)).unwrap();
}
fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

#[test]
fn positive_move_reorder_rename_and_ordinary_git_revert() {
    let dir = temp();
    let root = dir.path();
    fixture(root);
    let p = policy();
    let before = get(root);
    p.validate(&before).unwrap();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.name", "Synthetic Fixture"]);
    git(root, &["config", "user.email", "fixture@example.invalid"]);
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "fixture: retain original graph"]);
    let move_op = Operation::Move {
        node: id(3),
        parent: id(2),
    };
    let moved = apply(&p, &before, &move_op).unwrap();
    assert_eq!(moved.children[&id(1)], [id(2), id(4)]);
    assert_eq!(moved.children[&id(2)], [id(3)]);
    assert_eq!(moved.children[&id(3)], [id(5)]);
    assert_eq!(before.nodes, moved.nodes);
    for (path, entry) in &before.inventory {
        if !path.to_string_lossy().contains("/children/") && !path.ends_with("children.order") {
            assert_eq!(moved.inventory.get(path), Some(entry), "preserve {path:?}");
        }
    }
    let diff = git(root, &["diff", "--no-ext-diff"]);
    assert!(String::from_utf8_lossy(&diff).contains("children.order"));
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "fixture: checked native move"]);
    git(root, &["revert", "--no-edit", "HEAD"]);
    let restored = get(root);
    assert_eq!(before.inventory, restored.inventory);
    assert!(git(root, &["diff", "--exit-code"]).is_empty());
    let reordered = apply(
        &p,
        &restored,
        &Operation::Reorder {
            node: id(3),
            after: id(4),
        },
    )
    .unwrap();
    assert_eq!(reordered.children[&id(1)], [id(2), id(4), id(3)]);
    assert_eq!(reordered.nodes, before.nodes);
    assert_eq!(reordered.roots, before.roots);
    let renamed = apply(
        &p,
        &reordered,
        &Operation::Rename {
            node: id(3),
            name: "B renamed".into(),
        },
    )
    .unwrap();
    assert_eq!(renamed.nodes[&id(7)].see_also[0].target, id(3));
    let view = renamed.view();
    let e = view["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == id(7))
        .unwrap();
    assert_eq!(e["navigation"][0]["name"], "B renamed");
    assert_eq!(e["navigation"][0]["micro_summary"], "Summary 3");
    assert_eq!(renamed.nodes[&id(3)].content, before.nodes[&id(3)].content);
    assert_eq!(
        renamed.nodes[&id(3)].micro_summary,
        before.nodes[&id(3)].micro_summary
    );
    assert_eq!(renamed.children, reordered.children);
    assert_eq!(
        renamed.inventory[&PathBuf::from("evidence.txt")],
        before.inventory[&PathBuf::from("evidence.txt")]
    );
    if std::env::var_os("SIFTTEXT_EVIDENCE_DIR").is_some() {
        let kept = dir.keep();
        fs::write(
            kept.join("../implementation-latest-demo-path.txt"),
            kept.to_string_lossy().as_bytes(),
        )
        .unwrap();
        fs::write(kept.join("../implementation-native-move.diff"), diff).unwrap();
        fs::write(
            kept.join("../implementation-native-final.json"),
            serde_json::to_vec_pretty(&view).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn completed_native_private_draft_is_accepted_and_tampering_is_preserved() {
    let a = temp();
    let b = temp();
    fixture(a.path());
    fixture(b.path());
    let before = get(a.path());
    let source = b.path().join(format!("nodes/{}/children/{}", id(1), id(3)));
    let dest = b.path().join(format!("nodes/{}/children/{}", id(2), id(3)));
    assert!(
        Command::new("mv")
            .arg(source)
            .arg(dest)
            .status()
            .unwrap()
            .success()
    );
    order(b.path(), 1, &[2, 4]);
    order(b.path(), 2, &[3]);
    let after = get(b.path());
    let op = Operation::Move {
        node: id(3),
        parent: id(2),
    };
    validate_draft(&policy(), &before, &after, &op).unwrap();
    fs::write(b.path().join("evidence.txt"), "tampered\n").unwrap();
    let tampered = get(b.path());
    assert!(validate_draft(&policy(), &before, &tampered, &op).is_err());
    assert_eq!(tampered.inventory, get(b.path()).inventory);
}

#[test]
fn valid_reorder_draft_and_native_rename_are_accepted() {
    let a = temp();
    let b = temp();
    fixture(a.path());
    fixture(b.path());
    order(b.path(), 1, &[2, 4, 3]);
    validate_draft(
        &policy(),
        &get(a.path()),
        &get(b.path()),
        &Operation::Reorder {
            node: id(3),
            after: id(4),
        },
    )
    .unwrap();
    let c = temp();
    fixture(c.path());
    edit_node(c.path(), 3, |n| n.name = "Native name".into());
    validate_draft(
        &policy(),
        &get(a.path()),
        &get(c.path()),
        &Operation::Rename {
            node: id(3),
            name: "Native name".into(),
        },
    )
    .unwrap();
}

#[test]
fn illegal_operations_are_non_mutating() {
    let dir = temp();
    fixture(dir.path());
    let before = get(dir.path());
    for op in [
        Operation::Move {
            node: id(3),
            parent: id(5),
        },
        Operation::Move {
            node: id(1),
            parent: id(2),
        },
        Operation::Reorder {
            node: id(3),
            after: id(7),
        },
        Operation::Reorder {
            node: id(3),
            after: id(3),
        },
        Operation::Rename {
            node: id(3),
            name: " \n".into(),
        },
        Operation::Move {
            node: id(8),
            parent: id(2),
        },
    ] {
        assert!(
            apply(&policy(), &before, &op).is_err(),
            "must reject {op:?}"
        );
        assert_eq!(before.inventory, get(dir.path()).inventory);
    }
}

#[test]
fn duplicate_parent_and_cycle_drafts_are_rejected_but_remain_inspectable() {
    for mode in [0, 1] {
        let dir = temp();
        fixture(dir.path());
        if mode == 0 {
            link(dir.path(), 2, 3);
            order(dir.path(), 2, &[3]);
        } else {
            fs::remove_file(
                dir.path()
                    .join(format!("nodes/{}/children/{}", id(1), id(3))),
            )
            .unwrap();
            order(dir.path(), 1, &[2, 4]);
            link(dir.path(), 5, 3);
            order(dir.path(), 5, &[3]);
        }
        let invalid = get(dir.path());
        assert!(policy().validate(&invalid).is_err());
        assert_eq!(invalid.inventory, get(dir.path()).inventory);
    }
}

#[test]
fn malformed_files_and_unsafe_entries_reject_without_cleanup() {
    for mode in 0..12 {
        let dir = temp();
        fixture(dir.path());
        match mode {
            0 => order(dir.path(), 1, &[2, 3]),
            1 => order(dir.path(), 1, &[2, 3, 3, 4]),
            2 => order(dir.path(), 1, &[2, 3, 4, 7]),
            3 => edit_node(dir.path(), 2, |n| n.id = id(3)),
            4 => edit_node(dir.path(), 2, |n| n.see_also[0].target = id(8)),
            5 => edit_node(dir.path(), 2, |n| n.see_also[0].reason = " \n\t".into()),
            6 => {
                let path = dir
                    .path()
                    .join(format!("nodes/{}/children/{}", id(1), id(3)));
                fs::remove_file(&path).unwrap();
                std::os::unix::fs::symlink("/etc/passwd", path).unwrap();
            }
            7 => {
                fs::write(node_file(dir.path(), 2), [255, 254]).unwrap();
            }
            8 => {
                fs::write(node_file(dir.path(), 2), "---\nname: {bad JSON}\n---\nBody").unwrap();
            }
            9 => {
                fs::write(
                    dir.path().join(format!("nodes/{}/children.order", id(1))),
                    [255],
                )
                .unwrap();
            }
            10 => {
                let path = node_file(dir.path(), 2);
                let text = fs::read_to_string(&path).unwrap();
                fs::write(
                    path,
                    text.replacen("---\n", &format!("---\nparent: \"{}\"\n", id(1)), 1),
                )
                .unwrap();
            }
            11 => {
                let path = node_file(dir.path(), 2);
                fs::remove_file(&path).unwrap();
                std::os::unix::fs::symlink(node_file(dir.path(), 3), path).unwrap();
            }
            _ => unreachable!(),
        }
        let protected = fs::read(dir.path().join("evidence.txt")).unwrap();
        if let Ok(s) = Store::read(dir.path()) {
            assert!(policy().validate(&s).is_err(), "mode {mode}");
        }
        assert_eq!(
            fs::read(dir.path().join("evidence.txt")).unwrap(),
            protected
        );
        assert!(node_file(dir.path(), 2).symlink_metadata().is_ok());
    }
}

#[test]
fn markdown_unicode_unknown_bytes_native_rename_and_git_restore() {
    let helper = temp();
    let native = temp();
    for root in [helper.path(), native.path()] {
        fixture(root);
    }
    let body = "# Café 中 🦀\r\n\r\nname: \"not metadata\"\r\n---\n[[not-an-edge]]\n```\nsee_also: []\n```\nNo final newline";
    let text = format!(
        "---\r\nx_note: {{ \"nested\": [true, null, \"retain 中\"], \"target\": \"{}\" }} \t\r\nmicro_summary: \"\\u00e9 → 中\"\r\n\t\r\n \tname \t: \t\"Original \\u4e2d 🦀\" \t \r\nid: \"{}\"\r\nsee_also: [ {{ \"reason\" : \"é → 中\\nEvidence\", \"target\" : \"{}\" }} ]\r\n---\r\n{body}",
        id(99),
        id(3),
        id(5),
    );
    for root in [helper.path(), native.path()] {
        fs::write(node_file(root, 3), &text).unwrap();
    }
    let p = policy();
    let before = get(helper.path());
    p.validate(&before).unwrap();
    assert_eq!(before.nodes[&id(3)].content, body);
    assert_eq!(before.nodes[&id(3)].name, "Original 中 🦀");
    assert_eq!(before.nodes[&id(3)].micro_summary, "é → 中");
    assert_eq!(before.nodes[&id(3)].see_also[0].reason, "é → 中\nEvidence");
    assert_eq!(before.nodes[&id(3)].see_also.len(), 1); // Unknown metadata/prose creates no edges.
    assert_eq!(
        fs::read(
            helper
                .path()
                .join(format!("trees/{}/children/{}/node.md", id(1), id(3)))
        )
        .unwrap(),
        text.as_bytes(),
    );
    git(helper.path(), &["init", "-q"]);
    git(helper.path(), &["config", "user.name", "Synthetic Fixture"]);
    git(
        helper.path(),
        &["config", "user.email", "fixture@example.invalid"],
    );
    git(helper.path(), &["add", "."]);
    git(
        helper.path(),
        &["commit", "-qm", "fixture: retain authored Markdown"],
    );
    for op in [
        Operation::Move {
            node: id(3),
            parent: id(2),
        },
        Operation::Reorder {
            node: id(3),
            after: id(4),
        },
    ] {
        let after = apply(&p, &before, &op).unwrap();
        assert_eq!(before.nodes, after.nodes);
        for n in 1..=7 {
            assert_eq!(
                fs::read(node_file(helper.path(), n)).unwrap(),
                fs::read(node_file(native.path(), n)).unwrap()
            );
        }
        git(helper.path(), &["add", "."]);
        git(
            helper.path(),
            &["commit", "-qm", "fixture: checked placement"],
        );
        git(helper.path(), &["revert", "--no-edit", "HEAD"]);
        assert_eq!(before.inventory, get(helper.path()).inventory);
    }
    let new_name = "Renamed é 中 🦀: \"quoted\" \\path\nsecond line\tend";
    let expected_text = text.replacen(
        "\"Original \\u4e2d 🦀\"",
        &serde_json::to_string(new_name).unwrap(),
        1,
    );
    let rename = Operation::Rename {
        node: id(3),
        name: new_name.into(),
    };
    let after = apply(&p, &before, &rename).unwrap();
    // Independent ordinary text edit changes one JSON string, not a reconstructed document.
    fs::write(node_file(native.path(), 3), &expected_text).unwrap();
    validate_draft(&p, &before, &get(native.path()), &rename).unwrap();
    assert_eq!(
        fs::read_to_string(node_file(helper.path(), 3)).unwrap(),
        expected_text
    );
    assert_eq!(after.nodes[&id(3)].content, body);
    assert_eq!(after.nodes[&id(3)].see_also, before.nodes[&id(3)].see_also);
    assert_eq!(after.children, before.children);
    for (path, entry) in &before.inventory {
        if path != &PathBuf::from(format!("nodes/{}/node.md", id(3))) {
            assert_eq!(
                after.inventory.get(path),
                Some(entry),
                "unchanged bytes {path:?}"
            );
        }
    }
    let view = after.view();
    let incoming = view["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == id(7))
        .unwrap();
    assert_eq!(incoming["navigation"][0]["name"], new_name);
    assert_eq!(
        incoming["navigation"][0]["path"],
        node_file(helper.path(), 3)
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
    );
    // Semantically ignored metadata and body bytes are nevertheless protected by draft validation.
    for tampered in [
        expected_text.replace("retain 中", "changed 中"),
        expected_text.replace("No final newline", "Changed body"),
    ] {
        fs::write(node_file(native.path(), 3), &tampered).unwrap();
        let retained = get(native.path());
        p.validate(&retained).unwrap();
        assert!(validate_draft(&p, &before, &retained, &rename).is_err());
        assert_eq!(
            fs::read_to_string(node_file(native.path(), 3)).unwrap(),
            tampered
        );
    }
    git(helper.path(), &["add", "."]);
    git(
        helper.path(),
        &["commit", "-qm", "fixture: checked Unicode rename"],
    );
    git(helper.path(), &["revert", "--no-edit", "HEAD"]);
    assert_eq!(before.inventory, get(helper.path()).inventory);
    assert!(git(helper.path(), &["status", "--porcelain"]).is_empty());
}

#[test]
fn ordinary_markdown_body_and_reference_edits_reconstruct_without_prose_edges() {
    let dir = temp();
    fixture(dir.path());
    let path = node_file(dir.path(), 2);
    let text = fs::read_to_string(&path).unwrap();
    let edited = text
        .replace("Summary 2", "Résumé 中")
        .replace("A follows D's evidence", "New explicit reason é → 中")
        + "\n# Native appended content\nsee_also: [{\"target\":\"not-a-UUID\"}]\n";
    fs::write(&path, &edited).unwrap();
    let store = get(dir.path());
    policy().validate(&store).unwrap();
    assert_eq!(store.nodes[&id(2)].micro_summary, "Résumé 中");
    assert_eq!(store.nodes[&id(2)].see_also[0].target, id(5));
    assert_eq!(
        store.nodes[&id(2)].see_also[0].reason,
        "New explicit reason é → 中"
    );
    assert_eq!(store.nodes[&id(2)].see_also.len(), 1);
    let renamed = apply(
        &policy(),
        &store,
        &Operation::Rename {
            node: id(2),
            name: "Native edited node".into(),
        },
    )
    .unwrap();
    assert_eq!(renamed.nodes[&id(2)].content, store.nodes[&id(2)].content);
    assert_eq!(renamed.nodes[&id(2)].see_also, store.nodes[&id(2)].see_also);
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        edited.replace("\"Node 2\"", "\"Native edited node\"")
    );
}

#[test]
fn malformed_markdown_metadata_is_rejected_without_changing_any_bytes() {
    let reference = format!("[{{\"target\":\"{}\",\"reason\":\"Evidence\"}}]", id(5));
    let valid = format!(
        "---\nid: \"{}\"\nname: \"Café\"\nmicro_summary: \"Summary\"\nsee_also: {reference}\n---\nBody\n",
        id(2)
    );
    let malformed = [
        valid.replacen("---\n", "", 1),
        valid.replace("\n---\nBody", "\nBody"),
        valid.replace("\n---\nBody\n", "\n---"),
        valid.replace("name: \"Café\"", "name: Café"),
        valid.replace("name: \"Café\"", "name: 42"),
        valid.replace("name: \"Café\"", "name: \" \t\""),
        valid.replace("name: \"Café\"", "name: \"   \""),
        valid.replace("name: \"Café\"", "name: \"Café\"\nname: \"Other\""),
        valid.replace("name: \"Café\"\n", ""),
        valid.replace("micro_summary: \"Summary\"", "micro_summary: []"),
        valid.replace("micro_summary: \"Summary\"", "micro_summary: \" \""),
        valid.replace(&format!("id: \"{}\"", id(2)), "id: \"short-id\""),
        valid.replace(&format!("id: \"{}\"", id(2)), "id: null"),
        valid.replace(&reference, "null"),
        valid.replace(&reference, "[{}]"),
        valid.replace(
            "\"reason\":\"Evidence\"",
            "\"reason\":\"Evidence\",\"reason\":\"Other\"",
        ),
        valid.replace("\"reason\":\"Evidence\"", "\"reason\":\" \""),
        valid.replace("\"reason\":\"Evidence\"", "\"reason\":7"),
        valid.replace("\"reason\":\"Evidence\"", "\"extra\":\"Not allowed\""),
        valid.replace("see_also:", "bad key:"),
        valid.replacen("---\n", "---\nx_note: {}\nx_note: []\n", 1),
        valid.replacen("---\n", "---\nx_note: {broken}\n", 1),
        valid.replacen("---\n", "---\nparents: []\n", 1),
        valid.replacen("---\n", "---\nchildren: []\n", 1),
    ];
    for (case, text) in malformed.iter().enumerate() {
        let dir = temp();
        fixture(dir.path());
        let path = node_file(dir.path(), 2);
        fs::write(&path, text).unwrap();
        let evidence = fs::read(dir.path().join("evidence.txt")).unwrap();
        assert!(Store::read(dir.path()).is_err(), "metadata case {case}");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            *text,
            "retained case {case}"
        );
        assert_eq!(fs::read(dir.path().join("evidence.txt")).unwrap(), evidence);
    }
}

fn fake(body: &str) -> (TempDir, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp();
    fs::create_dir(dir.path().join("build")).unwrap();
    let exe = dir.path().join("build/graph-policy");
    fs::write(&exe, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
    let mut stamp = String::new();
    for file in ["Cargo.toml", "Cargo.lock", "README.md", ".gitignore"] {
        fs::write(dir.path().join(file), "test-only fake\n").unwrap();
        stamp.push_str(&format!("{}  {file}\n", sha256(b"test-only fake\n")));
    }
    fs::write(dir.path().join("build/candidate.sha256"), stamp).unwrap();
    fs::write(
        dir.path().join("build/graph-policy.sha256"),
        format!("{}  build/graph-policy\n", sha256(&fs::read(&exe).unwrap())),
    )
    .unwrap();
    (dir, exe)
}

#[test]
fn untrusted_process_failures_cannot_accept_or_mutate() {
    let store = temp();
    fixture(store.path());
    let before = get(store.path());
    for body in [
        "exit 9",
        "printf 'STG1 VALID\\nextra\\n'",
        "printf '\\377'",
        "printf 'STG1 VALID\\n' >&2",
        "printf 'garbage\\n'",
        "printf 'STG1 OK 1 0 0\\n'",
        "head -c 1048577 /dev/zero",
    ] {
        let (dir, _) = fake(body);
        let p = Policy::checked(dir.path()).unwrap();
        assert!(p.validate(&before).is_err(), "fake {body}");
        assert!(
            apply(
                &p,
                &before,
                &Operation::Move {
                    node: id(3),
                    parent: id(2)
                }
            )
            .is_err()
        );
        assert_eq!(before.inventory, get(store.path()).inventory);
    }
    assert!(invoke(Path::new("/does/not/exist"), b"input").is_err());
    let (dir, exe) = fake("printf 'STG1 VALID\\n'");
    let p = Policy::checked(dir.path()).unwrap();
    fs::write(&exe, "changed").unwrap();
    assert!(p.validate(&before).is_err());
    assert!(Policy::checked(dir.path()).is_err());
    let (dir, _) = fake("printf 'STG1 VALID\\n'");
    let p = Policy::checked(dir.path()).unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/added.rs"), "new candidate content").unwrap();
    assert!(p.validate(&before).is_err());
    assert_eq!(before.inventory, get(store.path()).inventory);
}

#[test]
fn changed_candidate_after_review_is_not_applied() {
    let dir = temp();
    fixture(dir.path());
    let before = get(dir.path());
    fs::write(dir.path().join("evidence.txt"), "new native edit\n").unwrap();
    assert!(
        apply(
            &policy(),
            &before,
            &Operation::Move {
                node: id(3),
                parent: id(2)
            }
        )
        .is_err()
    );
    assert_eq!(get(dir.path()).children, before.children);
    assert_eq!(
        fs::read(dir.path().join("evidence.txt")).unwrap(),
        b"new native edit\n"
    );
}

#[test]
fn malformed_numeric_runtime_input_fails_closed() {
    let exe = Path::new(env!("CARGO_MANIFEST_DIR")).join("build/graph-policy");
    for wire in [
        b"nonsense".as_slice(),
        b"1 0 0 0 1 0 0 9",
        b"1 0 0 0 1 0",
        b"\xff",
    ] {
        assert!(invoke(&exe, wire).is_err(), "bad wire {wire:?}");
    }
    assert_eq!(invoke(&exe, b"1 0 0 0 1 0 0").unwrap(), "STG1 VALID\n");
}
