use super::*;
use std::{io::Write, process::Stdio};

#[test]
fn native_shared_parent_submission_recomputes_merged_dependency() {
    let bin_dir = crate::native_bin_dir();
    let mut receipts = Vec::new();
    let mut call = |binary: &str, q: serde_json::Value| -> serde_json::Value {
        let mut child = Command::new(bin_dir.join(binary))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("build checked native binaries before testing");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(q.to_string().as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let r: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        receipts.push(json!({"binary":binary,"request":q,"result":r}));
        r
    };
    // Four baseline nodes match the legacy fixture; the unrelated root must survive.
    let f = Fixture::with_nodes(4);
    for (p, n, order) in [(&f.a, 5, vec![5, 2, 3]), (&f.b, 6, vec![2, 3, 6])] {
        add(p, n);
        std::os::unix::fs::symlink(
            format!("../../{}", id(n)),
            p.join(format!("nodes/{}/children/{}", id(1), id(n))),
        )
        .unwrap();
        fs::write(
            p.join(format!("nodes/{}/children.order", id(1))),
            order.into_iter().map(|n| id(n) + "\n").collect::<String>(),
        )
        .unwrap();
        commit(p);
        assert_eq!(
            call(
                "core-observe",
                json!({"op":"index","store":p,"mode":"full"})
            )["status"],
            "current"
        );
    }
    let x = git(&f.a, &["rev-parse", "HEAD"]);
    let y = git(&f.b, &["rev-parse", "HEAD"]);
    let draft_bytes = fs::read(f.a.join(".sifttext/index.sqlite")).unwrap();
    let accepted = call("git-submit-observe", f.request(&f.b));
    assert_eq!(accepted["status"], "current");
    let donor_db = Path::new(accepted["view"].as_str().unwrap()).join(".sifttext/index.sqlite");
    let donor_bytes = fs::read(&donor_db).unwrap();
    let root_navigation = donor_db
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(format!("nodes/{}/outline.full.lean", id(1)));
    let donor_navigation = fs::read(&root_navigation).unwrap();
    let donor_cost: serde_json::Value = serde_json::from_slice(
        &fs::read(root_navigation.with_file_name("outline.full.lean.meta.json")).unwrap(),
    )
    .unwrap();
    let combined = call("git-submit-observe", f.request(&f.a));
    assert_eq!(combined["status"], "current");
    assert_eq!(combined["accepted_index_available"], true);
    assert_eq!(combined["draft_index_available"], true);
    for (key, count) in [
        ("accepted_main", 4),
        ("prepared_worktree", 1),
        ("computed", 1),
        ("changed_dependencies", 1),
    ] {
        assert_eq!(combined["index_reuse"][key], count);
    }
    let view = Path::new(combined["view"].as_str().unwrap());
    assert_eq!(
        git(view, &["rev-parse", "HEAD"]),
        combined["candidate"].as_str().unwrap()
    );
    assert_eq!(
        git(&f.repo, &["rev-parse", "main"]),
        combined["candidate"].as_str().unwrap()
    );
    let indexed = call(
        "core-observe",
        json!({"op":"index","store":view,"mode":"incremental"}),
    );
    assert_eq!(indexed["status"], "current");
    assert_eq!(indexed["version"], combined["version"]);
    let nodes = indexed["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 6);
    assert_eq!(
        nodes.iter().find(|r| r["id"] == id(1)).unwrap()["children"],
        json!([id(5), id(2), id(3), id(6)])
    );
    for n in 1..=6 {
        assert_eq!(
            nodes.iter().find(|r| r["id"] == id(n)).unwrap()["name"],
            format!("Node {n}")
        );
    }
    assert_eq!(
        fs::read_to_string(view.join(format!("nodes/{}/children.order", id(1)))).unwrap(),
        [5, 2, 3, 6]
            .into_iter()
            .map(|n| id(n) + "\n")
            .collect::<String>()
    );
    let outlines = call("core-observe", json!({"op":"outline_inspect","store":view}));
    assert_eq!(outlines["status"], "current");
    assert_eq!(outlines["version"], combined["version"]);
    let combined_navigation = view.join(format!("nodes/{}/outline.full.lean", id(1)));
    assert_ne!(fs::read(&combined_navigation).unwrap(), donor_navigation);
    let combined_cost: serde_json::Value = serde_json::from_slice(
        &fs::read(combined_navigation.with_file_name("outline.full.lean.meta.json")).unwrap(),
    )
    .unwrap();
    assert!(
        combined_cost["tokens"].as_u64().unwrap() > donor_cost["tokens"].as_u64().unwrap(),
        "new shared-parent row must change the full navigation cost"
    );
    assert_eq!(fs::read(&root_navigation).unwrap(), donor_navigation);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &fs::read(root_navigation.with_file_name("outline.full.lean.meta.json")).unwrap()
        )
        .unwrap(),
        donor_cost
    );
    assert_eq!(fs::read(&donor_db).unwrap(), donor_bytes);
    assert_eq!(
        fs::read(f.a.join(".sifttext/index.sqlite")).unwrap(),
        draft_bytes
    );
    assert_eq!(git(&f.a, &["rev-parse", "HEAD"]), x);
    assert_eq!(git(&f.b, &["rev-parse", "HEAD"]), y);
    assert_eq!(git(&f.a, &["status", "--porcelain"]), "");
    assert_eq!(git(&f.b, &["status", "--porcelain"]), "");
    if let Some(path) = std::env::var_os("PREPARED_SHARED_EVIDENCE") {
        fs::write(path, serde_json::to_vec_pretty(&receipts).unwrap()).unwrap();
    }
}
