use super::{Fixture, commit, git, id, node};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(super) fn fingerprint(p: &Path) -> Value {
    fn walk(p: &Path, rel: PathBuf, files: &mut BTreeMap<PathBuf, String>) {
        for entry in fs::read_dir(p.join(&rel)).unwrap() {
            let entry = entry.unwrap();
            if rel.as_os_str().is_empty() && entry.file_name() == ".git" {
                continue;
            }
            let r = rel.join(entry.file_name());
            let m = fs::symlink_metadata(entry.path()).unwrap();
            if m.file_type().is_symlink() {
                files.insert(
                    r,
                    format!("link:{}", fs::read_link(entry.path()).unwrap().display()),
                );
            } else if m.is_dir() {
                walk(p, r, files);
            } else {
                files.insert(r, crate::sha256(&fs::read(entry.path()).unwrap()));
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(p, PathBuf::new(), &mut files);
    let index = git(
        p,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    );
    json!({"files":files,"head":git(p,&["rev-parse","HEAD"]),"branch":git(p,&["symbolic-ref","-q","HEAD"]),"status":git(p,&["status","--porcelain"]),"index":crate::sha256(&fs::read(index).unwrap())})
}

#[test]
fn native_unsafe_submission_inputs_preserve_drafts_and_accepted_history() {
    let bin_dir = crate::native_bin_dir();
    let evidence = std::env::var_os("UNSAFE_SUBMISSION_EVIDENCE").map(PathBuf::from);
    if let Some(p) = &evidence {
        fs::create_dir_all(p).unwrap();
    }
    for case in [
        "dirty",
        "untracked",
        "unknown_commit",
        "unknown_base",
        "foreign_repo",
        "head_ref",
        "unregistered",
        "attributes",
        "checked_out_main",
        "symbolic_alias",
    ] {
        let f = Fixture::new();
        node(&f.a, 2, "Safe authored X");
        commit(&f.a);
        let mut q = f.request(&f.a);
        // Retain the separate repository for the foreign-worktree request.
        let foreign = Fixture::new();
        let copied = f._dir.path().join("unregistered");
        match case {
            "dirty" => node(&f.a, 2, "Uncommitted authored X"),
            "untracked" => fs::write(f.a.join("unknown"), "unknown").unwrap(),
            "unknown_commit" => q["commit"] = json!("f".repeat(40)),
            "unknown_base" => q["base"] = json!("f".repeat(40)),
            "foreign_repo" => {
                q = json!({"repo":f.repo,"accepted_ref":"refs/heads/main","draft":foreign.a,"commit":git(&foreign.a,&["rev-parse","HEAD"]),"base":f.base})
            }
            "head_ref" => q["accepted_ref"] = json!("HEAD"),
            "unregistered" => {
                assert!(
                    Command::new("cp")
                        .arg("-R")
                        .arg(&f.a)
                        .arg(&copied)
                        .status()
                        .unwrap()
                        .success()
                );
                q["draft"] = json!(copied);
            }
            "attributes" => {
                fs::write(f.a.join(".gitattributes"), "* merge=custom\n").unwrap();
                git(&f.a, &["add", ".gitattributes"]);
                git(&f.a, &["commit", "-qm", "unsafe attributes"]);
                q = f.request(&f.a);
            }
            "checked_out_main" => {
                git(&f.repo, &["checkout", "-q", "main"]);
                q = json!({"repo":f.repo,"accepted_ref":"refs/heads/main"});
            }
            "symbolic_alias" => {
                git(
                    &f.repo,
                    &["symbolic-ref", "refs/heads/alias", "refs/heads/main"],
                );
                q = json!({"repo":f.repo,"accepted_ref":"refs/heads/alias"});
            }
            _ => unreachable!(),
        }
        let tip = git(&f.repo, &["rev-parse", "main"]);
        let reader_head = git(&f.repo, &["rev-parse", "HEAD"]);
        let reader_status = git(&f.repo, &["status", "--porcelain"]);
        let reader_bytes = fs::read(f.repo.join(format!("nodes/{}/node.md", id(2)))).unwrap();
        let a = fingerprint(&f.a);
        let b = fingerprint(&f.b);
        let other = fingerprint(&foreign.a);
        let copy = copied.exists().then(|| fingerprint(&copied));
        let mut child = Command::new(bin_dir.join("git-submit-observe"))
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
        let out = child.wait_with_output().unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        assert_eq!(out.status.code(), Some(1), "{case}: {text}");
        assert!(
            text.lines().any(|l| l == "status:rejected"),
            "{case}: {text}"
        );
        assert!(
            text.lines().any(|l| l == "check:unsafe_submission"),
            "{case}: {text}"
        );
        assert!(text.lines().any(|l| l == "published:F"), "{case}: {text}");
        assert!(
            text.lines()
                .any(|l| l == "rule:e8234ea5-3467-4f2b-8ca2-2bcc5cc80d11"),
            "{case}: {text}"
        );
        assert!(text.lines().any(|l| l == "rows[0]:"), "{case}: {text}");
        assert!(
            !text.contains(f._dir.path().to_str().unwrap()),
            "private fixture path: {text}"
        );
        assert!(
            !text.contains(foreign._dir.path().to_str().unwrap()),
            "private foreign path: {text}"
        );
        assert_eq!(git(&f.repo, &["rev-parse", "main"]), tip, "{case}");
        assert_eq!(git(&f.repo, &["rev-parse", "HEAD"]), reader_head);
        assert_eq!(git(&f.repo, &["status", "--porcelain"]), reader_status);
        assert_eq!(
            fs::read(f.repo.join(format!("nodes/{}/node.md", id(2)))).unwrap(),
            reader_bytes
        );
        assert_eq!(fingerprint(&f.a), a, "{case}: draft A");
        assert_eq!(fingerprint(&f.b), b, "{case}: draft B");
        assert_eq!(fingerprint(&foreign.a), other, "{case}: foreign");
        assert_eq!(
            copied.exists().then(|| fingerprint(&copied)),
            copy,
            "{case}: copy"
        );
        if let Some(p) = &evidence {
            fs::write(p.join(format!("{case}.json")), serde_json::to_vec_pretty(&json!({"request":q,"stdout":text,"stderr":String::from_utf8_lossy(&out.stderr),"code":out.status.code(),"accepted":tip})).unwrap()).unwrap();
        }
        // Restore only the intentional unsafe input, then prove real native recovery.
        match case {
            "dirty" => {
                git(&f.a, &["restore", "nodes"]);
            }
            "untracked" => fs::remove_file(f.a.join("unknown")).unwrap(),
            "attributes" => {
                git(&f.a, &["revert", "--no-edit", "HEAD"]);
            }
            "checked_out_main" => {
                git(&f.repo, &["checkout", "--detach", "-q"]);
            }
            _ => (),
        }
        let mut child = Command::new(bin_dir.join("git-submit-observe"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let recovery = json!({"repo":f.repo,"accepted_ref":"refs/heads/main"});
        child
            .stdin
            .take()
            .unwrap()
            .write_all(recovery.to_string().as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "{case}: {:?}", out);
        let recovered: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(recovered["status"], "current");
        assert_eq!(recovered["candidate"], tip);
        assert_eq!(git(&f.repo, &["rev-parse", "main"]), tip);
    }
}
