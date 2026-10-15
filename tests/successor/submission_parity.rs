use super::{Fixture, add, commit, git, id, node, unsafe_inputs::fingerprint};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

// Independent scalar-only pinned LEAN check, matching the legacy per-row rule predicate.
pub(super) fn assert_rule_rows(text: &str) {
    let mut lines = text.lines().skip_while(|line| !line.starts_with("rows["));
    let (count, keys) = lines
        .next()
        .unwrap()
        .strip_prefix("rows[")
        .unwrap()
        .split_once("]:")
        .unwrap();
    let count = count.parse::<usize>().unwrap();
    if count == 0 {
        assert!(keys.is_empty(), "{text}");
        assert!(lines.next().is_none(), "{text}");
        return;
    }
    let keys: Vec<_> = keys.split('\t').collect();
    let rule = keys.iter().position(|key| *key == "rule").unwrap();
    for _ in 0..count {
        let row: Vec<_> = lines
            .next()
            .unwrap()
            .strip_prefix("  ")
            .unwrap()
            .split('\t')
            .collect();
        assert_eq!(row.len(), keys.len(), "{text}");
        assert!(
            row[rule].len() == 36
                && row[rule]
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || b == b'-'),
            "{text}"
        );
    }
    assert!(lines.next().is_none(), "{text}");
}

#[test]
fn refusal_rule_check_inspects_every_row() {
    let valid = "rows[2]:check\trule\n  first\te8234ea5-3467-4f2b-8ca2-2bcc5cc80d11\n  second\t302b4549-ef27-4ab7-a9a7-7f240ffe058e\n";
    assert_rule_rows(valid);
    assert_rule_rows("rows[0]:\n");
    for invalid in [
        valid.replace("302b4549-ef27-4ab7-a9a7-7f240ffe058e", "invalid"),
        valid.replace(
            "302b4549-ef27-4ab7-a9a7-7f240ffe058e",
            "302b4549-ef27-4ab7-a9a7-7f240ffe058G",
        ),
        valid.replace("rows[2]", "rows[3]"),
        valid.replace("rows[2]", "rows[1]"),
        valid.replace("second\t", "second\textra\t"),
    ] {
        assert!(std::panic::catch_unwind(|| assert_rule_rows(&invalid)).is_err());
    }
}

