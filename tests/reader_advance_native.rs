//! Native CLI/Git parity for home_reader_advance.mjs. Build checked assets first.
//! SIFTTEXT_TEST_EVIDENCE retains receipts in a NEW directory.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::{fs::symlink, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
fn text(p: &Path) -> &str {
    p.to_str().unwrap()
}
/// CLI receipts are mid-migration: LEAN by default, JSON where the schema is
/// nested. Decode the dialect actually emitted so a receipt moving format does
/// not silently break these content assertions.
fn receipt(output: &str) -> Value {
    let body = output.trim_start();
    if body.starts_with('{') || body.starts_with('[') {
        serde_json::from_str(body).unwrap()
    } else {
        sifttext_file_graph::outlines::decode_lean(body).unwrap()
    }
}
struct Native {
    evidence: PathBuf,
    git_home: PathBuf,
    records: Vec<Value>,
}
impl Native {
    fn command(&self, bin: &str, args: &[&str]) -> Command {
        let mut c = Command::new(bin);
        for (key, _) in std::env::vars_os() {
            if key.as_encoded_bytes().starts_with(b"GIT_") {
                c.env_remove(key);
            }
        }
        c.args(args)
            .current_dir(&self.evidence)
            .env("HOME", &self.git_home)
            .env("XDG_CONFIG_HOME", self.git_home.join(".config"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        c
    }
    fn record(&mut self, command: &str, args: &[&str], out: &std::process::Output) {
        self.records.push(json!({"command":command,"args":args,"exit":out.status.code(),"signal":out.status.signal(),
            "stdout":String::from_utf8_lossy(&out.stdout),"stderr":String::from_utf8_lossy(&out.stderr)}));
        fs::write(
            self.evidence.join("commands.json"),
            serde_json::to_vec_pretty(&self.records).unwrap(),
        )
        .unwrap();
    }
    fn run(&mut self, command: &str, args: &[&str], ok: bool) -> String {
        let out = self.command(command, args).output().unwrap();
        self.record(command, args, &out);
        assert_eq!(out.status.signal(), None, "{out:?}");
        assert_eq!(out.status.success(), ok, "{command} {args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    }
    fn cli(&mut self, args: &[&str]) -> Value {
        receipt(&self.run(env!("CARGO_BIN_EXE_sifttext"), args, true))
    }
    fn refuse(&mut self, args: &[&str]) {
        self.run(env!("CARGO_BIN_EXE_sifttext"), args, false);
    }
    fn git(&mut self, home: &Path, args: &[&str]) -> String {
        let mut all = vec![
            "-C",
            text(home),
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgSign=false",
        ];
        all.extend_from_slice(args);
        self.run("git", &all, true).trim().into()
    }
    fn publish(&mut self, home: &Path, allocation: &Value) -> Value {
        let draft = Path::new(allocation["worktree"].as_str().unwrap());
        self.run(
            env!("CARGO_BIN_EXE_sifttext"),
            &["check", text(draft)],
            true,
        );
        self.git(draft, &["add", "--", "nodes", "trees"]);
        self.git(
            draft,
            &["commit", "-m", "knowledge: synthetic reader trial"],
        );
        let commit = self.git(draft, &["rev-parse", "HEAD"]);
        // Exercise explicit advance on a lagging reader. The low-level publisher
        // retains that contract; normal CLI submit now completes the home itself.
        let request = json!({"repo":home,"accepted_ref":"refs/heads/main",
            "draft":draft,"commit":commit,"base":allocation["base"],"explicit":[]});
        let observer = env!("CARGO_BIN_EXE_git-submit-observe");
        let mut child = self
            .command(observer, &[])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&request).unwrap())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        self.record(observer, &[], &out);
        assert!(out.status.success(), "{out:?}");
        let r: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(r["status"], "current");
        assert_eq!(r["published"], true);
        assert_eq!(self.git(draft, &["rev-parse", "HEAD"]), commit);
        r
    }
}
// Always reap the interrupted CLI and release its SQLite transaction on unwind.
struct Pending(Option<Child>);
impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
#[test]
fn reader_cli_preserves_history_and_recovers_after_post_ff_sigkill() {
    let tmp = tempfile::tempdir().unwrap();
    let evidence = match std::env::var_os("SIFTTEXT_TEST_EVIDENCE") {
        Some(p) => {
            let p = PathBuf::from(p);
            assert!(p.is_absolute());
            fs::create_dir(&p).unwrap();
            p.canonicalize().unwrap()
        }
        None => tmp.path().canonicalize().unwrap(),
    };
    let git_home = evidence.join("git-home");
    fs::create_dir(&git_home).unwrap();
    fs::write(git_home.join(".gitconfig"),"[user]\nname = Synthetic Fixture\nemail = fixture@example.invalid\n[commit]\ngpgSign = false\n[core]\nhooksPath = /dev/null\n").unwrap();
    let mut n = Native {
        evidence: evidence.clone(),
        git_home,
        records: vec![],
    };
    let home = evidence.join("home");
    let initial = n.cli(&["init", text(&home), "--empty"]);
    assert!(
        n.run(env!("CARGO_BIN_EXE_sifttext"), &["-h"], true)
            .contains("sifttext advance HOME")
    );
    let first = n.cli(&["worktree", text(&home)]);
    let second = n.cli(&["worktree", text(&home)]);
    let root = n.cli(&[
        "create",
        first["worktree"].as_str().unwrap(),
        "Advanceanchor",
        "Owns the synthetic reader advance.",
    ]);
    let child = n.cli(&[
        "create",
        first["worktree"].as_str().unwrap(),
        "Advancefirst",
        "Owns a first published child.",
        root["id"].as_str().unwrap(),
    ]);
    let separate = n.cli(&[
        "create",
        second["worktree"].as_str().unwrap(),
        "Advanceseparate",
        "Owns a second independent published change.",
    ]);
    let published1 = n.publish(&home, &first);
    let published2 = n.publish(&home, &second);
    assert_eq!(
        n.git(&home, &["rev-parse", "HEAD"]),
        initial["commit"].as_str().unwrap()
    );
    assert_eq!(n.cli(&["index", text(&home)])["nodes"], 0);
    n.refuse(&["worktree", text(&home)]);
    let detached = n.cli(&["accepted", text(&home)]);
    assert_eq!(detached["status"], "ready");
    assert_eq!(detached["home_advanced"], false);
    let view = Path::new(detached["view"].as_str().unwrap());
    assert_ne!(view, home);
    assert_eq!(
        n.git(&home, &["rev-parse", "HEAD"]),
        initial["commit"].as_str().unwrap()
    );
    assert_eq!(
        n.git(view, &["rev-parse", "HEAD"]),
        published2["candidate"].as_str().unwrap()
    );
    let advanced = n.cli(&["advance", text(&home)]);
    assert_eq!(advanced["status"], "advanced");
    assert_eq!(advanced["accepted"], published2["candidate"]);
    assert_eq!(advanced.as_object().unwrap().len(), 4);
    let output = sifttext_file_graph::lean_render::render(&advanced).unwrap() + "\n";
    let tokens = tiktoken_rs::o200k_base_singleton().lock().encode_ordinary(&output).len();
    println!("advance: {} bytes, {tokens} o200k tokens", output.len());
    assert!(tokens < 250);
    assert_eq!(advanced["version"], published2["version"]);
    assert_eq!(
        n.git(&home, &["rev-parse", "HEAD"]),
        published2["candidate"].as_str().unwrap()
    );
    assert_eq!(n.cli(&["advance", text(&home)])["status"], "current");
    for node in [&root, &separate] {
        assert_eq!(
            n.run(
                env!("CARGO_BIN_EXE_sifttext"),
                &["read", text(&home), node["id"].as_str().unwrap()],
                true
            ),
            fs::read_to_string(node["path"].as_str().unwrap()).unwrap()
        );
    }
    for name in ["Advancefirst", "Advanceseparate"] {
        let s = n.cli(&["search", text(&home), name]);
        assert_eq!(s["version"], advanced["version"]);
        assert_eq!(s["total_matches"], 1);
    }
    let sql = n.cli(&["sql", text(&home), "SELECT count(*) FROM nodes"]);
    assert_eq!(sql["version"], advanced["version"]);
    assert_eq!(sql["rows"][0]["c0"], "3");
    for full in [false, true] {
        let mut args = vec!["outline", text(&home), root["id"].as_str().unwrap()];
        if full {
            args.push("full");
        }
        assert!(
            n.run(env!("CARGO_BIN_EXE_sifttext"), &args, true)
                .contains(if full {
                    "coverage:full"
                } else {
                    "coverage:shallow"
                })
        );
    }
    n.refuse(&["sql", text(&home), "DELETE FROM nodes"]);
    let third = n.cli(&["worktree", text(&home)]);
    assert_eq!(third["base"], published2["candidate"]);
    let relative = format!("nodes/{}/node.md", child["id"].as_str().unwrap());
    let authored = home.join(&relative);
    let mut intent = fs::read(&authored).unwrap();
    intent.extend_from_slice(b"\nUnfinished authored intent.\n");
    fs::write(&authored, &intent).unwrap();
    fs::write(home.join("user-note"), "untracked intent\n").unwrap();
    n.refuse(&["advance", text(&home)]);
    assert_eq!(fs::read(&authored).unwrap(), intent);
    assert_eq!(
        fs::read_to_string(home.join("user-note")).unwrap(),
        "untracked intent\n"
    );
    assert_eq!(
        n.git(&home, &["rev-parse", "HEAD"]),
        published2["candidate"].as_str().unwrap()
    );
    n.git(&home, &["restore", "--", &relative]);
    fs::remove_file(home.join("user-note")).unwrap();
    let alias = evidence.join("alias");
    symlink(&home, &alias).unwrap();
    let nodes = home.join("nodes");
    for unsafe_home in [text(&alias), text(&nodes), "relative-home"] {
        n.refuse(&["advance", unsafe_home]);
    }
    n.git(&home, &["switch", "-c", "user-reader"]);
    n.refuse(&["advance", text(&home)]);
    n.git(&home, &["checkout", "--detach"]);
    n.git(
        &home,
        &[
            "commit",
            "--allow-empty",
            "-m",
            "knowledge: divergent reader intent",
        ],
    );
    let divergent = n.git(&home, &["rev-parse", "HEAD"]);
    n.refuse(&["advance", text(&home)]);
    assert_eq!(n.git(&home, &["rev-parse", "HEAD"]), divergent);
    assert_eq!(
        n.git(&home, &["rev-parse", "refs/heads/main"]),
        published2["candidate"].as_str().unwrap()
    );
    n.git(
        &home,
        &[
            "checkout",
            "--detach",
            published2["candidate"].as_str().unwrap(),
        ],
    );
    n.cli(&[
        "create",
        third["worktree"].as_str().unwrap(),
        "Advanceinterrupt",
        "Owns interruption recovery.",
    ]);
    let published3 = n.publish(&home, &third);
    let mut database = rusqlite::Connection::open(home.join(".sifttext/index.sqlite")).unwrap();
    let transaction = database
        .transaction_with_behavior(rusqlite::TransactionBehavior::Exclusive)
        .unwrap();
    let args = ["advance", text(&home)];
    let mut pending = Pending(Some(
        n.command(env!("CARGO_BIN_EXE_sifttext"), &args)
            .spawn()
            .unwrap(),
    ));
    let start = Instant::now();
    while n.git(&home, &["rev-parse", "HEAD"]) != published3["candidate"].as_str().unwrap() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "advance did not reach ff"
        );
        assert!(
            pending.0.as_mut().unwrap().try_wait().unwrap().is_none(),
            "advance exited before interruption"
        );
        thread::sleep(Duration::from_millis(20));
    }
    assert!(pending.0.as_mut().unwrap().try_wait().unwrap().is_none());
    pending.0.as_mut().unwrap().kill().unwrap(); // Unix Child::kill is SIGKILL.
    let out = pending.0.take().unwrap().wait_with_output().unwrap();
    n.record(env!("CARGO_BIN_EXE_sifttext"), &args, &out);
    assert_eq!(out.status.signal(), Some(libc::SIGKILL));
    assert!(
        out.stdout.is_empty(),
        "interrupted advance cannot claim current"
    );
    transaction.rollback().unwrap();
    drop(database);
    assert_eq!(
        n.git(&home, &["rev-parse", "HEAD"]),
        published3["candidate"].as_str().unwrap()
    );
    assert_eq!(
        n.git(&home, &["rev-parse", "refs/heads/main"]),
        published3["candidate"].as_str().unwrap()
    );
    let repaired = n.cli(&["advance", text(&home)]);
    assert_eq!(repaired["status"], "current");
    assert_eq!(repaired["version"], published3["version"]);
    assert_eq!(
        n.cli(&["sql", text(&home), "SELECT count(*) FROM nodes"])["rows"][0]["c0"],
        "4"
    );
    assert_eq!(
        n.cli(&["search", text(&home), "Advanceinterrupt"])["total_matches"],
        1
    );
    assert_eq!(
        n.git(view, &["rev-parse", "HEAD"]),
        published2["candidate"].as_str().unwrap(),
        "older detached API view untouched"
    );
    assert_eq!(n.git(&home, &["status", "--porcelain"]), "");
    fs::write(evidence.join("result.json"),serde_json::to_vec_pretty(&json!({"status":"passed","initial":initial,"published1":published1,"published2":published2,"detached":detached,"advanced":advanced,"published3":published3,"repaired":repaired,"commands":n.records.len(),"limits":"Synthetic CLI/Git interruption parity; no Linux, large dataset, default bundle, public release or performance claim."})).unwrap()).unwrap();
}
