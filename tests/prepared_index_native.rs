//! Native prepared-index coverage; complete legacy scenarios remain in ordinary Cargo.
//! Build checked assets first, then cargo test --locked --test prepared_index_native.
//! Resource observations share the independent process-tree observer with outlines.
include!("common/prepared_resources.rs");

use rusqlite::{Connection, OpenFlags, params_from_iter};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn id(n: usize) -> String {
    format!("70000000-0000-4000-8000-{n:012}")
}
fn node(p: &Path, n: usize) -> PathBuf {
    p.join("nodes").join(id(n))
}
fn copy(source: &Path, dest: &Path) {
    let meta = fs::symlink_metadata(source).unwrap();
    if meta.file_type().is_symlink() {
        symlink(fs::read_link(source).unwrap(), dest).unwrap();
    } else if meta.is_dir() {
        fs::create_dir(dest).unwrap();
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            copy(&entry.path(), &dest.join(entry.file_name()));
        }
    } else {
        fs::copy(source, dest).unwrap();
    }
}
fn order(p: &Path, n: usize, children: &[usize]) {
    let text = children.iter().map(|n| id(*n) + "\n").collect::<String>();
    fs::write(node(p, n).join("children.order"), text).unwrap();
}
fn authored(p: &Path, n: usize) {
    let dir = node(p, n);
    fs::create_dir_all(dir.join("children")).unwrap();
    order(p, n, &[]);
    let fields = json!({"id":id(n),"name":format!("Name {}",n.saturating_sub(1)),
        "micro_summary":format!("Summary {} Ω",n.saturating_sub(1)),"status":"in_progress",
        "scope":format!("Exact login reset {}",n.saturating_sub(1)),
        "see_also":if n==2 {json!([{"target":id(6),"reason":"Exact cross-tree reason"}])}else{json!([])}});
    let metadata: String = fields
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| format!("{k}: {v}\n"))
        .collect();
    fs::write(
        dir.join("node.md"),
        format!(
            "---\n{metadata}---\nBody {}. Login reset.\n",
            n.saturating_sub(1)
        ),
    )
    .unwrap();
}
fn field(p: &Path, n: usize, key: &str, value: Value) {
    let path = node(p, n).join("node.md");
    let text = fs::read_to_string(&path).unwrap();
    let prefix = format!("{key}: ");
    assert_eq!(text.lines().filter(|l| l.starts_with(&prefix)).count(), 1);
    fs::write(
        path,
        text.lines()
            .map(|l| {
                if l.starts_with(&prefix) {
                    format!("{prefix}{value}\n")
                } else {
                    format!("{l}\n")
                }
            })
            .collect::<String>(),
    )
    .unwrap();
}
fn fixture(p: &Path) {
    fs::create_dir_all(p.join("trees")).unwrap();
    for n in 1..=7 {
        authored(p, n);
    }
    for (parent, children) in [(1, vec![2, 3]), (2, vec![4]), (5, vec![6, 7])] {
        order(p, parent, &children);
        for child in children {
            symlink(
                format!("../../{}", id(child)),
                node(p, parent).join("children").join(id(child)),
            )
            .unwrap();
        }
    }
    for n in [1, 5] {
        symlink(format!("../nodes/{}", id(n)), p.join("trees").join(id(n))).unwrap();
    }
}
fn db(p: &Path) -> PathBuf {
    p.join(".sifttext/index.sqlite")
}
fn sql(p: &Path, query: &str, args: &[String]) -> Vec<Vec<Value>> {
    let c = Connection::open_with_flags(db(p), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut s = c.prepare(query).unwrap();
    let count = s.column_count();
    s.query_map(params_from_iter(args), |r| {
        Ok((0..count)
            .map(|i| match r.get_ref(i).unwrap() {
                rusqlite::types::ValueRef::Integer(n) => json!(n),
                rusqlite::types::ValueRef::Text(s) => json!(std::str::from_utf8(s).unwrap()),
                other => panic!("unexpected SQL value {other:?}"),
            })
            .collect())
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}
struct Native {
    bin: PathBuf,
    evidence: PathBuf,
    serial: usize,
    main: PathBuf,
    work: PathBuf,
}
impl Native {
    fn clone_store(&mut self, p: &Path, label: &str) -> PathBuf {
        let dest = self.evidence.join(format!("{label}-{}", self.serial));
        self.serial += 1;
        copy(p, &dest);
        dest
    }
    fn call(&mut self, request: Value, status: &str) -> Value {
        let start = std::time::Instant::now();
        let mut child = Command::new(&self.bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&request).unwrap())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        let out: Value = serde_json::from_slice(&output.stdout).unwrap();
        let receipt = json!({"request":request,"ms":start.elapsed().as_secs_f64()*1000.0,
            "exit":output.status.code(),"stdout":out,"stderr":String::from_utf8_lossy(&output.stderr)});
        fs::write(
            self.evidence.join(format!("call-{}.json", self.serial)),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
        self.serial += 1;
        assert_eq!(
            output.status.code(),
            Some(if status == "current" { 0 } else { 1 }),
            "{receipt:#}"
        );
        assert_eq!(
            out.get("view").unwrap_or(&out)["status"],
            status,
            "{receipt:#}"
        );
        out
    }
    fn paused(&mut self, p: &Path, action: impl FnOnce(), kill: bool) {
        let ready = self.evidence.join(format!("ready-{}", self.serial));
        let release = ready.with_extension("release");
        let request = json!({"op":"index_prepared","store":p,"accepted_main":self.main,
            "prepared_worktree":self.work,"pause_before_accept":{"ready":ready,"release":release}});
        let mut child = Command::new(&self.bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(request.to_string().as_bytes())
            .unwrap();
        let start = Instant::now();
        while !ready.exists() {
            if start.elapsed() > Duration::from_secs(30) {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!("prepared acceptance barrier unavailable: {output:?}");
            }
            assert!(
                child.try_wait().unwrap().is_none(),
                "prepared request exited before barrier"
            );
            thread::sleep(Duration::from_millis(5));
        }
        action();
        if kill {
            child.kill().unwrap();
        } else {
            fs::write(&release, "release").unwrap();
        }
        let output = child.wait_with_output().unwrap();
        let signal = std::os::unix::process::ExitStatusExt::signal(&output.status);
        let receipt = json!({"request":request,"exit":output.status.code(),"signal":signal,
            "stdout":String::from_utf8_lossy(&output.stdout),"stderr":String::from_utf8_lossy(&output.stderr)});
        fs::write(
            self.evidence.join(format!("race-{}.json", self.serial)),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
        self.serial += 1;
        if kill {
            assert_eq!(signal, Some(9), "{receipt:#}");
            assert!(output.stdout.is_empty(), "{receipt:#}");
        } else {
            assert_eq!(output.status.code(), Some(1), "{receipt:#}");
            if output.stdout.is_empty() {
                let diagnostic: Value = serde_json::from_slice(&output.stderr).unwrap();
                assert_eq!(
                    diagnostic,
                    json!({"kind":"not_found","status":"error"}),
                    "{receipt:#}"
                );
            } else {
                let result: Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(result["view"]["status"], "stale", "{receipt:#}");
                assert!(result["view"]["version"].is_null(), "{receipt:#}");
            }
        }
    }
    fn prepared_with(&mut self, p: &Path, extra: Value) -> Value {
        let mut request = json!({"op":"index_prepared","store":p,"accepted_main":self.main,"prepared_worktree":self.work});
        request
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.call(request, "current")
    }
    fn prepared(&mut self, p: &Path) -> Value {
        self.prepared_with(p, json!({}))
    }
    fn search_with(&mut self, p: &Path, query: &str, extra: Value) -> Value {
        let mut request = json!({"op":"search","store":p,"query":query,"under_node_id":null,
            "readable_ids":(1..=7).map(id).collect::<Vec<_>>(),"limit":20,"offset":0});
        request
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.call(request, "current")
    }
    fn search(&mut self, p: &Path, query: &str) -> Value {
        self.search_with(p, query, json!({}))
    }
    fn equality(&mut self, p: &Path) {
        let before = self.prepared(p)["view"].clone();
        let full = self.clone_store(p, "full");
        fs::remove_dir_all(full.join(".sifttext")).unwrap();
        let rebuilt = self.call(json!({"op":"index","store":full,"mode":"full"}), "current");
        assert_eq!(before, rebuilt);
        for query in [
            "login reset",
            "\"login reset\"",
            "login -reset",
            "resetting",
            "",
            "-login",
        ] {
            let a = self.search(p, query);
            let b = self.search(&full, query);
            assert_eq!(a["total_matches"], b["total_matches"]);
            let normalize = |v: &Value| {
                let mut rows = v["results"].as_array().unwrap().clone();
                for row in &mut rows {
                    row.as_object_mut().unwrap().remove("rank");
                }
                rows.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
                rows
            };
            assert_eq!(normalize(&a), normalize(&b));
            if query.is_empty() {
                assert_eq!(a, b);
            }
        }
    }
}
#[test]
fn native_settings_scopes_and_sparse_prepared_views() {
    let temp = tempfile::tempdir().unwrap();
    let evidence = if let Some(p) = std::env::var_os("SIFTTEXT_TEST_EVIDENCE") {
        let p = PathBuf::from(format!("{}-settings-sparse", PathBuf::from(p).display()));
        assert!(p.is_absolute());
        fs::create_dir(&p).unwrap();
        p
    } else {
        temp.path().to_path_buf()
    };
    let initial = evidence.join("initial");
    fixture(&initial);
    let main = evidence.join("main");
    copy(&initial, &main);
    let work = evidence.join("work");
    copy(&initial, &work);
    field(&main, 3, "name", json!("Current Y"));
    field(&work, 2, "name", json!("Prepared X"));
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .map(|p| p.join("core-observe"))
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_core-observe")));
    let mut native = Native {
        bin,
        evidence,
        serial: 0,
        main: main.clone(),
        work: work.clone(),
    };
    native.search(&main, "login");
    native.search(&work, "login");
    let combined = native.clone_store(&main, "combined");
    field(&combined, 2, "name", json!("Prepared X"));
    field(&combined, 4, "name", json!("Uncommitted unique"));
    fs::rename(
        node(&combined, 2).join("children").join(id(4)),
        node(&combined, 3).join("children").join(id(4)),
    )
    .unwrap();
    order(&combined, 2, &[]);
    order(&combined, 3, &[4]);
    native.prepared(&combined);
    assert_eq!(
        native.search_with(&combined, "unique", json!({"under_node_id":id(2)}))["total_matches"],
        0
    );
    assert_eq!(
        native.search_with(&combined, "unique", json!({"under_node_id":id(3)}))["results"][0]["id"],
        id(4)
    );
    assert_eq!(
        native.search_with(&combined, "login", json!({"under_node_id":id(2)}))["total_matches"],
        1
    );
    assert_eq!(
        native.search_with(&combined, "login", json!({"under_node_id":id(3)}))["total_matches"],
        2
    );
    let full = native.clone_store(&combined, "full");
    fs::remove_dir_all(full.join(".sifttext")).unwrap();
    native.call(json!({"op":"index","store":full,"mode":"full"}), "current");
    let bytes = fs::read(db(&combined)).unwrap();
    let unrestricted = native.search(&combined, "login");
    for (scope, expected) in [
        (None, 2),
        (Some(1), 2),
        (Some(3), 2),
        (Some(4), 1),
        (Some(5), 0),
    ] {
        let settings = json!({"under_node_id":scope.map(id),"readable_ids":[id(4),id(3)],
            "limit":1,"offset":1,"ranking":{"title_weight":3,"boosts":{id(4):2.5}}});
        let result = native.search_with(&combined, "login", settings.clone());
        assert_eq!(result["total_matches"], expected);
        assert_eq!(
            result["results"].as_array().unwrap().len(),
            if expected == 2 { 1 } else { 0 }
        );
        for hit in result["results"].as_array().unwrap() {
            assert!([id(4), id(3)].contains(&hit["id"].as_str().unwrap().to_owned()));
        }
        assert_eq!(result, native.search_with(&full, "login", settings));
        assert_eq!(fs::read(db(&combined)).unwrap(), bytes);
        assert_eq!(native.search(&combined, "login"), unrestricted);
        assert_eq!(fs::read(db(&combined)).unwrap(), bytes);
    }
    let isolated = native.clone_store(&combined, "isolated");
    field(&isolated, 2, "name", json!("Isolated"));
    assert_ne!(
        native.prepared(&isolated)["view"]["version"],
        native.prepared(&combined)["view"]["version"]
    );
    let complete_view = native.prepared(&combined)["view"].clone();
    let sparse = native.clone_store(&combined, "sparse");
    fs::remove_dir_all(node(&sparse, 4)).unwrap();
    assert_eq!(
        native.prepared_with(&sparse, json!({"base_store":combined}))["view"],
        complete_view
    );
    assert_eq!(
        native.search_with(&sparse, "login", json!({"base_store":combined})),
        unrestricted
    );
    let sparse_work = native.clone_store(&work, "sparse-work");
    fs::remove_dir_all(node(&sparse_work, 4)).unwrap();
    native.call(
        json!({"op":"index","store":sparse_work,"base_store":initial,"mode":"full"}),
        "current",
    );
    let reused = native.prepared_with(
        &combined,
        json!({"prepared_worktree":sparse_work,"prepared_base":initial}),
    );
    assert_eq!(reused["prepared_worktree_available"], true);
    assert_eq!(reused["view"], complete_view);
    assert_eq!(fs::read(db(&combined)).unwrap(), bytes);
}

#[test]
fn native_disjoint_reuse_dependencies_postings_and_donor_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let evidence = if let Some(p) = std::env::var_os("SIFTTEXT_TEST_EVIDENCE") {
        let p = PathBuf::from(p);
        assert!(p.is_absolute());
        fs::create_dir(&p).unwrap();
        p
    } else {
        temp.path().to_path_buf()
    };
    let initial = evidence.join("initial");
    fixture(&initial);
    let main = evidence.join("main");
    copy(&initial, &main);
    let work = evidence.join("work");
    copy(&initial, &work);
    field(&main, 3, "name", json!("Current Y"));
    field(&work, 2, "name", json!("Prepared X"));
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .map(|p| p.join("core-observe"))
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_core-observe")));
    let mut native = Native {
        bin,
        evidence,
        serial: 0,
        main: main.clone(),
        work: work.clone(),
    };
    native.search(&main, "login");
    native.search(&work, "login");
    let combined = native.clone_store(&main, "combined");
    field(&combined, 2, "name", json!("Prepared X"));
    let untouched = sql(
        &combined,
        "SELECT id,record FROM nodes WHERE id<>?1 ORDER BY id",
        &[id(2)],
    );
    let query = "SELECT rowid,title,body,names,tags FROM search_fts ORDER BY rowid";
    let postings = sql(&combined, query, &[]);
    let cold = native.prepared(&combined);
    for (key, expected) in [
        ("accepted_main", 6),
        ("prepared_worktree", 1),
        ("computed", 0),
        ("changed_rows", 1),
        ("search_bodies_reused", 7),
    ] {
        assert_eq!(cold["reuse"][key], expected);
    }
    assert_eq!(
        sql(
            &combined,
            "SELECT id,record FROM nodes WHERE id<>?1 ORDER BY id",
            &[id(2)]
        ),
        untouched
    );
    assert_eq!(
        sql(&combined, query, &[])
            .iter()
            .zip(&postings)
            .filter(|(a, b)| a != b)
            .count(),
        1
    );
    let warm = fs::read(db(&combined)).unwrap();
    for _ in 0..3 {
        assert_eq!(native.prepared(&combined)["reuse"]["changed_rows"], 0);
        assert_eq!(fs::read(db(&combined)).unwrap(), warm);
    }
    native.equality(&combined);
    authored(&combined, 0);
    field(&combined, 0, "name", json!("Added child"));
    symlink(
        format!("../../{}", id(0)),
        node(&combined, 1).join("children").join(id(0)),
    )
    .unwrap();
    order(&combined, 1, &[2, 3, 0]);
    let prior = sql(&combined, query, &[]);
    let inserted = native.prepared(&combined);
    for (key, expected) in [
        ("changed_dependencies", 1),
        ("computed", 2),
        ("changed_rows", 2),
    ] {
        assert_eq!(inserted["reuse"][key], expected);
    }
    assert_eq!(
        sql(
            &combined,
            "SELECT rowid,title,body,names,tags FROM search_fts WHERE rowid<=7 ORDER BY rowid",
            &[]
        ),
        prior
    );
    native.equality(&combined);
    field(&combined, 4, "name", json!("Uncommitted unique"));
    field(&combined, 4, "micro_summary", json!("Changed summary Ω"));
    let path = node(&combined, 4).join("node.md");
    fs::write(
        &path,
        fs::read_to_string(&path).unwrap() + "Unique body change.\n",
    )
    .unwrap();
    assert_eq!(native.prepared(&combined)["reuse"]["computed"], 3);
    assert_eq!(
        native.search(&combined, "unique")["results"][0]["id"],
        id(4)
    );
    fs::rename(
        node(&combined, 2).join("children").join(id(4)),
        node(&combined, 3).join("children").join(id(4)),
    )
    .unwrap();
    order(&combined, 2, &[]);
    order(&combined, 3, &[4]);
    field(&combined, 2, "see_also", json!([]));
    native.equality(&combined);
    order(&combined, 1, &[0, 3, 2]);
    native.equality(&combined);
    fs::remove_dir_all(node(&combined, 0)).unwrap();
    fs::remove_file(node(&combined, 1).join("children").join(id(0))).unwrap();
    order(&combined, 1, &[3, 2]);
    native.equality(&combined);
    let invalid = native.clone_store(&main, "dangling-reference");
    fs::remove_dir_all(node(&invalid, 6)).unwrap();
    fs::remove_file(node(&invalid, 5).join("children").join(id(6))).unwrap();
    order(&invalid, 5, &[7]);
    let before = fs::read(db(&invalid)).unwrap();
    native.call(json!({"op":"index_prepared","store":invalid,"accepted_main":main,"prepared_worktree":work}),"error");
    assert_eq!(fs::read(db(&invalid)).unwrap(), before);
    for mutation in [
        "missing",
        "corrupt",
        "record",
        "column",
        "schema",
        "parser",
        "tokenizer",
        "links",
    ] {
        let donor = native.clone_store(&work, mutation);
        match mutation {
            "missing" => fs::remove_file(db(&donor)).unwrap(),
            "corrupt" => fs::write(db(&donor), "corrupt").unwrap(),
            _ => {
                let c = Connection::open(db(&donor)).unwrap();
                match mutation {
                    "record" => c.execute("UPDATE nodes SET record='{}'", []),
                    "column" => c.execute("UPDATE nodes SET name='forged'", []),
                    "links" => c.execute("UPDATE links SET reason='forged'", []),
                    _ => c.execute(
                        "UPDATE metadata SET value=?1 WHERE key=?2",
                        [
                            format!("forged-{mutation}"),
                            if mutation == "schema" {
                                "schema"
                            } else {
                                "processing"
                            }
                            .into(),
                        ],
                    ),
                }
                .unwrap();
            }
        }
        let result=native.call(json!({"op":"index_prepared","store":combined,"accepted_main":main,"prepared_worktree":donor}),"current");
        assert_eq!(result["prepared_worktree_available"], false);
        assert_eq!(result["reuse"]["prepared_worktree"], 0);
        assert_eq!(
            result["view"],
            native.call(
                json!({"op":"index","store":combined,"mode":"full"}),
                "current"
            )
        );
    }
    let missing = native.clone_store(&main, "missing-main");
    fs::remove_file(db(&missing)).unwrap();
    let fallback=native.call(json!({"op":"index_prepared","store":combined,"accepted_main":missing,"prepared_worktree":work}),"current");
    assert_eq!(fallback["accepted_main_available"], false);
    assert_eq!(fallback["reuse"]["accepted_main"], 0);
    assert_eq!(fallback["reuse"]["prepared_worktree"], 0);
}

#[test]
fn native_prepared_source_cache_races_kill_and_reconstruction() {
    let temp = tempfile::tempdir().unwrap();
    let evidence = if let Some(p) = std::env::var_os("SIFTTEXT_TEST_EVIDENCE") {
        let p = PathBuf::from(format!("{}-races", PathBuf::from(p).display()));
        assert!(p.is_absolute());
        fs::create_dir(&p).unwrap();
        p
    } else {
        temp.path().to_path_buf()
    };
    let initial = evidence.join("initial");
    fixture(&initial);
    let main = evidence.join("main");
    copy(&initial, &main);
    let work = evidence.join("work");
    copy(&initial, &work);
    field(&main, 3, "name", json!("Current Y"));
    field(&work, 2, "name", json!("Prepared X"));
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .map(|p| p.join("core-observe"))
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_core-observe")));
    let mut native = Native {
        bin,
        evidence,
        serial: 0,
        main: main.clone(),
        work: work.clone(),
    };
    native.search(&main, "login");
    native.search(&work, "login");
    let combined = native.clone_store(&main, "combined");
    field(&combined, 2, "name", json!("Prepared X"));
    let accepted = native.prepared(&combined)["view"].clone();
    let main_bytes = fs::read(db(&main)).unwrap();
    let work_bytes = fs::read(db(&work)).unwrap();
    let source = node(&combined, 2).join("node.md");
    let original = fs::read(&source).unwrap();
    let bytes = fs::read(db(&combined)).unwrap();
    native.paused(
        &combined,
        || {
            let replacement = source.with_extension("replacement");
            fs::write(&replacement, &original).unwrap();
            fs::rename(replacement, &source).unwrap();
        },
        false,
    );
    assert_eq!(fs::read(db(&combined)).unwrap(), bytes);
    assert_eq!(native.prepared(&combined)["view"], accepted);

    native.paused(
        &combined,
        || field(&combined, 2, "name", json!("Race current")),
        false,
    );
    assert_eq!(fs::read(db(&combined)).unwrap(), bytes);
    let changed = native.search(&combined, "race");
    assert_eq!(changed["total_matches"], 1);
    assert_eq!(changed["results"][0]["id"], id(2));
    assert_ne!(changed["version"], accepted["version"]);

    let old_db = combined.join(".sifttext/old-index.sqlite");
    let before = fs::read(db(&combined)).unwrap();
    native.paused(
        &combined,
        || fs::rename(db(&combined), &old_db).unwrap(),
        false,
    );
    assert_eq!(fs::read(&old_db).unwrap(), before);
    native.prepared(&combined);
    let before = fs::read(db(&combined)).unwrap();
    native.paused(&combined, || {}, true);
    assert_eq!(fs::read(db(&combined)).unwrap(), before);
    assert_eq!(
        native.prepared(&combined)["view"]["version"],
        changed["version"]
    );

    fs::write(&source, original).unwrap();
    assert_eq!(native.prepared(&combined)["view"], accepted);
    fs::remove_dir_all(combined.join(".sifttext")).unwrap();
    assert_eq!(native.prepared(&combined)["view"], accepted);
    native.equality(&combined);
    assert_eq!(fs::read(db(&main)).unwrap(), main_bytes);
    assert_eq!(fs::read(db(&work)).unwrap(), work_bytes);
}
