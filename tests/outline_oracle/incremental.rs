// Dependency propagation slice of incremental_outlines.mjs. The legacy suite
// retains sparse, forgery, race, killed-recovery and sampled-resource controls.
use std::os::unix::fs::MetadataExt;

fn incremental_id(i: usize) -> String {
    format!("71000000-0000-4000-8000-{:012}", i + 1)
}
fn dependency_specimen() -> Vec<Value> {
    (0..10).map(|i| {
        let cs = match i { 0 => vec![1,2], 1 => vec![3], 2 => vec![4], 5 => vec![6,7], 6 => vec![8], _ => vec![] };
        let mut n = json!({"id":incremental_id(i),"name":format!("Node {i}"),
            "micro_summary":format!("Summary {i} Ω"),"status":if i == 7 {"discarded"} else if i == 8 {"resolved"} else {"in_progress"},
            "scope":"Stable scope.","content":"Original authored body.\n",
            "children":cs.into_iter().map(incremental_id).collect::<Vec<_>>(),"see_also":[]});
        if i == 7 { n["discard_reason"] = json!("Rejected branch still visible."); }
        n
    }).collect()
}
// Include identity as well as bytes: rewriting identical files is a regression.
fn outline_identity(store: &Path) -> BTreeMap<PathBuf, (String, u64, i64, i64)> {
    generated_snapshot(store)
        .into_iter()
        .map(|(p, hash)| {
            let m = fs::metadata(store.join(&p)).unwrap();
            (p, (hash, m.ino(), m.mtime(), m.mtime_nsec()))
        })
        .collect()
}
fn incremental_call(store: &Path) -> (Value, Value) {
    let bin = std::env::var_os("CORE_OBSERVE_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/core-observe"));
    let mut command = Command::new(&bin);
    command
        .current_dir(bin.parent().unwrap().parent().unwrap().parent().unwrap())
        .env("SIFTTEXT_RETRIEVAL_TRACE", "1");
    let out = capacity::observer_output(
        command,
        json!({"op":"index","store":store,"mode":"incremental"})
            .to_string()
            .into_bytes(),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(result["status"], "current");
    let stderr = String::from_utf8(out.stderr).unwrap();
    let work = serde_json::from_str(
        stderr
            .lines()
            .find_map(|l| l.strip_prefix("retrieval-work "))
            .expect("missing retrieval trace"),
    )
    .unwrap();
    (result, work)
}
fn upsert_dependency_field(store: &Path, i: usize, key: &str, value: Value) {
    let p = store.join("nodes").join(incremental_id(i)).join("node.md");
    let text = fs::read_to_string(&p).unwrap();
    if text.lines().any(|l| l.starts_with(&format!("{key}:"))) {
        change_field(store, &incremental_id(i), key, value);
    } else {
        fs::write(
            p,
            text.replacen("---\n", &format!("---\n{key}: {value}\n"), 1),
        )
        .unwrap();
    }
}
fn assert_dependency_equality(store: &Path, incremental: &Value, bpe: &tiktoken_rs::CoreBPE) {
    let version = incremental["version"].as_str().unwrap();
    let before = outline_identity(store);
    let (ok, full) = native(store, "index", Some("full"));
    assert!(ok, "{full}");
    assert_eq!(&full, incremental, "full/incremental node or version drift");
    assert_eq!(
        outline_identity(store),
        before,
        "full rebuild rewrote navigation"
    );
    check_native(store, version, bpe);
}
fn dependency_mutation(
    store: &Path,
    label: &str,
    act: impl FnOnce(),
    allowed: &[usize],
    required: &[usize],
    bpe: &tiktoken_rs::CoreBPE,
) {
    let before = outline_identity(store);
    act();
    let (ok, stale) = native(store, "outline_inspect", None);
    assert!(!ok, "{label}: expected refusal");
    assert_eq!(stale["status"], "stale", "{label}");
    let (result, _) = incremental_call(store);
    let after = outline_identity(store);
    let writes: Vec<_> = after
        .iter()
        .filter(|(p, s)| before.get(*p) != Some(*s))
        .map(|(p, _)| p)
        .collect();
    for p in &writes {
        assert!(
            allowed
                .iter()
                .any(|i| p.starts_with(Path::new("nodes").join(incremental_id(*i)))),
            "{label}: unrelated write {}",
            p.display()
        );
    }
    for i in required {
        assert!(
            writes.iter().any(|p| **p
                == Path::new("nodes")
                    .join(incremental_id(*i))
                    .join("outline.full.lean")),
            "{label}: missing dependency {i}"
        );
    }
    assert_dependency_equality(store, &result, bpe);
}
fn dependency_order(store: &Path, parent: usize, cs: &[usize]) {
    fs::write(
        store
            .join("nodes")
            .join(incremental_id(parent))
            .join("children.order"),
        cs.iter()
            .map(|i| format!("{}\n", incremental_id(*i)))
            .collect::<String>(),
    )
    .unwrap();
}
#[test]
fn incremental_dependencies_preserve_unrelated_file_identity_and_independent_rows() {
    let bpe = ORACLE_BPE.clone();
    let dir = TempDir::new().unwrap();
    let store = fs::canonicalize(dir.path()).unwrap().join("store");
    materialize(&store, &dependency_specimen());
    let (initial, _) = incremental_call(&store);
    let baseline = outline_identity(&store);
    let (warm, _) = incremental_call(&store);
    assert_eq!(warm, initial);
    assert_eq!(outline_identity(&store), baseline);
    upsert_dependency_field(&store, 3, "scope", json!("Changed authored scope only."));
    let source = store.join("nodes").join(incremental_id(3)).join("node.md");
    fs::write(
        &source,
        fs::read_to_string(&source)
            .unwrap()
            .replace("Original authored body.", "Changed body only."),
    )
    .unwrap();
    let (ok, stale) = native(&store, "outline_inspect", None);
    assert!(!ok);
    assert_eq!(stale["status"], "stale");
    let (body, work) = incremental_call(&store);
    assert_ne!(body["version"], initial["version"]);
    let db = rusqlite::Connection::open(store.join(".sifttext/index.sqlite")).unwrap();
    let fields: (String, String) = db
        .query_row(
            "SELECT scope,content FROM nodes WHERE id=?",
            [incremental_id(3)],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        fields,
        (
            "Changed authored scope only.".into(),
            "Changed body only.\n".into()
        )
    );
    drop(db);
    assert_eq!(
        outline_identity(&store),
        baseline,
        "body/scope rewrote navigation"
    );
    assert_eq!(work["outline_files_staged"], 0);
    assert!(!store.join(".sifttext/outline-stage").exists());
    assert_dependency_equality(&store, &body, &bpe);
    dependency_mutation(
        &store,
        "summary",
        || {
            upsert_dependency_field(
                &store,
                3,
                "micro_summary",
                json!("A changed lossless summary Ω\nline."),
            )
        },
        &[0, 1, 3],
        &[0, 1, 3],
        &bpe,
    );
    dependency_mutation(
        &store,
        "status-reason",
        || {
            upsert_dependency_field(&store, 3, "status", json!("deferred"));
            upsert_dependency_field(
                &store,
                3,
                "deferred_reason",
                json!("Waiting on external facts."),
            );
        },
        &[0, 1, 3],
        &[0, 1, 3],
        &bpe,
    );
    dependency_mutation(
        &store,
        "warning",
        || {
            upsert_dependency_field(
                &store,
                4,
                "warnings",
                json!("Never drop \"quotes\", \\ or Ω\nlines."),
            )
        },
        &[0, 2, 4],
        &[0, 2, 4],
        &bpe,
    );
    let inactive = outline_identity(&store);
    upsert_dependency_field(
        &store,
        4,
        "blocker",
        json!("Inactive reason must not appear."),
    );
    let (result, work) = incremental_call(&store);
    assert_eq!(outline_identity(&store), inactive);
    assert_eq!(work["outline_files_staged"], 0);
    assert_dependency_equality(&store, &result, &bpe);
    dependency_mutation(
        &store,
        "reorder",
        || dependency_order(&store, 0, &[2, 1]),
        &[0],
        &[0],
        &bpe,
    );
    dependency_mutation(
        &store,
        "old-new-ancestor-move",
        || {
            fs::rename(
                store
                    .join("nodes")
                    .join(incremental_id(0))
                    .join("children")
                    .join(incremental_id(1)),
                store
                    .join("nodes")
                    .join(incremental_id(5))
                    .join("children")
                    .join(incremental_id(1)),
            )
            .unwrap();
            dependency_order(&store, 0, &[2]);
            dependency_order(&store, 5, &[6, 7, 1]);
        },
        &[0, 5],
        &[0, 5],
        &bpe,
    );
    dependency_mutation(
        &store,
        "add",
        || {
            let leaf = json!({"id":incremental_id(10),"name":"New leaf","micro_summary":"Additional branch","status":"in_progress","see_also":[],"children":[],"content":"New authored body.\n"});
            materialize(&store, &[leaf]);
            // materialize regards a standalone node as a root; this leaf is attached below.
            fs::remove_file(store.join("trees").join(incremental_id(10))).unwrap();
            std::os::unix::fs::symlink(
                format!("../../{}", incremental_id(10)),
                store
                    .join("nodes")
                    .join(incremental_id(6))
                    .join("children")
                    .join(incremental_id(10)),
            )
            .unwrap();
            dependency_order(&store, 6, &[8, 10]);
        },
        &[5, 6, 10],
        &[5, 6, 10],
        &bpe,
    );
    dependency_mutation(
        &store,
        "delete",
        || {
            fs::remove_dir_all(store.join("nodes").join(incremental_id(10))).unwrap();
            fs::remove_file(
                store
                    .join("nodes")
                    .join(incremental_id(6))
                    .join("children")
                    .join(incremental_id(10)),
            )
            .unwrap();
            dependency_order(&store, 6, &[8]);
        },
        &[5, 6],
        &[5, 6],
        &bpe,
    );
}
