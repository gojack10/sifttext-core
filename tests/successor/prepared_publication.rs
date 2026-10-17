#[path = "prepared_publication_tokens.rs"]
mod tokens;
#[path = "prepared_publication_precas.rs"]
mod precas;
#[path = "unsafe_submission.rs"]
mod unsafe_inputs;
#[path = "submission_parity.rs"]
mod submission_parity;
#[path = "prepared_publication_draft.rs"]
mod draft_fallback;
#[path = "prepared_publication_main.rs"]
mod main_donors;
#[path = "prepared_publication_resources.rs"]
mod resources;
#[path = "prepared_publication_shared.rs"]
mod shared_parent;
#[path = "prepared_publication_outlines.rs"]
mod root_outlines;

use crate::{Policy, checked_publication as publication, index};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn git(p: &Path, args: &[&str]) -> String {
    let o = Command::new("git")
        .arg("-C")
        .arg(p)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().into()
}
fn id(n: usize) -> String {
    format!("81000000-0000-4000-8000-{n:012}")
}
fn node(p: &Path, n: usize, body: &str) {
    let fields = json!({"id":id(n),"name":format!("Node {n}"),"micro_summary":"Summary", "status":"in_progress", "see_also":[]});
    fs::write(
        p.join(format!("nodes/{}/node.md", id(n))),
        format!(
            "---\n{}---\n{body}",
            fields
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| format!("{k}: {v}\n"))
                .collect::<String>()
        ),
    )
    .unwrap();
}
fn add(p: &Path, n: usize) {
    let d = p.join("nodes").join(id(n));
    fs::create_dir_all(d.join("children")).unwrap();
    fs::write(d.join("children/.keep"), "").unwrap();
    fs::write(d.join("children.order"), "").unwrap();
    node(p, n, "Body");
}
fn commit(p: &Path) -> String {
    git(p, &["add", "nodes", "trees"]);
    git(p, &["commit", "-qm", "edit"]);
    git(p, &["rev-parse", "HEAD"])
}
#[path = "prepared_publication_postcas.rs"]
mod post_cas;
#[path = "prepared_publication_sparse.rs"]
mod sparse;

