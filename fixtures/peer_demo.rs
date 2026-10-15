//! Checked CLI/Git demo after the independent declarative fixture precheck.
//! Compile as a test; SIFTTEXT_PROJECT selects the checked source/binary pair.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

fn run(command: &Path, args: &[&str], reject: bool) -> String {
    let out = Command::new(command).args(args).output().unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    if reject {
        assert_eq!(out.status.code(), Some(1), "{command:?} {args:?}: {stderr}");
        assert!(stderr.starts_with("rejected: "), "{stderr}");
    } else {
        assert!(out.status.success(), "{command:?} {args:?}: {stderr}");
    }
    String::from_utf8(out.stdout).unwrap()
}
#[cfg(test)]
fn git(store: &Path, args: &[&str]) -> String {
    let mut all = vec!["-C", store.to_str().unwrap()];
    all.extend_from_slice(args);
    run(Path::new("git"), &all, false)
}
#[cfg(test)]
fn load(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn node_path(store: &Path, id: &str, file: &str) -> PathBuf {
    store.join("nodes").join(id).join(file)
}
fn order(children: &[&str]) -> String {
    children.iter().map(|id| format!("{id}\n")).collect()
}
pub fn materialize(store: &Path, fixture: &Value) {
    fs::create_dir(store).unwrap(); // Exclusive, never repair or follow an existing store.
    fs::create_dir(store.join("trees")).unwrap();
    for n in fixture["nodes"].as_array().unwrap() {
        let id = n["id"].as_str().unwrap();
        fs::create_dir_all(node_path(store, id, "children")).unwrap();
        fs::write(node_path(store, id, "children/.keep"), "").unwrap();
        let text = format!(
            "---\nid: {}\nname: {}\nmicro_summary: {}\nsee_also: {}\n---\n{}",
            n["id"],
            n["name"],
            n["micro_summary"],
            n["see_also"],
            n["content"].as_str().unwrap()
        );
        fs::write(node_path(store, id, "node.md"), text).unwrap();
        let children: Vec<_> = n["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        fs::write(node_path(store, id, "children.order"), order(&children)).unwrap();
        for child in children {
            std::os::unix::fs::symlink(
                format!("../../{child}"),
                node_path(store, id, &format!("children/{child}")),
            )
            .unwrap();
        }
    }
    for root in fixture["roots"].as_array().unwrap() {
        let id = root.as_str().unwrap();
        std::os::unix::fs::symlink(format!("../nodes/{id}"), store.join("trees").join(id)).unwrap();
    }
    for sentinel in fixture["sentinels"].as_array().unwrap() {
        let path = sentinel["path"].as_str().unwrap();
        assert!(
            !path.contains('\\')
                && !path.starts_with('/')
                && path.split('/').all(|s| !matches!(s, "" | "." | ".."))
        );
        assert!(!matches!(
            path.split('/').next().unwrap(),
            "nodes" | "trees" | ".git"
        ));
        let dest = store.join(path);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dest)
            .unwrap()
            .write_all(sentinel["content"].as_str().unwrap().as_bytes())
            .unwrap();
    }
}
fn keyed(nodes: &Value) -> BTreeMap<String, Value> {
    nodes
        .as_array()
        .unwrap()
        .iter()
        .map(|n| (n["id"].as_str().unwrap().to_owned(), n.clone()))
        .collect()
}
pub fn check(bin: &Path, store: &Path, want: &Value) -> Value {
    let canonical = store.canonicalize().unwrap();
    let store = canonical.as_path();
    let got: Value =
        serde_json::from_str(&run(bin, &["inspect", store.to_str().unwrap()], false)).unwrap();
    let mut authored = got["nodes"].clone();
    for n in authored.as_array_mut().unwrap() {
        assert_eq!(
            n["path"],
            json!(node_path(store, n["id"].as_str().unwrap(), "node.md"))
        );
        n.as_object_mut().unwrap().remove("path");
        n.as_object_mut().unwrap().remove("navigation");
    }
    assert_eq!(
        authored.as_array().unwrap().len(),
        want["nodes"].as_array().unwrap().len()
    );
    assert_eq!(keyed(&authored), keyed(&want["nodes"]));
    assert_eq!(got["roots"], want["roots"]);
    let nodes = keyed(&authored);
    let mut parents: BTreeMap<String, Value> =
        nodes.keys().map(|id| (id.clone(), json!("none"))).collect();
    for n in got["nodes"].as_array().unwrap() {
        for child in n["children"].as_array().unwrap() {
            parents.insert(child.as_str().unwrap().into(), n["id"].clone());
        }
        let navigation: Vec<_> = n["see_also"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let mut r = r.clone();
                let target = &nodes[r["target"].as_str().unwrap()];
                r["name"] = target["name"].clone();
                r["micro_summary"] = target["micro_summary"].clone();
                r["path"] = json!(node_path(store, r["target"].as_str().unwrap(), "node.md"));
                r
            })
            .collect();
        assert_eq!(n["navigation"], json!(navigation));
    }
    assert_eq!(json!(parents), want["derived_parents"]);
    for s in want["sentinels"].as_array().unwrap() {
        assert_eq!(
            fs::read(store.join(s["path"].as_str().unwrap())).unwrap(),
            s["content"].as_str().unwrap().as_bytes()
        );
    }
    got
}
#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
enum Entry {
    Directory,
    Link(PathBuf),
    File(Vec<u8>),
}
#[cfg(test)]
fn inventory(store: &Path) -> BTreeMap<PathBuf, Entry> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Entry>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(root).unwrap().to_owned();
            if relative == Path::new(".git") {
                continue;
            }
            let m = fs::symlink_metadata(&path).unwrap();
            let value = if m.file_type().is_symlink() {
                Entry::Link(fs::read_link(&path).unwrap())
            } else if m.is_dir() {
                walk(root, &path, out);
                Entry::Directory
            } else {
                Entry::File(fs::read(&path).unwrap())
            };
            out.insert(relative, value);
        }
    }
    let mut out = BTreeMap::new();
    walk(store, store, &mut out);
    out
}
#[cfg(test)]
fn init(store: &Path) {
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.name", "Synthetic Fixture"],
        vec!["config", "user.email", "fixture@example.invalid"],
        vec!["add", "."],
        vec!["commit", "-qm", "fixture: retain original graph"],
    ] {
        git(store, &args);
    }
}
#[test]
fn checked_cli_helper_native_bytes_refusals_and_git_revert() {
    use std::collections::BTreeSet;

    let project = std::env::var_os("SIFTTEXT_PROJECT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    crate::fixture_contract::check_fixture(&project);
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| project.join("target/debug"))
        .join("sifttext-file-graph");
    let fixture = load(&project.join("fixtures/fixture-contract.json"));
    let expected = |name: &str| load(&project.join(format!("fixtures/expected/{name}.json")));
    let label = |name: &str| fixture["labels"][name].as_str().unwrap();
    let (r, a, b, c, d) = (label("R"), label("A"), label("B"), label("C"), label("D"));
    let temporary = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("SIFTTEXT_EVIDENCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().to_owned());
    fs::create_dir_all(&evidence).unwrap();
    let dir = tempfile::Builder::new()
        .prefix("rust-peer-demo-")
        .tempdir_in(&evidence)
        .unwrap()
        .keep();
    let before = dir.join("before");
    materialize(&before, &fixture);
    check(&bin, &before, &expected("before"));
    let original = inventory(&before);
    for (name, kind, arg, changed) in [
        (
            "move-b-under-a",
            "move",
            a.to_owned(),
            vec![
                format!("nodes/{r}/children/{b}"),
                format!("nodes/{a}/children/{b}"),
                format!("nodes/{r}/children.order"),
                format!("nodes/{a}/children.order"),
            ],
        ),
        (
            "reorder-b-after-c",
            "reorder",
            c.to_owned(),
            vec![format!("nodes/{r}/children.order")],
        ),
        (
            "rename-b",
            "rename",
            expected("rename-b")["transition"]["new_display_name"]
                .as_str()
                .unwrap()
                .to_owned(),
            vec![format!("nodes/{b}/node.md")],
        ),
    ] {
        let helper = dir.join(name);
        let native = dir.join(format!("native-{name}"));
        for store in [&helper, &native] {
            materialize(store, &fixture);
            init(store);
        }
        run(&bin, &[kind, helper.to_str().unwrap(), b, &arg], false);
        match kind {
            "move" => {
                fs::rename(
                    node_path(&native, r, &format!("children/{b}")),
                    node_path(&native, a, &format!("children/{b}")),
                )
                .unwrap();
                fs::write(node_path(&native, r, "children.order"), order(&[a, c])).unwrap();
                fs::write(node_path(&native, a, "children.order"), order(&[b])).unwrap();
            }
            "reorder" => {
                fs::write(node_path(&native, r, "children.order"), order(&[a, c, b])).unwrap();
            }
            "rename" => {
                let file = node_path(&native, b, "node.md");
                let text = fs::read_to_string(&file).unwrap();
                let old = format!(
                    "name: {}",
                    fixture["nodes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|n| n["id"] == b)
                        .unwrap()["name"]
                );
                assert_eq!(text.matches(&old).count(), 1);
                fs::write(
                    file,
                    text.replacen(&old, &format!("name: {}", json!(arg)), 1),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        for store in [&helper, &native] {
            run(
                &bin,
                &[
                    "validate-draft",
                    before.to_str().unwrap(),
                    store.to_str().unwrap(),
                    kind,
                    b,
                    &arg,
                ],
                false,
            );
            let view = check(&bin, store, &expected(name));
            let after = inventory(store);
            assert_eq!(
                after,
                inventory(if store == &helper { &native } else { &helper })
            );
            let paths: BTreeSet<_> = original.keys().chain(after.keys()).collect();
            let actual: BTreeSet<_> = paths
                .into_iter()
                .filter(|p| original.get(*p) != after.get(*p))
                .cloned()
                .collect();
            assert_eq!(actual, changed.iter().map(PathBuf::from).collect());
            for id in [b, d] {
                assert_eq!(
                    after[&PathBuf::from(format!("nodes/{id}"))],
                    original[&PathBuf::from(format!("nodes/{id}"))]
                );
            }
            git(store, &["add", "."]);
            let tag = if store == &helper {
                name.to_owned()
            } else {
                format!("native-{name}")
            };
            fs::write(
                dir.join(format!("{tag}.diff")),
                git(
                    store,
                    &["diff", "--cached", "--no-ext-diff", "--no-renames"],
                ),
            )
            .unwrap();
            fs::write(
                dir.join(format!("{tag}.json")),
                serde_json::to_vec_pretty(&view).unwrap(),
            )
            .unwrap();
        }
        assert_eq!(
            fs::read(dir.join(format!("{name}.diff"))).unwrap(),
            fs::read(dir.join(format!("native-{name}.diff"))).unwrap()
        );
        git(
            &helper,
            &["commit", "-qm", &format!("fixture: checked {kind}")],
        );
        git(&helper, &["revert", "--no-edit", "HEAD"]);
        assert_eq!(inventory(&helper), original);
        check(&bin, &helper, &expected("before"));
        assert!(git(&helper, &["status", "--porcelain"]).is_empty());
        fs::write(
            dir.join(format!("{name}-history.log")),
            git(&helper, &["log", "--oneline"]),
        )
        .unwrap();
    }
    for (node, parent) in [(b, d), (r, a)] {
        run(
            &bin,
            &["move", before.to_str().unwrap(), node, parent],
            true,
        );
        assert_eq!(inventory(&before), original);
    }
    for mode in ["duplicate-parent", "missing-order", "tampered-sentinel"] {
        let invalid = dir.join(mode);
        materialize(&invalid, &fixture);
        match mode {
            "duplicate-parent" => {
                std::os::unix::fs::symlink(
                    format!("../../{b}"),
                    node_path(&invalid, a, &format!("children/{b}")),
                )
                .unwrap();
                fs::write(node_path(&invalid, a, "children.order"), order(&[b])).unwrap();
            }
            "missing-order" => {
                fs::write(node_path(&invalid, r, "children.order"), order(&[a, c])).unwrap();
            }
            "tampered-sentinel" => {
                // Match the legacy control: an otherwise valid ordinary move.
                fs::rename(
                    node_path(&invalid, r, &format!("children/{b}")),
                    node_path(&invalid, a, &format!("children/{b}")),
                )
                .unwrap();
                fs::write(node_path(&invalid, r, "children.order"), order(&[a, c])).unwrap();
                fs::write(node_path(&invalid, a, "children.order"), order(&[b])).unwrap();
                let file = invalid.join(fixture["sentinels"][0]["path"].as_str().unwrap());
                fs::OpenOptions::new()
                    .append(true)
                    .open(file)
                    .unwrap()
                    .write_all(b"tampered\n")
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let retained = inventory(&invalid);
        run(
            &bin,
            &[
                "validate-draft",
                before.to_str().unwrap(),
                invalid.to_str().unwrap(),
                "move",
                b,
                a,
            ],
            true,
        );
        assert_eq!(inventory(&invalid), retained);
    }
    fs::write(
        dir.join("candidate.txt"),
        git(&project, &["rev-parse", "HEAD"]),
    )
    .unwrap();
    println!("PASS checked peer fixture: {}", dir.display());
}
