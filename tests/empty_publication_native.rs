//! Empty-store process controls; other publication families live in successor/prepared_publication*.rs.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};
#[path = "common/native_resources.rs"]
mod resources;
fn id() -> &'static str {
    "60000000-0000-4000-8000-000000000001"
}
fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "-c",
            "user.name=Synthetic Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn root(p: &Path, summary: &str) {
    let n = p.join("nodes").join(id());
    fs::create_dir_all(n.join("children")).unwrap();
    fs::create_dir_all(p.join("trees")).unwrap();
    fs::write(n.join("children/.keep"), "").unwrap();
    fs::write(n.join("children.order"), "").unwrap();
    if !p.join("trees").join(id()).exists() {
        symlink(format!("../nodes/{}", id()), p.join("trees").join(id())).unwrap();
    }
    let fields = json!({"id":id(),"name":"Node","micro_summary":summary,"status":"in_progress","see_also":[]});
    let metadata = fields
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| format!("{k}: {v}\n"))
        .collect::<String>();
    fs::write(
        n.join("node.md"),
        format!("---\n{metadata}---\nReal first root"),
    )
    .unwrap();
}
fn commit(p: &Path) -> String {
    git(p, &["add", "nodes", "trees"]);
    git(p, &["commit", "-qm", "authored edit"]);
    git(p, &["rev-parse", "HEAD"])
}
fn draft(repo: &Path, base: &str, path: &Path, name: &str) {
    git(
        repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            name,
            path.to_str().unwrap(),
            base,
        ],
    );
}
fn fingerprint(p: &Path) -> Value {
    fn walk(p: &Path, rel: PathBuf, files: &mut BTreeMap<PathBuf, String>) {
        for e in fs::read_dir(p.join(&rel)).unwrap() {
            let e = e.unwrap();
            if rel.as_os_str().is_empty() && e.file_name() == ".git" {
                continue;
            }
            let r = rel.join(e.file_name());
            let m = fs::symlink_metadata(e.path()).unwrap();
            if m.file_type().is_symlink() {
                files.insert(
                    r,
                    format!("link:{}", fs::read_link(e.path()).unwrap().display()),
                );
            } else if m.is_dir() {
                walk(p, r, files);
            } else {
                files.insert(
                    r,
                    format!("{:x}", Sha256::digest(fs::read(e.path()).unwrap())),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(p, PathBuf::new(), &mut files);
    let index = git(
        p,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    );
    json!({"files":files,"head":git(p,&["rev-parse","HEAD"]),"branch":git(p,&["symbolic-ref","-q","HEAD"]),"status":git(p,&["status","--porcelain"]),"index":format!("{:x}",Sha256::digest(fs::read(index).unwrap()))})
}
fn check_rejection_rows(text: &str) {
    let mut lines = text.lines();
    let header = lines
        .find_map(|line| line.strip_prefix("rows["))
        .expect("missing rejection rows");
    let (count, fields) = header.split_once("]:").unwrap();
    let count: usize = count.parse().unwrap();
    if count > 0 {
        assert_eq!(fields, "check\tid\tlaw\trule\tsource\ttarget\ttokens");
    } else {
        assert!(fields.is_empty());
    }
    for _ in 0..count {
        let cells: Vec<_> = lines
            .next()
            .unwrap()
            .strip_prefix("  ")
            .unwrap()
            .split('\t')
            .collect();
        assert_eq!(cells.len(), 7);
        let rule = cells[3];
        assert!(
            rule.len() == 36
                && rule
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c) || c == b'-'),
            "invalid rejection rule: {rule}"
        );
    }
    assert!(lines.next().is_none(), "unexpected data after rows");
}
#[test]
fn rejection_rows_require_every_rule_and_declared_row() {
    let header = "rows[1]:check\tid\tlaw\trule\tsource\ttarget\ttokens\n";
    let row = "  capture_or_graph_invalid\t_\t_\te8234ea5-3467-4f2b-8ca2-2bcc5cc80d11\t_\t_\t_";
    check_rejection_rows("rows[0]:");
    check_rejection_rows(&format!("{header}{row}"));
    for bad in [
        format!(
            "{header}{}",
            row.replace("e8234ea5-3467-4f2b-8ca2-2bcc5cc80d11", "_")
        ),
        header.to_owned(),
        format!("{header}{row}\n{row}"),
    ] {
        assert!(std::panic::catch_unwind(|| check_rejection_rows(&bad)).is_err());
    }
}
struct Native {
    bin: PathBuf,
    evidence: PathBuf,
    calls: usize,
    submits: usize,
}
impl Native {
    fn call(&mut self, name: &str, q: Value, pass: bool) -> Value {
        let start = Instant::now();
        let mut c = Command::new("/usr/bin/time");
        #[cfg(target_os = "macos")]
        c.arg("-l");
        #[cfg(target_os = "linux")]
        c.arg("-v");
        let mut child = c
            .arg(self.bin.join(name))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(q.to_string().as_bytes())
            .unwrap();
        let (sampled_rss, sampled_fds, resource_sample) = resources::sample(child.id());
        let out = child.wait_with_output().unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        let stderr = String::from_utf8(out.stderr).unwrap();
        let receipt = json!({"bin":name,"request":q,"exit":out.status.code(),"stdout":text,"stderr":stderr,"ms":start.elapsed().as_secs_f64()*1000.,"native_rss_bytes":resources::native_rss(&stderr),"single_process_tree_sample":{"rss_bytes":sampled_rss,"numeric_fds":sampled_fds,"sample":resource_sample,"limit":"single sample, not peak evidence"}});
        fs::write(
            self.evidence.join(format!("call-{}.json", self.calls)),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
        self.calls += 1;
        assert_eq!(out.status.success(), pass, "{receipt:#}");
        if pass {
            let r: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(r["status"], "current", "{receipt:#}");
            r
        } else {
            assert!(
                !text.contains(self.evidence.to_str().unwrap()),
                "private path leaked"
            );
            // Match the legacy invoke's independent row/rule checks as well as status.
            assert!(text.lines().any(|s| s == "status:rejected"), "{text}");
            check_rejection_rows(&text);
            let check = text
                .lines()
                .find_map(|s| s.strip_prefix("check:"))
                .unwrap_or("");
            json!({"status":"rejected","check":check})
        }
    }
    fn submit(&mut self, repo: &Path, base: Option<&str>, d: Option<&Path>, pass: bool) -> Value {
        self.submits += 1;
        let commit = d.map(|p| git(p, &["rev-parse", "HEAD"]));
        self.call("git-submit-observe",json!({"repo":repo,"accepted_ref":"refs/heads/main","draft":d,"base":base,"commit":commit}),pass)
    }
    fn view(&mut self, r: &Value) -> Value {
        let view = Path::new(r["view"].as_str().unwrap());
        assert_eq!(
            git(view, &["rev-parse", "HEAD"]),
            r["candidate"].as_str().unwrap()
        );
        assert_eq!(
            git(view, &["rev-parse", "HEAD^{tree}"]),
            r["tree"].as_str().unwrap()
        );
        let indexed = self.call(
            "core-observe",
            json!({"op":"index","store":view,"mode":"incremental"}),
            true,
        );
        assert_eq!(indexed["version"], r["version"]);
        let outline = self.call(
            "core-observe",
            json!({"op":"outline_inspect","store":view}),
            true,
        );
        assert_eq!(outline["version"], r["version"]);
        indexed
    }
}
#[test]
fn empty_baseline_first_root_last_deletion_and_unsafe_symlink_preserve_drafts() {
    let temp = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("EMPTY_PUBLICATION_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("evidence"));
    assert!(evidence.is_absolute());
    fs::create_dir(&evidence).unwrap();
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_BIN_EXE_git-submit-observe"))
                .parent()
                .unwrap()
                .into()
        });
    let mut native = Native {
        bin,
        evidence: evidence.clone(),
        calls: 0,
        submits: 0,
    };
    for kept in [false, true] {
        let folder = evidence.join(if kept { "kept" } else { "absent" });
        fs::create_dir(&folder).unwrap();
        let repo = folder.join("repo");
        fs::create_dir(&repo).unwrap();
        fs::write(repo.join(".gitignore"), ".sifttext/\n*.lean\n*.meta.json\n").unwrap();
        if kept {
            fs::create_dir(repo.join("trees")).unwrap();
            fs::write(repo.join("trees/.keep"), "").unwrap();
        }
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "empty baseline"]);
        let base = git(&repo, &["rev-parse", "HEAD"]);
        git(&repo, &["checkout", "--detach", "-q", &base]);
        let accepted_files = git(&repo, &["ls-tree", "-r", "HEAD"]);
        let recovery = native.submit(&repo, None, None, true);
        assert_eq!(recovery["candidate"], base);
        assert_eq!(native.view(&recovery)["nodes"].as_array().unwrap().len(), 0);
        assert_eq!(git(&repo, &["ls-tree", "-r", "main"]), accepted_files);
        assert!(!repo.join("nodes").exists());
        assert_eq!(git(&repo, &["status", "--porcelain"]), "");
        let first = folder.join("first");
        draft(&repo, &base, &first, "first");
        root(&first, " ");
        commit(&first);
        let invalid = fingerprint(&first);
        native.submit(&repo, Some(&base), Some(&first), false);
        assert_eq!(git(&repo, &["rev-parse", "main"]), base);
        assert_eq!(fingerprint(&first), invalid);
        root(&first, "Summary");
        let first_sha = commit(&first);
        let valid = fingerprint(&first);
        let published = native.submit(&repo, Some(&base), Some(&first), true);
        assert_eq!(
            git(&repo, &["rev-parse", "main"]),
            published["candidate"].as_str().unwrap()
        );
        assert_eq!(
            published["validation"]["before_version"],
            recovery["version"]
        );
        let nodes = native.view(&published)["nodes"].as_array().unwrap().clone();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0]["id"], id());
        assert_eq!(nodes[0]["content"], "Real first root");
        let view = Path::new(published["view"].as_str().unwrap());
        assert_eq!(
            git(
                view,
                &["show", &format!("{first_sha}:nodes/{}/node.md", id())]
            ),
            git(view, &["show", &format!("HEAD:nodes/{}/node.md", id())])
        );
        assert_eq!(fingerprint(&first), valid);
        assert!(!repo.join("nodes").exists());
        fs::remove_dir_all(view.join(".sifttext")).unwrap();
        let retry = native.submit(&repo, Some(&base), Some(&first), true);
        assert_eq!(retry["candidate"], published["candidate"]);
        native.view(&retry);
        assert_eq!(fingerprint(&first), valid);
        let deletion = folder.join("deletion");
        let published_sha = published["candidate"].as_str().unwrap();
        draft(&repo, published_sha, &deletion, "deletion");
        fs::remove_dir_all(deletion.join("nodes").join(id())).unwrap();
        fs::remove_file(deletion.join("trees").join(id())).unwrap();
        commit(&deletion);
        let deleted = native.submit(&repo, Some(published_sha), Some(&deletion), true);
        assert_eq!(native.view(&deleted)["nodes"].as_array().unwrap().len(), 0);
        assert_eq!(
            native.submit(&repo, None, None, true)["candidate"],
            deleted["candidate"]
        );
        let unsafe_draft = folder.join("unsafe");
        draft(&repo, &base, &unsafe_draft, "unsafe");
        let outside = folder.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel"), "unchanged").unwrap();
        symlink(&outside, unsafe_draft.join("nodes")).unwrap();
        git(&unsafe_draft, &["add", "nodes"]);
        git(&unsafe_draft, &["commit", "-qm", "unsafe directory"]);
        let tip = git(&repo, &["rev-parse", "main"]);
        let unsafe_before = fingerprint(&unsafe_draft);
        assert_eq!(
            native.submit(&repo, Some(&base), Some(&unsafe_draft), false)["check"],
            "unsafe_submission"
        );
        assert_eq!(git(&repo, &["rev-parse", "main"]), tip);
        assert_eq!(fingerprint(&unsafe_draft), unsafe_before);
        assert_eq!(
            fs::read_to_string(outside.join("sentinel")).unwrap(),
            "unchanged"
        );
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
    }
    assert_eq!(native.submits, 14);
    fs::write(evidence.join("passed.json"),serde_json::to_vec_pretty(&json!({"submission_cases":native.submits,"native_calls":native.calls,"scope":"empty baselines with absent/kept trees, invalid/valid first root, retry/cache-loss, last-root deletion/recovery, unsafe symlink nonmutation","other_coverage":"successor/prepared_publication*.rs and unsafe_submission.rs cover nonempty publication, races, recovery and resources"})).unwrap()).unwrap();
}