struct Fixture {
    _dir: tempfile::TempDir,
    repo: PathBuf,
    a: PathBuf,
    b: PathBuf,
    base: String,
    policy: Policy,
}
impl Fixture {
    fn new() -> Self {
        Self::with_nodes(3)
    }
    fn with_nodes(count: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir_all(repo.join("trees")).unwrap();
        for n in 1..=count {
            add(&repo, n);
        }
        if count == 4 {
            std::os::unix::fs::symlink(
                format!("../nodes/{}", id(4)),
                repo.join("trees").join(id(4)),
            )
            .unwrap();
        }
        std::os::unix::fs::symlink(
            format!("../nodes/{}", id(1)),
            repo.join("trees").join(id(1)),
        )
        .unwrap();
        for n in 2..=3 {
            std::os::unix::fs::symlink(
                format!("../../{}", id(n)),
                repo.join(format!("nodes/{}/children/{}", id(1), id(n))),
            )
            .unwrap();
        }
        fs::write(
            repo.join(format!("nodes/{}/children.order", id(1))),
            format!("{}\n{}\n", id(2), id(3)),
        )
        .unwrap();
        fs::write(repo.join(".gitignore"), ".sifttext/\n*.lean\n*.meta.json\n").unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "base"]);
        let base = git(&repo, &["rev-parse", "HEAD"]);
        git(&repo, &["checkout", "--detach", "-q"]);
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        for (name, p) in [("a", &a), ("b", &b)] {
            git(
                &repo,
                &[
                    "worktree",
                    "add",
                    "-q",
                    "-b",
                    name,
                    p.to_str().unwrap(),
                    &base,
                ],
            );
        }
        Self {
            _dir: dir,
            repo,
            a,
            b,
            base,
            policy: Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap(),
        }
    }
    fn prepare(&self, p: &Path) {
        assert_eq!(
            index::refresh(&self.policy, p, None, index::Mode::Full, None)
                .unwrap()
                .status,
            "current"
        );
    }
    fn request(&self, p: &Path) -> serde_json::Value {
        json!({"repo":self.repo,"accepted_ref":"refs/heads/main","draft":p,"commit":git(p,&["rev-parse","HEAD"]),"base":self.base})
    }
    fn submit(&self, p: &Path) -> publication::Receipt {
        let q = serde_json::from_value(self.request(p)).unwrap();
        let r = publication::submit(&self.policy, &q);
        assert_eq!(r.status, "current", "{r:?}");
        r
    }
}
#[test]
fn actual_submission_reuses_x_and_y_without_pulling_or_mutating_donors() {
    let f = Fixture::new();
    node(&f.a, 2, "X");
    let x = commit(&f.a);
    node(&f.b, 3, "Y");
    commit(&f.b);
    f.prepare(&f.a);
    f.prepare(&f.b);
    let b = f.submit(&f.b);
    let donor = b.view.as_ref().unwrap().join(".sifttext/index.sqlite");
    let before = fs::read(&donor).unwrap();
    let draft = f.a.join(".sifttext/index.sqlite");
    let draft_before = fs::read(&draft).unwrap();
    let a = f.submit(&f.a);
    let reuse = a.index_reuse.as_ref().unwrap();
    assert!(a.accepted_index_available && a.draft_index_available);
    assert_eq!(
        (reuse.accepted_main, reuse.prepared_worktree, reuse.computed),
        (2, 1, 0)
    );
    assert_eq!(reuse.changed_rows, 3); // Fresh detached cache writes all records once.
    assert_eq!(fs::read(donor).unwrap(), before);
    assert_eq!(fs::read(draft).unwrap(), draft_before);
    assert_eq!(git(&f.a, &["rev-parse", "HEAD"]), x);
    let view = index::refresh(
        &f.policy,
        a.view.as_ref().unwrap(),
        None,
        index::Mode::Incremental,
        None,
    )
    .unwrap();
    assert_eq!(
        view.nodes.iter().find(|n| n["id"] == id(2)).unwrap()["content"],
        "X"
    );
    assert_eq!(
        view.nodes.iter().find(|n| n["id"] == id(3)).unwrap()["content"],
        "Y"
    );
    let retry = f.submit(&f.a);
    assert_eq!(retry.candidate, a.candidate);
    assert_eq!(retry.index_reuse.unwrap().accepted_main, 3);
}
// Process-level counterpart of the legacy disjoint SQL/postings and warm-retry controls.
#[test]
fn native_prepared_publication_preserves_sql_postings_and_warm_retry() {
    use std::{io::Write, process::Stdio};
    let bin_dir = crate::native_bin_dir();
    let call = |binary: &str, q: serde_json::Value| -> serde_json::Value {
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
        serde_json::from_slice(&out.stdout).unwrap()
    };
    let sql = |p: &Path, query: &str| -> Vec<Vec<String>> {
        let db = rusqlite::Connection::open_with_flags(
            p.join(".sifttext/index.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let mut stmt = db.prepare(query).unwrap();
        let columns = stmt.column_count();
        stmt.query_map([], |row| (0..columns).map(|n| row.get(n)).collect())
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    let f = Fixture::with_nodes(4);
    for (p, n, name) in [(&f.a, 2, "Prepared X"), (&f.b, 3, "Published Y")] {
        node(
            p,
            n,
            if n == 2 {
                "X authored by A"
            } else {
                "Y authored by B"
            },
        );
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
                json!({"op":"index", "store":p, "mode":"full"})
            )["status"],
            "current"
        );
    }
    let x = git(&f.a, &["rev-parse", "HEAD"]);
    let y = git(&f.b, &["rev-parse", "HEAD"]);
    let a_before = precas::fingerprint(&f.a);
    let b_before = precas::fingerprint(&f.b);
    let reader_before = precas::fingerprint(&f.repo);
    let b = call("git-submit-observe", f.request(&f.b));
    let donor = PathBuf::from(b["view"].as_str().unwrap());
    let donor_bytes = fs::read(donor.join(".sifttext/index.sqlite")).unwrap();
    let draft_bytes = fs::read(f.a.join(".sifttext/index.sqlite")).unwrap();
    let records = format!(
        "SELECT id,record FROM nodes WHERE id<>'{}' ORDER BY id",
        id(2)
    );
    let postings = format!(
        "SELECT CAST(f.rowid AS TEXT),f.title,f.body,f.names,f.tags FROM search_fts f JOIN search_ids s ON s.rowid=f.rowid WHERE s.id<>'{}' ORDER BY f.rowid",
        id(2)
    );
    let unchanged_records = sql(&donor, &records);
    let unchanged_postings = sql(&donor, &postings);
    let a = call("git-submit-observe", f.request(&f.a));
    assert_eq!(a["status"], "current");
    assert_eq!(a["accepted_index_available"], true);
    assert_eq!(a["draft_index_available"], true);
    for (key, count) in [
        ("accepted_main", 3),
        ("prepared_worktree", 1),
        ("computed", 0),
        ("changed_rows", 4),
        ("search_bodies_reused", 4),
    ] {
        assert_eq!(a["index_reuse"][key], count);
    }
    let view = PathBuf::from(a["view"].as_str().unwrap());
    assert_eq!(sql(&view, &records), unchanged_records);
    assert_eq!(sql(&view, &postings), unchanged_postings);
    let verify = |r: &serde_json::Value| {
        assert_eq!(
            git(
                Path::new(r["view"].as_str().unwrap()),
                &["rev-parse", "HEAD"]
            ),
            r["candidate"].as_str().unwrap()
        );
        let view = Path::new(r["view"].as_str().unwrap());
        assert_eq!(
            git(view, &["rev-parse", "HEAD^{tree}"]),
            r["tree"].as_str().unwrap()
        );
        for tip in [&x, &y] {
            git(view, &["merge-base", "--is-ancestor", tip, "HEAD"]);
        }
        assert_eq!(
            git(&f.repo, &["rev-parse", "main"]),
            r["candidate"].as_str().unwrap()
        );
        let indexed = call(
            "core-observe",
            json!({"op":"index","store":r["view"],"mode":"incremental"}),
        );
        assert_eq!(indexed["status"], "current");
        assert_eq!(indexed["version"], r["version"]);
        let outline = call(
            "core-observe",
            json!({"op":"outline_inspect","store":r["view"]}),
        );
        assert_eq!(outline["version"], r["version"]);
        let rows = indexed["nodes"].as_array().unwrap();
        assert_eq!(
            rows.iter()
                .map(|r| r["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec![id(1), id(2), id(3), id(4)]
        );
        for (n, name, body) in [
            (2, "Prepared X", "X authored by A"),
            (3, "Published Y", "Y authored by B"),
        ] {
            let row = rows.iter().find(|r| r["id"] == id(n)).unwrap();
            assert_eq!(row["name"], name);
            assert_eq!(row["content"], body);
        }
        root_outlines::disjoint(view);
        let allowed = call(
            "core-observe",
            json!({"op":"search","store":view,"query":"Prepared","under_node_id":id(1),"limit":10,"offset":0,"readable_ids":[id(1),id(2)]}),
        );
        assert_eq!(allowed["status"], "current");
        assert!(
            allowed["results"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["id"] == id(2))
        );
        let hidden = call(
            "core-observe",
            json!({"op":"search","store":view,"query":"Published","under_node_id":id(1),"limit":10,"offset":0,"readable_ids":[id(1),id(2)]}),
        );
        assert_eq!(hidden["status"], "current");
        assert_eq!(hidden["total_matches"], 0);
    };
    verify(&a);
    for _ in 0..2 {
        let warm = call("git-submit-observe", f.request(&f.a));
        assert_eq!(warm["candidate"], a["candidate"]);
        assert_eq!(warm["index_reuse"]["accepted_main"], 4);
        assert_eq!(warm["index_reuse"]["computed"], 0);
        verify(&warm);
    }
    fs::remove_dir_all(view.join(".sifttext")).unwrap();
    let retry = call("git-submit-observe", f.request(&f.a));
    assert_eq!(retry["candidate"], a["candidate"]);
    verify(&retry);
    let recovered = call(
        "git-submit-observe",
        json!({"repo":f.repo,"accepted_ref":"refs/heads/main","draft":null,"commit":null,"base":null}),
    );
    assert_eq!(recovered["candidate"], a["candidate"]);
    verify(&recovered);
    assert_eq!(precas::fingerprint(&f.a), a_before);
    assert_eq!(precas::fingerprint(&f.b), b_before);
    assert_eq!(precas::fingerprint(&f.repo), reader_before);
    let search = call(
        "core-observe",
        json!({"op":"search","store":view,"query":"Prepared","under_node_id":id(1),"limit":10,"offset":0,"readable_ids":[id(1),id(3)]}),
    );
    assert_eq!(search["status"], "current");
    assert_eq!(search["total_matches"], 0);
    assert_eq!(
        fs::read(donor.join(".sifttext/index.sqlite")).unwrap(),
        donor_bytes
    );
    assert_eq!(
        fs::read(f.a.join(".sifttext/index.sqlite")).unwrap(),
        draft_bytes
    );
    assert_eq!(git(&f.a, &["rev-parse", "HEAD"]), x);
    assert_eq!(git(&f.b, &["rev-parse", "HEAD"]), y);
    assert_eq!(git(&f.a, &["status", "--porcelain"]), "");
    assert_eq!(git(&f.b, &["status", "--porcelain"]), "");
}

// This is a real process boundary, unlike the library reuse tests above.
// Build checked assets/binaries first; an absent observer is a failure, not a skip.
#[test]
fn prepared_submission_cache_race_write_once_and_interruption() {
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
    let bin = crate::native_bin("git-submit-observe");
    let temp = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("PREPARED_PUBLICATION_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("receipts"));
    assert!(evidence.is_absolute());
    fs::create_dir(&evidence).unwrap();
    thread::scope(|scope| {
        let evidence = &evidence;
        let bin = &bin;
        let workers = ["cache", "draft", "kill"].map(|action| scope.spawn(move || {
        let f = Fixture::new();
        node(&f.a, 2, "Prepared X");
        let x = commit(&f.a);
        node(&f.b, 3, "Published Y");
        let y = commit(&f.b);
        f.prepare(&f.a);
        f.prepare(&f.b);
        let accepted = f.submit(&f.b);
        let donor = accepted
            .view
            .as_ref()
            .unwrap()
            .join(".sifttext/index.sqlite");
        let donor_before = fs::read(&donor).unwrap();
        let draft_db = f.a.join(".sifttext/index.sqlite");
        let draft_before = fs::read(&draft_db).unwrap();
        let tip = git(&f.repo, &["rev-parse", "main"]);
        let a_before = precas::fingerprint(&f.a);
        let b_before = precas::fingerprint(&f.b);
        let ready = evidence.join(format!("{action}.ready"));
        let release = evidence.join(format!("{action}.release"));
        let mut q = f.request(&f.a);
        q["pause_before_publish"] = json!({"ready":ready,"release":release});
        let mut running = Running(Some(
            Command::new(bin)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("build git-submit-observe before native checks"),
        ));
        running
            .0
            .as_mut()
            .unwrap()
            .stdin
            .take()
            .unwrap()
            .write_all(q.to_string().as_bytes())
            .unwrap();
        let start = Instant::now();
        while !ready.exists() {
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "missing {action} barrier"
            );
            assert!(
                running.0.as_mut().unwrap().try_wait().unwrap().is_none(),
                "early {action} exit"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let candidate_view = PathBuf::from(fs::read_to_string(&ready).unwrap());
        // The new detached cache has already written all three rows and postings once.
        let candidate_db = candidate_view.join(".sifttext/index.sqlite");
        let cache_before = fs::read(&candidate_db).unwrap();
        assert_eq!(git(&f.repo, &["rev-parse", "main"]), tip);
        match action {
            "cache" => fs::write(&donor, b"corrupted after acquisition").unwrap(),
            "draft" => node(&f.a, 2, "Raced dirty source"),
            "kill" => running.0.as_mut().unwrap().kill().unwrap(),
            _ => unreachable!(),
        }
        if action != "kill" {
            fs::write(&release, "go").unwrap();
        }
        let output = running.0.take().unwrap().wait_with_output().unwrap();
        let receipt = json!({"request":q,"exit":output.status.code(),"signal":output.status.signal(),
            "stdout":String::from_utf8_lossy(&output.stdout),"stderr":String::from_utf8_lossy(&output.stderr)});
        fs::write(
            evidence.join(format!("{action}.json")),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
        if action == "cache" {
            assert!(output.status.success(), "{receipt:#}");
            let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(result["status"], "current");
            for (key, count) in [
                ("accepted_main", 2),
                ("prepared_worktree", 1),
                ("computed", 0),
                ("changed_rows", 3),
                ("search_bodies_reused", 3),
            ] {
                assert_eq!(result["index_reuse"][key], count, "{receipt:#}");
            }
            assert_eq!(fs::read(&donor).unwrap(), b"corrupted after acquisition");
            let view = PathBuf::from(result["view"].as_str().unwrap());
            assert_eq!(view, candidate_view);
            assert_eq!(fs::read(&candidate_db).unwrap(), cache_before);
            let indexed =
                index::refresh(&f.policy, &view, None, index::Mode::Incremental, None).unwrap();
            assert_eq!(indexed.status, "current");
            assert_eq!(indexed.version.as_deref(), result["version"].as_str());
            for (n, body) in [(2, "Prepared X"), (3, "Published Y")] {
                assert_eq!(
                    indexed.nodes.iter().find(|r| r["id"] == id(n)).unwrap()["content"],
                    body
                );
            }
            assert_eq!(fs::read(&candidate_db).unwrap(), cache_before);
        } else {
            assert!(!output.status.success(), "{receipt:#}");
            assert_eq!(git(&f.repo, &["rev-parse", "main"]), tip);
            assert_eq!(fs::read(&donor).unwrap(), donor_before);
            if action == "kill" {
                assert_eq!(output.status.signal(), Some(libc::SIGKILL));
                assert!(output.stdout.is_empty());
                assert_eq!(precas::fingerprint(&f.a), a_before);
                assert_eq!(precas::fingerprint(&f.b), b_before);
            } else {
                assert!(
                    String::from_utf8_lossy(&output.stdout).contains("source_or_ref_changed"),
                    "{receipt:#}"
                );
                git(&f.a, &["restore", "nodes"]);
            }
            let recovered = sparse::call("git-submit-observe", f.request(&f.a));
            assert_eq!(recovered["status"], "current");
            let view = Path::new(recovered["view"].as_str().unwrap());
            assert_eq!(
                git(&f.repo, &["rev-parse", "main"]),
                recovered["candidate"].as_str().unwrap()
            );
            assert_eq!(
                git(view, &["rev-parse", "HEAD"]),
                recovered["candidate"].as_str().unwrap()
            );
            assert_eq!(
                git(view, &["rev-parse", "HEAD^{tree}"]),
                recovered["tree"].as_str().unwrap()
            );
            assert_eq!(recovered["index_reuse"]["computed"], 0);
            let indexed = sparse::call(
                "core-observe",
                json!({"op":"index","store":view,"mode":"incremental"}),
            );
            assert_eq!(indexed["status"], "current");
            assert_eq!(indexed["version"], recovered["version"]);
            let rows = indexed["nodes"].as_array().unwrap();
            assert_eq!(rows.len(), 3);
            for (n, body) in [(2, "Prepared X"), (3, "Published Y")] {
                assert_eq!(
                    rows.iter().find(|r| r["id"] == id(n)).unwrap()["content"],
                    body
                );
            }
            let outline =
                sparse::call("core-observe", json!({"op":"outline_inspect","store":view}));
            assert_eq!(outline["status"], "current");
            assert_eq!(outline["version"], recovered["version"]);
        }
        // The draft-race control intentionally restores authored bytes, refreshing Git stat data.
        if action != "draft" {
            assert_eq!(precas::fingerprint(&f.a), a_before);
        }
        assert_eq!(precas::fingerprint(&f.b), b_before);
        assert_eq!(fs::read(&draft_db).unwrap(), draft_before);
        assert_eq!(git(&f.a, &["rev-parse", "HEAD"]), x);
        assert_eq!(git(&f.b, &["rev-parse", "HEAD"]), y);
        assert_eq!(git(&f.a, &["status", "--porcelain"]), "");
        assert_eq!(git(&f.b, &["status", "--porcelain"]), "");
        }));
        for worker in workers {
            worker.join().unwrap();
        }
    });
}

#[test]
fn git_clean_shared_parent_inserts_recompute_combined_dependency() {
    let f = Fixture::new();
    for (p, n, order) in [(&f.a, 4, vec![4, 2, 3]), (&f.b, 5, vec![2, 3, 5])] {
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
        f.prepare(p);
    }
    f.submit(&f.b);
    let a = f.submit(&f.a);
    let reuse = a.index_reuse.unwrap();
    assert_eq!(
        (
            reuse.accepted_main,
            reuse.prepared_worktree,
            reuse.computed,
            reuse.changed_dependencies
        ),
        (3, 1, 1, 1)
    );
}
#[test]
fn missing_main_still_reuses_valid_draft_and_forged_draft_has_no_credit() {
    for forged in [false, true] {
        let f = Fixture::new();
        node(&f.a, 2, "X");
        commit(&f.a);
        f.prepare(&f.a);
        if forged {
            rusqlite::Connection::open(f.a.join(".sifttext/index.sqlite"))
                .unwrap()
                .execute("UPDATE nodes SET record='{}'", [])
                .unwrap();
        }
        let a = f.submit(&f.a);
        let reuse = a.index_reuse.unwrap();
        assert!(!a.accepted_index_available);
        assert_eq!(a.draft_index_available, !forged);
        assert_eq!(reuse.accepted_main, 0);
        assert_eq!(reuse.prepared_worktree, if forged { 0 } else { 3 });
        assert_eq!(reuse.computed, if forged { 3 } else { 0 });
    }
}
