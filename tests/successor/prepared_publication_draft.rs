use super::{Fixture, commit, git, id};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

// Exact draft-donor mutation family from prepared_publication.mjs.
#[test]
fn native_draft_donor_misses_keep_complete_checked_publication() {
    let bin_dir = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug"));
    let evidence = std::env::var_os("PREPARED_DRAFT_FALLBACK_EVIDENCE").map(PathBuf::from);
    if let Some(p) = &evidence {
        fs::create_dir(p).unwrap();
    }
    for mutation in [
        "missing",
        "corrupt",
        "record",
        "column",
        "schema",
        "processing",
        "version",
    ] {
        let mut receipts = Vec::new();
        let mut call = |binary: &str, q: Value| -> Value {
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
                "{mutation}: {}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let result: Value = serde_json::from_slice(&out.stdout).unwrap();
            receipts.push(json!({"binary":binary,"request":q,"result":result}));
            result
        };
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
        let x = git(&f.a, &["rev-parse", "HEAD"]);
        let y = git(&f.b, &["rev-parse", "HEAD"]);
        let accepted = call("git-submit-observe", f.request(&f.b));
        assert_eq!(accepted["status"], "current");
        let donor =
            PathBuf::from(accepted["view"].as_str().unwrap()).join(".sifttext/index.sqlite");
        let main_bytes = fs::read(&donor).unwrap();
        let draft = f.a.join(".sifttext/index.sqlite");
        match mutation {
            "missing" => fs::remove_file(&draft).unwrap(),
            "corrupt" => fs::write(&draft, "corrupt").unwrap(),
            _ => {
                let db = rusqlite::Connection::open(&draft).unwrap();
                let query = match mutation {
                    "record" => "UPDATE nodes SET record='{}'".into(),
                    "column" => "UPDATE nodes SET name='forged'".into(),
                    key => format!("UPDATE metadata SET value='incompatible' WHERE key='{key}'"),
                };
                assert!(
                    db.execute(&query, []).unwrap() > 0,
                    "mutation must change donor"
                );
            }
        }
        let draft_bytes = fs::read(&draft).ok();
        let r = call("git-submit-observe", f.request(&f.a));
        assert_eq!(r["status"], "current", "{mutation}");
        assert_eq!(r["accepted_index_available"], true, "{mutation}");
        assert_eq!(r["draft_index_available"], false, "{mutation}");
        for (key, count) in [
            ("accepted_main", 3),
            ("prepared_worktree", 0),
            ("computed", 1),
        ] {
            assert_eq!(r["index_reuse"][key], count, "{mutation}: {key}");
        }
        let view = PathBuf::from(r["view"].as_str().unwrap());
        assert_eq!(
            git(&view, &["rev-parse", "HEAD"]),
            r["candidate"].as_str().unwrap()
        );
        assert_eq!(
            git(&f.repo, &["rev-parse", "main"]),
            r["candidate"].as_str().unwrap()
        );
        let indexed = call(
            "core-observe",
            json!({"op":"index","store":view,"mode":"incremental"}),
        );
        assert_eq!(indexed["status"], "current");
        assert_eq!(indexed["version"], r["version"]);
        let nodes = indexed["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 4);
        for n in 1..=4 {
            let record = nodes.iter().find(|row| row["id"] == id(n)).unwrap();
            let name = match n {
                2 => "Prepared X".into(),
                3 => "Published Y".into(),
                _ => format!("Node {n}"),
            };
            assert_eq!(record["name"], name, "{mutation}: node {n}");
            assert_eq!(record["content"], "Body");
        }
        let outline = call("core-observe", json!({"op":"outline_inspect","store":view}));
        assert_eq!(outline["status"], "current");
        assert_eq!(outline["version"], r["version"]);
        assert_eq!(fs::read(&donor).unwrap(), main_bytes);
        assert_eq!(fs::read(&draft).ok(), draft_bytes);
        assert_eq!(git(&f.a, &["rev-parse", "HEAD"]), x);
        assert_eq!(git(&f.b, &["rev-parse", "HEAD"]), y);
        assert_eq!(git(&f.a, &["status", "--porcelain"]), "");
        assert_eq!(git(&f.b, &["status", "--porcelain"]), "");
        if let Some(p) = &evidence {
            fs::write(
                p.join(format!("{mutation}.json")),
                serde_json::to_vec_pretty(&receipts).unwrap(),
            )
            .unwrap();
        }
    }
}
