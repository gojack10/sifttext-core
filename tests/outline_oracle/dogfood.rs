// Dogfood-local's two-tree Git/publication/recovery slice. The sibling Rust
// modules preserve its search, graph-inspect and forged-manifest controls.
fn dogfood_nodes() -> Vec<Value> {
    let ident = |i| format!("7a000000-0000-4000-8000-{i:012}");
    [(1,"Knowledge root","in_progress",vec![2,3]),
     (2,"Alpha branch","in_progress",vec![5]),
     (3,"Beta leaf","deferred",vec![]),
     (4,"Gamma leaf","resolved",vec![]),
     (5,"Delta leaf","not_examined",vec![]),
     (6,"Second root","in_progress",vec![4])]
    .into_iter().map(|(i,name,status,cs)| {
        let mut n=json!({"id":ident(i),"name":name,"micro_summary":format!("Synthetic {name}."),
            "status":status,"scope":format!("Synthetic {name} scope."),
            "content":format!("{name} prose about knowledge work.\n"),
            "children":cs.into_iter().map(ident).collect::<Vec<_>>(),"see_also":[]});
        if i==3 { n["deferred_reason"]=json!("Wait; resume after the synthetic check."); }
        if i==4 || i==5 { n["see_also"]=json!([{"target":ident(if i==4 {1} else {4}),"reason":"Read the linked synthetic node."}]); }
        n
    }).collect()
}
fn dogfood_git(root: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    command.arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Local Dogfood",
            "-c",
            "user.email=dogfood@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args);
    let out = capacity::observer_output(command, vec![]);
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn dogfood_graph_output(args: &[&Path]) -> std::process::Output {
    let bin = std::env::var_os("CORE_GRAPH_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/sifttext-file-graph")
        });
    assert!(
        bin.is_file(),
        "build sifttext-file-graph first: {}",
        bin.display()
    );
    let mut command = Command::new(bin);
    command.args(args);
    capacity::observer_output(command, vec![])
}
fn dogfood_graph(args: &[&Path], success: bool) -> String {
    let out = dogfood_graph_output(args);
    assert_eq!(
        out.status.success(),
        success,
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(if success { out.stdout } else { out.stderr }).unwrap();
    assert!(text.contains(if success {
        "valid private draft"
    } else {
        "rejected:"
    }));
    text
}
fn dogfood_search(store: &Path, query: &str, under: Option<&str>) -> Value {
    let bin = std::env::var_os("CORE_OBSERVE_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/core-observe"));
    let command = Command::new(bin);
    let ids = authored_records(store)
        .iter()
        .map(|n| n["id"].clone())
        .collect::<Vec<_>>();
    let out = capacity::observer_output(
        command,
        json!({"op":"search","store":store,"query":query,
        "under_node_id":under,"limit":6,"offset":0,"readable_ids":ids})
        .to_string()
        .into_bytes(),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "current");
    assert_eq!(v["other_groups"], json!([]));
    v
}
#[test]
fn dogfood_two_roots_checked_git_publication_rejections_and_recovery() {
    let bpe = ORACLE_BPE.clone();
    let temp = TempDir::new().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let store = root.join("knowledge");
    let draft = root.join("draft");
    let nodes = dogfood_nodes();
    materialize(&store, &nodes);
    // Git does not preserve empty directories; retain the authored leaf surface.
    for n in nodes.iter().filter(|n| children(n).is_empty()) {
        fs::write(
            store
                .join("nodes")
                .join(n["id"].as_str().unwrap())
                .join("children/.keep"),
            "",
        )
        .unwrap();
    }
    fs::write(store.join(".gitignore"),".sifttext/\nnodes/*/outline.lean\nnodes/*/outline.full.lean\nnodes/*/outline.lean.meta.json\nnodes/*/outline.full.lean.meta.json\n").unwrap();
    dogfood_graph(&[Path::new("validate"), &store], true);
    eprintln!("dogfood: baseline full index");
    let base_version = refresh(&store, "full");
    check_native(&store, &base_version, &bpe);
    assert!(versions(&store, "planted-global-v9").is_err());
    let authored = authored_records(&store);
    assert_eq!(authored.len(), 6);
    let alpha = authored
        .iter()
        .find(|n| n["name"] == "Alpha branch")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let gamma = authored.iter().find(|n| n["name"] == "Gamma leaf").unwrap()["id"]
        .as_str()
        .unwrap();
    let first = authored
        .iter()
        .find(|n| n["name"] == "Knowledge root")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let second = authored
        .iter()
        .find(|n| n["name"] == "Second root")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    assert_eq!(
        dogfood_search(&store, "gamma", Some(first))["results"],
        json!([])
    );
    assert_eq!(
        dogfood_search(&store, "gamma", Some(second))["results"][0]["id"],
        gamma
    );
    let baseline = authored_snapshot(&store);
    let alpha_before = authored.iter().find(|n| n["id"] == alpha).unwrap().clone();
    dogfood_git(&store, &["init", "-q", "-b", "main"]);
    dogfood_git(&store, &["add", "-A", "--", "nodes", "trees", ".gitignore"]);
    let tracked = dogfood_git(&store, &["diff", "--cached", "--name-only"]);
    assert!(!tracked.contains("outline") && !tracked.contains(".sifttext") && !tracked.contains(".meta.json"));
    dogfood_git(&store, &["commit", "-qm", "synthetic knowledge baseline"]);
    let base = dogfood_git(&store, &["rev-parse", "HEAD"]);
    dogfood_git(
        &store,
        &[
            "worktree",
            "add",
            "-q",
            draft.to_str().unwrap(),
            "-b",
            "dogfood/rename",
            "main",
        ],
    );
    eprintln!("dogfood: private worktree full index");
    assert_eq!(refresh(&draft, "full"), base_version);
    check_native(&draft, &base_version, &bpe);
    change_field(&draft, alpha, "name", json!("Alpha branch renamed"));
    dogfood_graph(&[Path::new("validate"), &draft], true);
    let checked_rename = || {
        dogfood_graph_output(&[
            Path::new("validate-draft"),
            &store,
            &draft,
            Path::new("rename"),
            Path::new(alpha),
            Path::new("Alpha branch renamed"),
        ])
    };
    let checked = checked_rename();
    assert!(checked.status.success());
    assert!(String::from_utf8_lossy(&checked.stdout).contains("exactly matches checked operation"));
    eprintln!("dogfood: renamed incremental index");
    let published = refresh(&draft, "incremental");
    assert_ne!(published, base_version);
    check_native(&draft, &published, &bpe);
    dogfood_git(&draft, &["add", "-A", "--", "nodes", "trees", ".gitignore"]);
    let target = format!("nodes/{alpha}/node.md");
    assert_eq!(
        dogfood_git(&draft, &["diff", "--cached", "--name-only"]),
        target
    );
    dogfood_git(&draft, &["commit", "-qm", "synthetic rename"]);
    let candidate = dogfood_git(&draft, &["rev-parse", "HEAD"]);
    assert_eq!(dogfood_git(&draft, &["status", "--porcelain"]), "");
    assert_eq!(dogfood_git(&store, &["rev-parse", "HEAD"]), base);
    let checked = checked_rename();
    assert!(checked.status.success());
    assert!(String::from_utf8_lossy(&checked.stdout).contains("exactly matches checked operation"));
    dogfood_git(&store, &["merge", "--ff-only", "dogfood/rename"]);
    assert_eq!(dogfood_git(&store, &["rev-parse", "HEAD"]), candidate);
    assert_eq!(refresh(&store, "incremental"), published);
    check_native(&store, &published, &bpe);
    assert_eq!(dogfood_git(&store, &["status", "--porcelain"]), "");
    assert_eq!(dogfood_search(&store, "alpha", None)["results"][0]["title"], "Alpha branch renamed");
    assert_eq!(dogfood_search(&store, "Gamma", None)["results"][0]["id"], gamma);
    assert_eq!(
        dogfood_search(&store, "renamed", Some(alpha))["results"][0]["id"],
        alpha
    );
    let committed = authored_snapshot(&store);
    let mut alpha_expected = alpha_before;
    alpha_expected["name"] = json!("Alpha branch renamed");
    assert_eq!(
        authored_records(&store).into_iter().find(|n| n["id"] == alpha).unwrap(),
        alpha_expected,
        "rename preserves Alpha UUID, body and every other authored field"
    );
    for (p, h) in &baseline {
        if p != Path::new(&target) {
            assert_eq!(committed[p], *h);
        }
    }
    let p = draft.join(&target);
    let original = fs::read_to_string(&p).unwrap();
    for case in ["blank-summary", "malformed-graph", "body-edit"] {
        let link = draft.join("nodes").join(first).join("children").join(alpha);
        match case {
            "blank-summary" => change_field(&draft, alpha, "micro_summary", json!("   ")),
            "malformed-graph" => fs::remove_file(&link).unwrap(),
            _ => fs::write(&p, original.replace("prose about", "edited prose about")).unwrap(),
        }
        let rejected_authored = authored_snapshot(&draft);
        dogfood_graph(&[Path::new("validate"), &draft], case == "body-edit");
        let refused = checked_rename();
        assert!(!refused.status.success(), "{case}");
        assert!(String::from_utf8_lossy(&refused.stderr).starts_with("rejected:"));
        if case == "body-edit" {
            let (ok, v) = native(&draft, "outline_inspect", None);
            assert!(!ok);
            assert_eq!(v["status"], "stale");
            assert_eq!(v["version"], Value::Null);
            assert_eq!(v["files"], json!([]));
            let body_view = dogfood_read_request(&draft, json!({"op":"index","mode":"incremental"}));
            let edited = body_view["version"].as_str().unwrap();
            assert_ne!(edited, published);
            let body = authored_records(&draft).into_iter().find(|n| n["id"] == alpha).unwrap()["content"].clone();
            assert_eq!(
                body_view["nodes"].as_array().unwrap().iter().find(|n| n["id"] == alpha).unwrap()["content"],
                body,
                "body-only refresh returns the edited authored body"
            );
            check_native(&draft, edited, &bpe);
        } else {
            for (op, mode) in [("index", Some("full")), ("outline_inspect", None)] {
                let (ok, v) = native(&draft, op, mode);
                assert!(!ok);
                assert_eq!(v["status"], "error");
                assert_eq!(v["version"], Value::Null);
                if op == "index" {
                    assert_eq!(v["nodes"], json!([]));
                    assert_eq!(v["links"], json!([]));
                } else {
                    assert_eq!(v["files"], json!([]));
                }
            }
        }
        assert_eq!(
            authored_snapshot(&draft),
            rejected_authored,
            "refusal must not repair authored bytes"
        );
        assert_eq!(dogfood_git(&store, &["rev-parse", "HEAD"]), candidate);
        assert_eq!(refresh(&store, "incremental"), published);
        assert_eq!(authored_snapshot(&store), committed);
        dogfood_git(&draft, &["restore", "--", "nodes"]);
        assert_eq!(dogfood_git(&draft, &["status", "--porcelain"]), "");
    }
    assert_eq!(dogfood_search(&store, "edited", None)["total_matches"], 0);
    dogfood_git(&store, &["revert", "--no-edit", &candidate]);
    let reverted = dogfood_git(&store, &["rev-parse", "HEAD"]);
    assert_ne!(reverted, candidate);
    assert_eq!(dogfood_git(&store, &["rev-parse", "HEAD~1"]), candidate);
    assert_eq!(refresh(&store, "incremental"), base_version);
    check_native(&store, &base_version, &bpe);
    assert_eq!(authored_snapshot(&store), baseline);
    assert_eq!(
        dogfood_search(&store, "renamed", None)["results"],
        json!([])
    );
    assert_eq!(
        dogfood_search(&store, "alpha", None)["results"][0]["title"],
        "Alpha branch"
    );
    dogfood_git(&store, &["revert", "--no-edit", &reverted]);
    assert_eq!(refresh(&store, "incremental"), published);
    check_native(&store, &published, &bpe);
    assert_eq!(authored_snapshot(&store), committed);
    assert_eq!(
        dogfood_search(&store, "renamed", None)["results"][0]["id"],
        alpha
    );
    assert_eq!(dogfood_git(&store, &["status", "--porcelain"]), "");
    let tracked = dogfood_git(&store, &["ls-files"]);
    assert!(!tracked.contains("outline") && !tracked.contains(".sifttext") && !tracked.contains(".meta.json"));
}
