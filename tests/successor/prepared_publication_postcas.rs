use super::{sparse::call, *};
use std::{
    io::Write,
    os::unix::process::ExitStatusExt,
    process::{Child, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Running(Option<Child>);
impl Drop for Running {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn native_post_cas_kill_recovers_and_later_publisher_invalidates_old_receipt() {
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug"))
        .join("git-submit-observe");
    for kill in [true, false] {
        let f = Fixture::new();
        node(&f.a, 2, "Published X");
        let x = commit(&f.a);
        node(&f.b, 3, "Later Y");
        let y = commit(&f.b);
        let a_before = super::precas::fingerprint(&f.a);
        let b_before = super::precas::fingerprint(&f.b);
        let draft_bytes = fs::read(f.a.join(format!("nodes/{}/node.md", id(2)))).unwrap();
        let ready = f._dir.path().join("ready");
        let release = f._dir.path().join("release");
        let mut request = f.request(&f.a);
        request["pause_after_publish"] = json!({"ready":ready,"release":release});
        let mut running = Running(Some(
            Command::new(&bin)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("build checked native observer before testing"),
        ));
        running
            .0
            .as_mut()
            .unwrap()
            .stdin
            .take()
            .unwrap()
            .write_all(request.to_string().as_bytes())
            .unwrap();
        let start = Instant::now();
        while !ready.exists() {
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "missing post-CAS barrier"
            );
            assert!(
                running.0.as_mut().unwrap().try_wait().unwrap().is_none(),
                "early observer exit"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let published = git(&f.repo, &["rev-parse", "main"]);
        assert_ne!(published, f.base);
        let view = PathBuf::from(fs::read_to_string(&ready).unwrap());
        assert_eq!(git(&view, &["rev-parse", "HEAD"]), published);
        let later = if kill {
            running.0.as_mut().unwrap().kill().unwrap();
            None
        } else {
            let later = call("git-submit-observe", f.request(&f.b));
            assert_eq!(later["status"], "current");
            assert_ne!(later["candidate"], published);
            fs::write(&release, "go").unwrap();
            Some(later)
        };
        let output = running.0.take().unwrap().wait_with_output().unwrap();
        assert!(!output.status.success());
        assert_eq!(super::precas::fingerprint(&f.a), a_before);
        assert_eq!(super::precas::fingerprint(&f.b), b_before);
        if kill {
            assert_eq!(output.status.signal(), Some(libc::SIGKILL));
            assert!(
                output.stdout.is_empty(),
                "killed publisher must emit no receipt"
            );
            assert_eq!(git(&f.repo, &["rev-parse", "main"]), published);
        } else {
            // Rejections use pinned LEAN scalars, not the success JSON format.
            let old = std::str::from_utf8(&output.stdout).unwrap();
            for expected in ["status:unavailable", "published:T"] {
                assert_eq!(old.lines().filter(|line| *line == expected).count(), 1);
            }
            assert_eq!(
                git(&f.repo, &["rev-parse", "main"]),
                later.as_ref().unwrap()["candidate"].as_str().unwrap()
            );
        }
        let recovered = call("git-submit-observe", f.request(&f.a));
        assert_eq!(recovered["status"], "current");
        let recovered_view = Path::new(recovered["view"].as_str().unwrap());
        assert_eq!(
            git(recovered_view, &["rev-parse", "HEAD"]),
            recovered["candidate"].as_str().unwrap()
        );
        assert_eq!(
            git(recovered_view, &["rev-parse", "HEAD^{tree}"]),
            recovered["tree"].as_str().unwrap()
        );
        assert_eq!(
            git(&f.repo, &["rev-parse", "main"]),
            recovered["candidate"].as_str().unwrap()
        );
        if kill {
            assert_eq!(recovered["candidate"], published);
        } else {
            assert_eq!(recovered["candidate"], later.as_ref().unwrap()["candidate"]);
        }
        let index = call(
            "core-observe",
            json!({"op":"index","store":recovered["view"],"mode":"incremental"}),
        );
        assert_eq!(index["status"], "current");
        assert_eq!(index["version"], recovered["version"]);
        let nodes = index["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 3);
        for (n, body) in [
            (2, "Published X"),
            (3, if kill { "Body" } else { "Later Y" }),
        ] {
            assert_eq!(
                nodes.iter().find(|r| r["id"] == id(n)).unwrap()["content"],
                body
            );
        }
        let outline = call(
            "core-observe",
            json!({"op":"outline_inspect","store":recovered["view"]}),
        );
        assert_eq!(outline["status"], "current");
        assert_eq!(outline["version"], recovered["version"]);
        assert_eq!(super::precas::fingerprint(&f.a), a_before);
        assert_eq!(super::precas::fingerprint(&f.b), b_before);
        assert_eq!(
            fs::read(f.a.join(format!("nodes/{}/node.md", id(2)))).unwrap(),
            draft_bytes
        );
        assert_eq!(git(&f.a, &["rev-parse", "HEAD"]), x);
        assert_eq!(git(&f.b, &["rev-parse", "HEAD"]), y);
        assert_eq!(git(&f.a, &["status", "--porcelain"]), "");
        assert_eq!(git(&f.b, &["status", "--porcelain"]), "");
    }
}

// Same three deterministic specimens as retained c990764; not power-loss proof.
#[test]
fn native_combined_post_cas_derived_damage_recovers_from_accepted_git_alone() {
    use super::precas::fingerprint;
    use std::collections::BTreeMap;
    let bin_dir = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug"));
    let temp = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("DERIVED_RECOVERY_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("receipts"));
    fs::create_dir(&evidence).unwrap();
    let artifacts = |view: &Path| -> BTreeMap<PathBuf, Vec<u8>> {
        (1..=3)
            .flat_map(|n| {
                [
                    "outline.lean",
                    "outline.full.lean",
                    "outline.lean.meta.json",
                    "outline.full.lean.meta.json",
                ]
                .map(|name| PathBuf::from(format!("nodes/{}/{name}", id(n))))
            })
            .map(|rel| {
                let bytes = fs::read(view.join(&rel)).unwrap();
                (rel, bytes)
            })
            .collect()
    };
    let mut cases = Vec::new();
    for damage in ["sqlite", "outline", "sidecar"] {
        let f = Fixture::new();
        node(&f.a, 2, "Recovery X body");
        let x = commit(&f.a);
        node(&f.b, 3, "Recovery Y body");
        let y = commit(&f.b);
        for p in [&f.a, &f.b] {
            assert_eq!(
                call(
                    "core-observe",
                    json!({"op":"index","store":p,"mode":"full"})
                )["status"],
                "current"
            );
        }
        let a_before = fingerprint(&f.a);
        let b_before = fingerprint(&f.b);
        let reader_before = fingerprint(&f.repo);
        let b = call("git-submit-observe", f.request(&f.b));
        let donor = Path::new(b["view"].as_str().unwrap());
        let donor_before = artifacts(donor);
        let donor_db = fs::read(donor.join(".sifttext/index.sqlite")).unwrap();
        let ready = evidence.join(format!("{damage}.ready"));
        let release = evidence.join(format!("{damage}.release"));
        let mut request = f.request(&f.a);
        request["pause_after_publish"] = json!({"ready":ready,"release":release});
        let mut running = Running(Some(
            Command::new(bin_dir.join("git-submit-observe"))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("build checked observer before native checks"),
        ));
        running
            .0
            .as_mut()
            .unwrap()
            .stdin
            .take()
            .unwrap()
            .write_all(request.to_string().as_bytes())
            .unwrap();
        let start = Instant::now();
        while !ready.exists() {
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "missing {damage} post-CAS barrier"
            );
            assert!(
                running.0.as_mut().unwrap().try_wait().unwrap().is_none(),
                "early observer exit"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let view = PathBuf::from(fs::read_to_string(&ready).unwrap());
        let tip = git(&f.repo, &["rev-parse", "main"]);
        let tree = git(&f.repo, &["rev-parse", "main^{tree}"]);
        assert_ne!(tip, b["candidate"].as_str().unwrap());
        for sha in [&x, &y] {
            git(&f.repo, &["merge-base", "--is-ancestor", sha, &tip]);
        }
        let valid_artifacts = artifacts(&view);
        assert_eq!(valid_artifacts.len(), 12);
        let damaged = match damage {
            "sqlite" => view.join(".sifttext/index.sqlite"),
            "outline" => view.join(format!("nodes/{}/outline.full.lean", id(1))),
            "sidecar" => view.join(format!("nodes/{}/outline.full.lean.meta.json", id(1))),
            _ => unreachable!(),
        };
        assert!(damaged.is_file());
        fs::write(
            &damaged,
            if damage == "sqlite" {
                vec![0; 64]
            } else {
                b"torn derived bytes".to_vec()
            },
        )
        .unwrap();
        fs::write(&release, "go").unwrap();
        let out = running.0.take().unwrap().wait_with_output().unwrap();
        assert!(!out.status.success());
        assert_eq!(out.status.signal(), None);
        let text = std::str::from_utf8(&out.stdout).unwrap();
        for line in [
            "status:unavailable",
            "check:published_view_requires_recovery",
            "published:T",
        ] {
            assert_eq!(text.lines().filter(|s| *s == line).count(), 1, "{text}");
        }
        assert!(
            !text.lines().any(|s| s.starts_with("view:")),
            "must not advertise damaged view"
        );
        assert!(!text.contains(f._dir.path().to_str().unwrap()));
        assert!(!text.contains(evidence.to_str().unwrap()));
        assert_eq!(git(&f.repo, &["rev-parse", "main"]), tip);
        assert_eq!(git(&f.repo, &["rev-parse", "main^{tree}"]), tree);
        assert_eq!(fingerprint(&f.a), a_before);
        assert_eq!(fingerprint(&f.b), b_before);
        assert_eq!(fingerprint(&f.repo), reader_before);
        assert_eq!(artifacts(donor), donor_before);
        assert_eq!(
            fs::read(donor.join(".sifttext/index.sqlite")).unwrap(),
            donor_db
        );
        // No draft/commit/base is supplied: reconstruct only accepted authored Git.
        let recovered = call(
            "git-submit-observe",
            json!({"repo":f.repo,"accepted_ref":"refs/heads/main"}),
        );
        assert_eq!(recovered["candidate"], tip);
        assert_eq!(recovered["tree"], tree);
        let recovered_view = Path::new(recovered["view"].as_str().unwrap());
        assert_eq!(git(recovered_view, &["rev-parse", "HEAD"]), tip);
        assert_eq!(git(recovered_view, &["rev-parse", "HEAD^{tree}"]), tree);
        let indexed = call(
            "core-observe",
            json!({"op":"index","store":recovered_view,"mode":"incremental"}),
        );
        assert_eq!(indexed["status"], "current");
        assert_eq!(indexed["version"], recovered["version"]);
        let rows: Vec<_> = indexed["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| {
                (
                    n["id"].as_str().unwrap().to_string(),
                    n["name"].as_str().unwrap().to_string(),
                    n["content"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                (id(1), "Node 1".into(), "Body".into()),
                (id(2), "Node 2".into(), "Recovery X body".into()),
                (id(3), "Node 3".into(), "Recovery Y body".into())
            ]
        );
        let db = rusqlite::Connection::open_with_flags(
            recovered_view.join(".sifttext/index.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let sql_rows: Vec<serde_json::Value> = db
            .prepare("SELECT record FROM nodes ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| serde_json::from_str(&r.unwrap()).unwrap())
            .collect();
        assert_eq!(json!(sql_rows), indexed["nodes"]);
        let inspected = call(
            "core-observe",
            json!({"op":"outline_inspect","store":recovered_view}),
        );
        assert_eq!(inspected["status"], "current");
        assert_eq!(inspected["version"], recovered["version"]);
        assert_eq!(inspected["files"].as_array().unwrap().len(), 12);
        assert_eq!(artifacts(recovered_view), valid_artifacts);
        for n in [2, 3] {
            let search = call(
                "core-observe",
                json!({"op":"search","store":recovered_view,"query":if n == 2 { "\"Recovery X\"" } else { "\"Recovery Y\"" },"under_node_id":id(1),"limit":10,"offset":0,"readable_ids":[id(1),id(2),id(3)]}),
            );
            assert_eq!(search["status"], "current");
            assert_eq!(search["version"], recovered["version"]);
            assert_eq!(
                search["results"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| r["id"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                vec![id(n)]
            );
        }
        assert_eq!(git(&f.repo, &["rev-parse", "main"]), tip);
        assert_eq!(fingerprint(&f.a), a_before);
        assert_eq!(fingerprint(&f.b), b_before);
        assert_eq!(fingerprint(&f.repo), reader_before);
        assert_eq!(artifacts(donor), donor_before);
        assert_eq!(
            fs::read(donor.join(".sifttext/index.sqlite")).unwrap(),
            donor_db
        );
        cases.push(json!({"damage":damage,"request":request,"exit":out.status.code(),"stdout":text,"stderr":String::from_utf8_lossy(&out.stderr),"accepted":tip,"tree":tree,"X":x,"Y":y,"recovered":recovered}));
    }
    fs::write(evidence.join("derived-recovery.json"), serde_json::to_vec_pretty(&json!({"cases":cases,"limits":["three-node deterministic post-CAS corruption, not arbitrary interruption or power-loss proof","no private-store, founder, aggregate performance or full-Core acceptance"]})).unwrap()).unwrap();
}
