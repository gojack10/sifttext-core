use super::*;
use std::{io::Write, process::Stdio};

pub(super) fn call(binary: &str, request: serde_json::Value) -> serde_json::Value {
    let bin = crate::native_bin_dir();
    let mut child = Command::new(bin.join(binary))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("build checked native binaries before testing");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(request.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn native_sparse_donor_uses_original_base_and_refuses_generated_residue() {
    for residual in [true, false] {
        let f = Fixture::with_nodes(4);
        for (p, n, name) in [(&f.a, 2, "Prepared X"), (&f.b, 3, "Published Y")] {
            let file = p.join(format!("nodes/{}/node.md", id(n)));
            fs::write(
                &file,
                fs::read_to_string(&file)
                    .unwrap()
                    .replace(&format!("Node {n}"), name),
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
        let a_tip = git(&f.a, &["rev-parse", "HEAD"]);
        let b_tip = git(&f.b, &["rev-parse", "HEAD"]);
        let b = call("git-submit-observe", f.request(&f.b));
        assert_eq!(b["status"], "current");
        let accepted_db = Path::new(b["view"].as_str().unwrap()).join(".sifttext/index.sqlite");
        let accepted_bytes = fs::read(&accepted_db).unwrap();
        let draft_db = f.a.join(".sifttext/index.sqlite");
        let draft_bytes = fs::read(&draft_db).unwrap();
        git(
            &f.a,
            &[
                "sparse-checkout",
                "set",
                "--no-cone",
                "/.gitignore",
                "/trees/",
                &format!("/nodes/{}/", id(2)),
            ],
        );
        assert!(!f.a.join(format!("nodes/{}/node.md", id(3))).exists());
        // Ignored generated-only directories are deliberately different from absent nodes.
        for n in [1, 3, 4] {
            let directory = f.a.join("nodes").join(id(n));
            assert!(directory.exists(), "fixture must retain generated residue");
            if !residual {
                fs::remove_dir_all(directory).unwrap();
            }
        }
        let sparse_before = unsafe_inputs::fingerprint(&f.a);
        let donor_before = unsafe_inputs::fingerprint(&f.b);
        let a = call("git-submit-observe", f.request(&f.a));
        assert_eq!(a["status"], "current");
        assert_eq!(unsafe_inputs::fingerprint(&f.a), sparse_before);
        assert_eq!(unsafe_inputs::fingerprint(&f.b), donor_before);
        assert!(!f.a.join(format!("nodes/{}/node.md", id(3))).exists());
        assert_eq!(a["accepted_index_available"], true);
        assert_eq!(a["draft_index_available"], !residual);
        for (key, count) in [
            ("accepted_main", 3),
            ("prepared_worktree", if residual { 0 } else { 1 }),
            ("computed", if residual { 1 } else { 0 }),
        ] {
            assert_eq!(a["index_reuse"][key], count, "residual={residual}: {a}");
        }
        let view = Path::new(a["view"].as_str().unwrap());
        assert_eq!(
            git(view, &["rev-parse", "HEAD"]),
            a["candidate"].as_str().unwrap()
        );
        assert_eq!(
            git(view, &["rev-parse", "HEAD^{tree}"]),
            a["tree"].as_str().unwrap()
        );
        assert_eq!(
            git(&f.repo, &["rev-parse", "main"]),
            a["candidate"].as_str().unwrap()
        );
        let indexed = call(
            "core-observe",
            json!({"op":"index","store":view,"mode":"incremental"}),
        );
        assert_eq!(indexed["status"], "current");
        assert_eq!(indexed["version"], a["version"]);
        let rows = indexed["nodes"].as_array().unwrap();
        assert_eq!(rows.len(), 4);
        for (n, name) in [
            (1, "Node 1"),
            (2, "Prepared X"),
            (3, "Published Y"),
            (4, "Node 4"),
        ] {
            assert_eq!(
                rows.iter().find(|r| r["id"] == id(n)).unwrap()["name"],
                name
            );
        }
        assert_eq!(
            rows.iter().find(|r| r["id"] == id(1)).unwrap()["children"],
            json!([id(2), id(3)])
        );
        let outline = call("core-observe", json!({"op":"outline_inspect","store":view}));
        assert_eq!(outline["status"], "current");
        assert_eq!(outline["version"], a["version"]);
        let full = call(
            "core-observe",
            json!({"op":"index","store":view,"mode":"full"}),
        );
        assert_eq!(full["status"], "current");
        assert_eq!(full["version"], a["version"]);
        assert_eq!(full["nodes"], indexed["nodes"]);
        assert_eq!(fs::read(accepted_db).unwrap(), accepted_bytes);
        assert_eq!(fs::read(draft_db).unwrap(), draft_bytes);
        assert_eq!(git(&f.a, &["rev-parse", "HEAD"]), a_tip);
        assert_eq!(git(&f.b, &["rev-parse", "HEAD"]), b_tip);
        assert_eq!(git(&f.a, &["status", "--porcelain"]), "");
        assert_eq!(git(&f.b, &["status", "--porcelain"]), "");
        assert_eq!(unsafe_inputs::fingerprint(&f.a), sparse_before);
        assert_eq!(unsafe_inputs::fingerprint(&f.b), donor_before);
    }
}
