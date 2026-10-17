use super::*;
use std::{
    collections::BTreeMap,
    io::Write,
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
pub(super) fn fingerprint(p: &Path) -> (BTreeMap<PathBuf, Vec<u8>>, Vec<String>, Vec<u8>) {
    fn walk(root: &Path, p: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(p).unwrap() {
            let path = entry.unwrap().path();
            let rel = path.strip_prefix(root).unwrap();
            if rel == Path::new(".git") {
                continue;
            }
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.is_symlink() {
                files.insert(
                    rel.into(),
                    format!("link:{}", fs::read_link(&path).unwrap().display()).into_bytes(),
                );
            } else if meta.is_dir() {
                walk(root, &path, files);
            } else {
                files.insert(rel.into(), fs::read(path).unwrap());
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(p, p, &mut files);
    let state = [
        vec!["rev-parse", "HEAD"],
        vec!["rev-parse", "--abbrev-ref", "HEAD"],
        vec!["status", "--porcelain"],
    ]
    .into_iter()
    .map(|args| git(p, &args))
    .collect();
    let index = fs::read(git(
        p,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    ))
    .unwrap();
    (files, state, index)
}

#[test]
fn native_pre_cas_candidate_cache_ref_and_contender_races() {
    let temp = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("PRECAS_PUBLICATION_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("receipts"));
    fs::create_dir(&evidence).unwrap();
    let bin = crate::native_bin("git-submit-observe");
    for action in ["candidate", "cache", "ref", "contention"] {
        let f = Fixture::new();
        node(&f.a, 2, "Race original X");
        let x = commit(&f.a);
        node(&f.b, 3, "Contender Y");
        let y = commit(&f.b);
        let a_before = fingerprint(&f.a);
        let b_before = fingerprint(&f.b);
        let reader_before = fingerprint(&f.repo);
        let ready = evidence.join(format!("{action}.ready"));
        let release = evidence.join(format!("{action}.release"));
        let mut q = f.request(&f.a);
        q["pause_before_publish"] = json!({"ready":ready,"release":release});
        let mut running = Running(Some(
            Command::new(&bin)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("build native observer before testing"),
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
        let view = PathBuf::from(fs::read_to_string(&ready).unwrap());
        assert_eq!(git(&f.repo, &["rev-parse", "main"]), f.base);
        let tip = match action {
            "candidate" => {
                let file = view.join(format!("nodes/{}/node.md", id(2)));
                let bytes = fs::read(&file).unwrap();
                let replacement = file.with_extension("replacement");
                fs::write(&replacement, &bytes).unwrap();
                fs::rename(replacement, &file).unwrap();
                assert_eq!(fs::read(file).unwrap(), bytes); // Same bytes, different physical identity.
                f.base.clone()
            }
            "cache" => {
                fs::write(view.join(".sifttext/index.sqlite"), [0; 64]).unwrap();
                f.base.clone()
            }
            "ref" => {
                git(&f.repo, &["update-ref", "refs/heads/main", &x, &f.base]);
                x.clone()
            }
            "contention" => main_donors::native("git-submit-observe", f.request(&f.b))["candidate"]
                .as_str()
                .unwrap()
                .into(),
            _ => unreachable!(),
        };
        fs::write(&release, "go").unwrap();
        let out = running.0.take().unwrap().wait_with_output().unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        assert_eq!(out.status.code(), Some(1), "{action}: {text}");
        // Exact scalar lines in pinned LEAN, not JSON or a substring verdict.
        let check = if action == "cache" {
            // Derived damage is detected by inspection after retained validation.
            "check:candidate_recheck_rejected"
        } else {
            "check:source_or_ref_changed"
        };
        for scalar in [
            check,
            "status:rejected",
            "published:F",
            "rule:e8234ea5-3467-4f2b-8ca2-2bcc5cc80d11",
            "rows[0]:",
        ] {
            assert_eq!(
                text.lines().filter(|line| *line == scalar).count(),
                1,
                "{action}: {text}"
            );
        }
        assert!(
            !text.contains(f._dir.path().to_str().unwrap()),
            "private path in refusal"
        );
        assert_eq!(git(&f.repo, &["rev-parse", "main"]), tip);
        assert_eq!(fingerprint(&f.a), a_before);
        assert_eq!(fingerprint(&f.b), b_before);
        assert_eq!(fingerprint(&f.repo), reader_before);
        // Retry computes against actual accepted history, not the refused detached view.
        let recovered = main_donors::native("git-submit-observe", f.request(&f.a));
        assert_eq!(recovered["status"], "current");
        let candidate = recovered["candidate"].as_str().unwrap();
        assert_eq!(git(&f.repo, &["rev-parse", "main"]), candidate);
        let accepted_view = Path::new(recovered["view"].as_str().unwrap());
        assert_eq!(git(accepted_view, &["rev-parse", "HEAD"]), candidate);
        assert_eq!(
            git(accepted_view, &["rev-parse", "HEAD^{tree}"]),
            recovered["tree"].as_str().unwrap()
        );
        if action == "ref" {
            assert_eq!(candidate, tip);
        }
        let indexed = main_donors::native(
            "core-observe",
            json!({"op":"index","store":recovered["view"],"mode":"incremental"}),
        );
        let outline = main_donors::native(
            "core-observe",
            json!({"op":"outline_inspect","store":recovered["view"]}),
        );
        for response in [&indexed, &outline] {
            assert_eq!(response["status"], "current");
            assert_eq!(response["version"], recovered["version"]);
        }
        let rows = indexed["nodes"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        for (n, body) in [
            (2, "Race original X"),
            (
                3,
                if action == "contention" {
                    "Contender Y"
                } else {
                    "Body"
                },
            ),
        ] {
            assert_eq!(
                rows.iter().find(|r| r["id"] == id(n)).unwrap()["content"],
                body
            );
        }
        assert_eq!(git(&f.a, &["rev-parse", "HEAD"]), x);
        assert_eq!(git(&f.b, &["rev-parse", "HEAD"]), y);
        assert_eq!(fingerprint(&f.a), a_before);
        assert_eq!(fingerprint(&f.b), b_before);
        assert_eq!(fingerprint(&f.repo), reader_before);
        fs::write(evidence.join(format!("{action}.json")), serde_json::to_vec_pretty(&json!({"request":q,"exit":out.status.code(),"stdout":text,"stderr":String::from_utf8_lossy(&out.stderr),"winner":tip,"recovered":recovered,"index":indexed,"outline":outline})).unwrap()).unwrap();
    }
}