fn call(binary: &str, q: Value, pass: bool) -> Value {
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug"));
    let mut child = Command::new(bin.join(binary))
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
    assert_eq!(
        out.status.code(),
        Some(if pass { 0 } else { 1 }),
        "{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    if let Some(e) = std::env::var_os("SUBMISSION_PARITY_EVIDENCE") {
        let e = PathBuf::from(e);
        fs::create_dir_all(&e).unwrap();
        let n = fs::read_dir(&e).unwrap().count();
        fs::write(e.join(format!("call-{n}.json")), serde_json::to_vec_pretty(&json!({"request":q,"stdout":text,"stderr":String::from_utf8_lossy(&out.stderr),"code":out.status.code()})).unwrap()).unwrap();
    }
    if pass {
        serde_json::from_str(&text).unwrap()
    } else {
        assert!(text.lines().any(|l| l == "status:rejected"));
        // Full-law failures emit validation reports rather than publication receipts.
        if text.lines().any(|l| l.starts_with("published:")) {
            assert!(text.lines().any(|l| l == "published:F"));
        }
        assert!(text.contains("e8234ea5-3467-4f2b-8ca2-2bcc5cc80d11"));
        assert_rule_rows(&text);
        assert!(!text.contains(q["repo"].as_str().unwrap()));
        assert!(!text.contains(q["draft"].as_str().unwrap()));
        json!(text)
    }
}
fn submit(f: &Fixture, p: &Path) -> Value {
    call("git-submit-observe", f.request(p), true)
}
fn view(r: &Value) -> Value {
    let p = Path::new(r["view"].as_str().unwrap());
    assert_eq!(
        git(p, &["rev-parse", "HEAD"]),
        r["candidate"].as_str().unwrap()
    );
    assert_eq!(
        git(p, &["rev-parse", "HEAD^{tree}"]),
        r["tree"].as_str().unwrap()
    );
    let indexed = call(
        "core-observe",
        json!({"op":"index","store":p,"mode":"incremental"}),
        true,
    );
    assert_eq!(indexed["status"], "current");
    assert_eq!(indexed["version"], r["version"]);
    assert_eq!(
        call(
            "core-observe",
            json!({"op":"outline_inspect","store":p}),
            true
        )["version"],
        r["version"]
    );
    indexed
}
fn row(v: &Value, n: usize) -> &Value {
    v["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id(n))
        .unwrap()
}
fn outline(r: &Value, ids: &[usize], descendants: usize) {
    let text = fs::read_to_string(
        Path::new(r["view"].as_str().unwrap()).join(format!("nodes/{}/outline.full.lean", id(1))),
    )
    .unwrap();
    let mut lines = text.lines().skip_while(|l| !l.starts_with("rows["));
    let header = lines.next().unwrap();
    let (count, keys) = header
        .strip_prefix("rows[")
        .unwrap()
        .split_once("]:")
        .unwrap();
    assert_eq!(count.parse::<usize>().unwrap(), ids.len());
    let keys: Vec<_> = keys.split('\t').collect();
    let i = keys.iter().position(|k| *k == "id").unwrap();
    let d = keys.iter().position(|k| *k == "descendants").unwrap();
    let rows: Vec<Vec<_>> = lines
        .take(ids.len())
        .map(|l| l.strip_prefix("  ").unwrap().split('\t').collect())
        .collect();
    assert_eq!(
        rows.iter().map(|r| r[i]).collect::<Vec<_>>(),
        ids.iter().map(|n| id(*n)).collect::<Vec<_>>()
    );
    assert_eq!(rows[0][d].parse::<usize>().unwrap(), descendants);
}
fn insert(p: &Path, n: usize, parent: usize) {
    add(p, n);
    std::os::unix::fs::symlink(
        format!("../../{}", id(n)),
        p.join(format!("nodes/{}/children/{}", id(parent), id(n))),
    )
    .unwrap();
    let order = p.join(format!("nodes/{}/children.order", id(parent)));
    fs::write(&order, fs::read_to_string(&order).unwrap() + &id(n) + "\n").unwrap();
}
#[test]
fn native_move_with_older_ancestor_edit_preserves_both_drafts() {
    let f = Fixture::new();
    insert(&f.a, 4, 2);
    commit(&f.a);
    submit(&f, &f.a);
    let moved = f._dir.path().join("moved");
    git(
        &f.repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "moved",
            moved.to_str().unwrap(),
            "main",
        ],
    );
    fs::remove_file(moved.join(format!("nodes/{}/children/{}", id(2), id(4)))).unwrap();
    fs::write(moved.join(format!("nodes/{}/children.order", id(2))), "").unwrap();
    std::os::unix::fs::symlink(
        format!("../../{}", id(4)),
        moved.join(format!("nodes/{}/children/{}", id(3), id(4))),
    )
    .unwrap();
    fs::write(
        moved.join(format!("nodes/{}/children.order", id(3))),
        id(4) + "\n",
    )
    .unwrap();
    commit(&moved);
    node(&f.b, 1, "Unrelated ancestor body");
    commit(&f.b);
    let m = fingerprint(&moved);
    let b = fingerprint(&f.b);
    submit(&f, &f.b);
    let r = submit(&f, &moved);
    let v = view(&r);
    assert_eq!(row(&v, 2)["children"], json!([]));
    assert_eq!(row(&v, 3)["children"], json!([id(4)]));
    assert_eq!(row(&v, 1)["content"], "Unrelated ancestor body");
    outline(&r, &[1, 2, 3, 4], 3);
    assert_eq!(fingerprint(&moved), m);
    assert_eq!(fingerprint(&f.b), b);
}
#[test]
fn native_delete_reference_and_same_uuid_refusals_preserve_both_drafts() {
    for conflict in [false, true] {
        let f = Fixture::new();
        if conflict {
            node(&f.a, 2, "Conflicting A");
            node(&f.b, 2, "Conflicting B");
        } else {
            fs::remove_dir_all(f.a.join(format!("nodes/{}", id(2)))).unwrap();
            fs::remove_file(f.a.join(format!("nodes/{}/children/{}", id(1), id(2)))).unwrap();
            fs::write(
                f.a.join(format!("nodes/{}/children.order", id(1))),
                id(3) + "\n",
            )
            .unwrap();
            let file = f.b.join(format!("nodes/{}/node.md", id(3)));
            fs::write(
                &file,
                fs::read_to_string(&file).unwrap().replace(
                    "see_also: []",
                    &format!(
                        "see_also: {}",
                        json!([{"target":id(2),"reason":"Uses this knowledge"}])
                    ),
                ),
            )
            .unwrap();
        }
        commit(&f.a);
        commit(&f.b);
        let r = submit(&f, &f.b);
        view(&r);
        let tip = git(&f.repo, &["rev-parse", "main"]);
        let a = fingerprint(&f.a);
        let b = fingerprint(&f.b);
        let r = call("git-submit-observe", f.request(&f.a), false);
        let text = r.as_str().unwrap();
        if conflict {
            assert!(text.lines().any(|l| l == "check:git_file_conflict"));
        } else {
            let exact = format!(
                "  dangling_reference\t{}\tlaws/Spec.bend:refs_valid\te8234ea5-3467-4f2b-8ca2-2bcc5cc80d11\t{}\t{}\t_",
                id(3),
                id(3),
                id(2)
            );
            assert!(text.lines().any(|l| l == exact), "{text}");
        }
        assert_eq!(git(&f.repo, &["rev-parse", "main"]), tip);
        assert_eq!(fingerprint(&f.a), a);
        assert_eq!(fingerprint(&f.b), b);
    }
}
