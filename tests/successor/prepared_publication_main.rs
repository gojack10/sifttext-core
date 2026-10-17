use super::*;
use std::{io::Write, process::Stdio};

pub(super) fn native(binary: &str, request: serde_json::Value) -> serde_json::Value {
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
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn native_main_donor_misses_reuse_only_verified_draft_records() {
    let mut receipts = Vec::new();
    for mutation in ["missing", "corrupt", "record"] {
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
                native(
                    "core-observe",
                    json!({"op":"index","store":p,"mode":"full"})
                )["status"],
                "current"
            );
        }
        let x = git(&f.a, &["rev-parse", "HEAD"]);
        let y = git(&f.b, &["rev-parse", "HEAD"]);
        let draft_db = f.a.join(".sifttext/index.sqlite");
        let draft_bytes = fs::read(&draft_db).unwrap();
        let b = native("git-submit-observe", f.request(&f.b));
        let donor = PathBuf::from(b["view"].as_str().unwrap()).join(".sifttext/index.sqlite");
        match mutation {
            "missing" => fs::remove_file(&donor).unwrap(),
            "corrupt" => fs::write(&donor, "corrupt").unwrap(),
            "record" | "processing" => {
                let db = rusqlite::Connection::open(&donor).unwrap();
                db.execute(
                    if mutation == "record" {
                        "UPDATE nodes SET record='{}'"
                    } else {
                        "UPDATE metadata SET value='incompatible' WHERE key='processing'"
                    },
                    [],
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let donor_after = fs::read(&donor).ok();
        let a = native("git-submit-observe", f.request(&f.a));
        assert_eq!(a["status"], "current", "{mutation}: {a:#}");
        assert_eq!(a["accepted_index_available"], false, "{mutation}");
        assert_eq!(a["draft_index_available"], true, "{mutation}");
        for (key, count) in [
            ("accepted_main", 0),
            ("prepared_worktree", 3),
            ("computed", 1),
        ] {
            assert_eq!(a["index_reuse"][key], count, "{mutation}: {a:#}");
        }
        let view = PathBuf::from(a["view"].as_str().unwrap());
        assert_eq!(
            git(&f.repo, &["rev-parse", "main"]),
            a["candidate"].as_str().unwrap()
        );
        assert_eq!(
            git(&view, &["rev-parse", "HEAD"]),
            a["candidate"].as_str().unwrap()
        );
        let indexed = native(
            "core-observe",
            json!({"op":"index","store":view,"mode":"incremental"}),
        );
        let outline = native("core-observe", json!({"op":"outline_inspect","store":view}));
        for response in [&indexed, &outline] {
            assert_eq!(response["status"], "current", "{mutation}: {response:#}");
            assert_eq!(response["version"], a["version"]);
        }
        let rows = indexed["nodes"].as_array().unwrap();
        assert_eq!(rows.len(), 4);
        for (n, name) in [
            (1, "Node 1"),
            (2, "Prepared X"),
            (3, "Published Y"),
            (4, "Node 4"),
        ] {
            let row = rows.iter().find(|r| r["id"] == id(n)).unwrap();
            assert_eq!(row["name"], name);
            assert_eq!(row["content"], "Body");
        }
        assert_eq!(
            rows.iter().find(|r| r["id"] == id(1)).unwrap()["children"],
            json!([id(2), id(3)])
        );
        let full = native(
            "core-observe",
            json!({"op":"index","store":view,"mode":"full"}),
        );
        assert_eq!(full["status"], "current");
        assert_eq!(full["version"], a["version"]);
        assert_eq!(full["nodes"], indexed["nodes"]);
        assert_eq!(fs::read(&draft_db).unwrap(), draft_bytes);
        assert_eq!(fs::read(&donor).ok(), donor_after);
        assert_eq!(git(&f.a, &["rev-parse", "HEAD"]), x);
        assert_eq!(git(&f.b, &["rev-parse", "HEAD"]), y);
        assert_eq!(git(&f.a, &["status", "--porcelain"]), "");
        assert_eq!(git(&f.b, &["status", "--porcelain"]), "");
        receipts.push(json!({"mutation":mutation,"submission":a,"index":indexed,"outline":outline,"full":full}));
    }
    if let Some(path) = std::env::var_os("PREPARED_MAIN_EVIDENCE") {
        fs::write(path, serde_json::to_vec_pretty(&receipts).unwrap()).unwrap();
    }
}
