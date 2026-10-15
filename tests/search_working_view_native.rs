//! Search CLI working-view controls; independent differential/resource coverage lives in search_differential_native.rs.
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
    format!("40000000-0000-4000-8000-{n:012}")
}
fn file(p: &Path, n: usize) -> PathBuf {
    p.join("nodes").join(id(n)).join("node.md")
}
fn text(n: usize, name: &str) -> String {
    format!(
        "---\nid: {:?}\nname: {name:?}\nmicro_summary: \"Reset guidance\"\nstatus: \"in_progress\"\nsee_also: []\nscope: \"Authored scope\"\n---\n{}\n",
        id(n),
        if n == 4 {
            "Unrelated content."
        } else {
            "Login reset. Resetting users authenticate. Ω."
        }
    )
}
fn fixture(p: &Path) {
    fs::create_dir_all(p.join("trees")).unwrap();
    for n in 1..=4 {
        let node = p.join("nodes").join(id(n));
        fs::create_dir_all(node.join("children")).unwrap();
        fs::write(file(p, n), text(n, &format!("Login {n}"))).unwrap();
        let children = match n {
            1 => vec![2, 3],
            2 => vec![4],
            _ => vec![],
        };
        fs::write(
            node.join("children.order"),
            children.iter().map(|n| id(*n) + "\n").collect::<String>(),
        )
        .unwrap();
        for c in children {
            symlink(
                format!("../../{}", id(c)),
                node.join("children").join(id(c)),
            )
            .unwrap();
        }
    }
    symlink(format!("../nodes/{}", id(1)), p.join("trees").join(id(1))).unwrap();
}
fn command(bin: &Path) -> Command {
    let mut c = Command::new(bin);
    for (key, _) in std::env::vars_os() {
        if key.as_encoded_bytes().starts_with(b"GIT_") {
            c.env_remove(key);
        }
    }
    c.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c
}
struct Native {
    bin: PathBuf,
    evidence: PathBuf,
    sequence: usize,
}
impl Native {
    fn request(&self, p: &Path, query: &str) -> Value {
        json!({"op":"search","store":p,"query":query,"readable_ids":(1..=4).map(id).collect::<Vec<_>>(),"limit":3,"offset":0})
    }
    fn inspect(&mut self, q: &Value, out: std::process::Output, current: bool) -> Value {
        fs::write(self.evidence.join(format!("call-{}.json",self.sequence)),serde_json::to_vec_pretty(&json!({"request":q,"exit":out.status.code(),"stdout":String::from_utf8_lossy(&out.stdout),"stderr":String::from_utf8_lossy(&out.stderr)})).unwrap()).unwrap();
        self.sequence += 1;
        assert_eq!(
            out.status.code(),
            Some(if current { 0 } else { 1 }),
            "{out:?}"
        );
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        if current {
            assert_eq!(v["status"], "current");
            assert!(v["version"].is_string());
        } else {
            assert!(["stale", "error"].contains(&v["status"].as_str().unwrap()));
            assert!(v["version"].is_null());
            assert_eq!(v["results"], json!([]));
            assert_eq!(v["other_groups"], json!([]));
            assert_eq!(v["total_matches"], 0);
        }
        v
    }
    fn call(&mut self, q: Value, current: bool) -> Value {
        let mut child = command(&self.bin).spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(q.to_string().as_bytes())
            .unwrap();
        self.inspect(&q, child.wait_with_output().unwrap(), current)
    }
    fn search(&mut self, p: &Path, query: &str) -> Value {
        self.call(self.request(p, query), true)
    }
    fn git(&self, p: &Path, args: &[&str]) {
        let out = command(Path::new("git"))
            .args([
                "-C",
                p.to_str().unwrap(),
                "-c",
                "user.name=Search Fixture",
                "-c",
                "user.email=search@example.invalid",
                "-c",
                "commit.gpgSign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
    }
    fn paused(&mut self, p: &Path, mutate: bool) {
        let ready = self.evidence.join(format!("ready-{}", self.sequence));
        let release = ready.with_extension("release");
        let mut q = self.request(p, "login");
        q["pause_before_accept"] = json!({"ready":ready,"release":release});
        let mut child = command(&self.bin).spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(q.to_string().as_bytes())
            .unwrap();
        let start = Instant::now();
        while !ready.exists() {
            if start.elapsed() > Duration::from_secs(30) {
                child.kill().unwrap();
                let out = child.wait_with_output().unwrap();
                panic!("search barrier unavailable: {out:?}");
            }
            assert!(
                child.try_wait().unwrap().is_none(),
                "search exited before barrier"
            );
            thread::sleep(Duration::from_millis(5));
        }
        if mutate {
            fs::write(file(p, 2), text(2, "Newer accepted identity")).unwrap();
            fs::write(release, "go").unwrap();
            self.inspect(&q, child.wait_with_output().unwrap(), false);
        } else {
            child.kill().unwrap();
            let out = child.wait_with_output().unwrap();
            assert_eq!(
                std::os::unix::process::ExitStatusExt::signal(&out.status),
                Some(9)
            );
            fs::write(
                self.evidence.join("interrupted.json"),
                serde_json::to_vec_pretty(
                    &json!({"request":q,"signal":9,"stdout":String::from_utf8_lossy(&out.stdout)}),
                )
                .unwrap(),
            )
            .unwrap();
        }
    }
}
#[test]
fn search_cli_git_sparse_race_and_cache_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("SIFTTEXT_TEST_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("evidence"));
    fs::create_dir(&evidence).unwrap();
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_BIN_EXE_core-observe"))
                .parent()
                .unwrap()
                .into()
        })
        .join("core-observe");
    let mut native = Native {
        bin,
        evidence: evidence.clone(),
        sequence: 0,
    };
    let store = evidence.join("store");
    fixture(&store);
    let before = (1..=4)
        .map(|n| fs::read(file(&store, n)).unwrap())
        .collect::<Vec<_>>();
    let original = native.search(&store, "login");
    let mut q = native.request(&store, "login");
    q["readable_ids"] = json!([id(3)]);
    q["limit"] = json!(1);
    let page = native.call(q.clone(), true);
    assert_eq!(page["total_matches"], 1);
    assert_eq!(page["results"][0]["id"], id(3));
    q["offset"] = json!(1);
    let page = native.call(q, true);
    assert_eq!(page["total_matches"], 1);
    assert_eq!(page["results"], json!([]));
    let mut q = native.request(&store, "login");
    q["readable_ids"] = json!([]);
    assert_eq!(native.call(q, true)["total_matches"], 0);
    fs::write(
        store.join(".gitignore"),
        ".sifttext/\n*.lean\n*.meta.json\n",
    )
    .unwrap();
    native.git(&store, &["init", "-q", "-b", "main"]);
    native.git(&store, &["add", "nodes", "trees", ".gitignore"]);
    native.git(&store, &["commit", "-qm", "search: baseline"]);
    fs::write(file(&store, 2), text(2, "Git working edit")).unwrap();
    assert_ne!(
        native.search(&store, "login")["version"],
        original["version"]
    );
    native.git(&store, &["restore", "nodes"]);
    assert_eq!(
        native.search(&store, "login")["version"],
        original["version"]
    );
    fs::write(file(&store, 2), text(2, "Git committed edit")).unwrap();
    native.git(&store, &["add", "nodes"]);
    native.git(&store, &["commit", "-qm", "search: edit"]);
    assert_ne!(
        native.search(&store, "login")["version"],
        original["version"]
    );
    native.git(&store, &["revert", "--no-edit", "HEAD"]);
    assert_eq!(
        native.search(&store, "login")["version"],
        original["version"]
    );
    fs::write(file(&store, 4), text(4, "Login changed immediately")).unwrap();
    let edited = native.search(&store, "login");
    assert_ne!(edited["version"], original["version"]);
    let mut q = native.request(&store, "changed");
    q["under_node_id"] = json!(id(4));
    assert_eq!(native.call(q, true)["results"][0]["id"], id(4));
    let valid = fs::read_to_string(file(&store, 4)).unwrap();
    for bad in [
        "malformed draft".into(),
        valid.replace(
            "micro_summary: \"Reset guidance\"",
            "micro_summary: \"   \"",
        ),
    ] {
        fs::write(file(&store, 4), &bad).unwrap();
        native.call(native.request(&store, "login"), false);
        assert_eq!(fs::read_to_string(file(&store, 4)).unwrap(), bad);
    }
    fs::write(file(&store, 4), &valid).unwrap();
    assert_eq!(native.search(&store, "login")["version"], edited["version"]);
    fs::write(file(&store, 4), &before[3]).unwrap();
    assert_eq!(
        native.search(&store, "login")["version"],
        original["version"]
    );
    let twin = evidence.join("twin");
    fixture(&twin);
    fs::write(file(&twin, 2), text(2, "Uncommitted rename unique")).unwrap();
    assert_ne!(
        native.search(&twin, "login")["version"],
        original["version"]
    );
    assert_eq!(native.search(&twin, "unique")["results"][0]["id"], id(2));
    assert_eq!(native.search(&store, "unique")["total_matches"], 0);
    fs::remove_file(store.join(".sifttext/index.sqlite")).unwrap();
    assert_eq!(
        native.search(&store, "login")["results"],
        original["results"]
    );
    let sparse = evidence.join("sparse");
    fixture(&sparse);
    fs::remove_dir_all(sparse.join("nodes").join(id(4))).unwrap();
    let mut q = native.request(&sparse, "unrelated");
    q["base_store"] = json!(store);
    let view = native.call(q.clone(), true);
    assert_eq!(view["results"][0]["id"], id(4));
    let node = sparse.join("nodes").join(id(4));
    fs::create_dir_all(node.join("children")).unwrap();
    fs::write(node.join("children.order"), "").unwrap();
    fs::write(file(&sparse, 4), &before[3]).unwrap();
    assert_eq!(native.call(q, true), view);
    let old = store.join("nodes").join(id(2));
    let next = store.join("nodes").join(id(3));
    fs::rename(
        old.join("children").join(id(4)),
        next.join("children").join(id(4)),
    )
    .unwrap();
    fs::write(old.join("children.order"), "").unwrap();
    fs::write(next.join("children.order"), id(4) + "\n").unwrap();
    for (scope, count) in [(2, 0), (3, 1)] {
        let mut q = native.request(&store, "unrelated");
        q["under_node_id"] = json!(id(scope));
        let result = native.call(q, true);
        assert_eq!(result["total_matches"], count);
        if count == 1 {
            assert_eq!(result["results"][0]["id"], id(4));
        }
    }
    fs::rename(
        next.join("children").join(id(4)),
        old.join("children").join(id(4)),
    )
    .unwrap();
    fs::write(old.join("children.order"), id(4) + "\n").unwrap();
    fs::write(next.join("children.order"), "").unwrap();
    assert_eq!(
        native.search(&store, "login")["version"],
        original["version"]
    );
    assert_eq!(
        (1..=4)
            .map(|n| fs::read(file(&store, n)).unwrap())
            .collect::<Vec<_>>(),
        before
    );
    native.paused(&store, true);
    assert_eq!(native.search(&store, "newer")["results"][0]["id"], id(2));
    native.paused(&store, false);
    let current = native.search(&store, "login");
    let conn = rusqlite::Connection::open_with_flags(
        store.join(".sifttext/index.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let version: String = conn
        .query_row("select value from metadata where key='version'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(current["version"], version);
    let postings: i64 = conn
        .query_row("select count(*) from search_fts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(postings, 4);
    fs::write(
        evidence.join("readonly-sql.json"),
        serde_json::to_vec_pretty(
            &json!({"version":version,"postings":postings,"calls":native.sequence}),
        )
        .unwrap(),
    )
    .unwrap();
}
