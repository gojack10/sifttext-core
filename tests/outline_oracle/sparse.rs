// Independent-worktree and complete pinned sparse-base slice of incremental_outlines.mjs.
#[test]
fn native_sparse_branch_leaf_overlay_and_hydration_preserve_logical_outlines() {
    let bpe = ORACLE_BPE.clone();
    let dir = TempDir::new().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let base = root.join("base");
    materialize(&base, &special_text_specimen());
    refresh(&base, "full");
    let base_files = snapshot(&base);
    let base_db = fs::read(base.join(".sifttext/index.sqlite")).unwrap();
    let sparse = root.join("sparse");
    copy_incremental_store(&base, &sparse);
    let ident = |n| format!("90000000-0000-4000-8000-{n:012}");
    for n in [6, 7] {
        fs::remove_dir_all(sparse.join("nodes").join(ident(n))).unwrap();
    }
    change_field(&sparse, &ident(8), "name", json!("Sparse overlay edit Ω"));
    let sparse_authored = authored_snapshot(&sparse);
    // Authoritative expectations merge authored records, never native SQL/output.
    let mut logical = authored_records(&base)
        .into_iter()
        .map(|n| (n["id"].as_str().unwrap().to_owned(), n))
        .collect::<BTreeMap<_, _>>();
    for n in authored_records(&sparse) {
        logical.insert(n["id"].as_str().unwrap().to_owned(), n);
    }
    let logical = logical.into_values().collect::<Vec<_>>();
    let verify = |version: &str| {
        let mut inventory = capacity::inventory(&sparse);
        for (path, entry) in capacity::inventory(&base) {
            if path.starts_with("nodes") {
                let owner = path.components().nth(1).unwrap().as_os_str();
                if !sparse.join("nodes").join(owner).exists() {
                    assert!(inventory.insert(path, entry).is_none());
                }
            }
        }
        assert_eq!(
            version,
            capacity::identities(&inventory).0,
            "sparse global identity is independently merged authored inventory"
        );
        let before = snapshot(&sparse);
        let view = sparse_request(&sparse, &base, "outline_inspect", None);
        assert_eq!(view["version"], version);
        assert_eq!(view["files"].as_array().unwrap().len(), 72);
        let mirror = TempDir::new_in(&root).unwrap();
        materialize(mirror.path(), &logical);
        let mut identities = BTreeMap::new();
        let mut saved = BTreeMap::new();
        for f in view["files"].as_array().unwrap() {
            let p = Path::new(f["path"].as_str().unwrap());
            let text = f["text"].as_str().unwrap();
            let owner = p.parent().unwrap().file_name().unwrap().to_str().unwrap();
            assert!(logical.iter().any(|n| n["id"] == owner));
            let expected_dir = if sparse.join("nodes").join(owner).is_dir() {
                sparse.join("nodes").join(owner)
            } else {
                sparse.join(".sifttext/outlines").join(owner)
            };
            assert_eq!(p.parent().unwrap(), expected_dir);
            assert_eq!(fs::read_to_string(p).unwrap(), text);
            assert_eq!(f["hash"], sha(text.as_bytes()));
            let target = mirror
                .path()
                .join("nodes")
                .join(owner)
                .join(p.file_name().unwrap());
            assert!(
                saved
                    .insert(
                        target.strip_prefix(mirror.path()).unwrap().to_path_buf(),
                        text.to_owned()
                    )
                    .is_none()
            );
            fs::write(&target, text).unwrap();
            if p.extension().is_some_and(|e| e == "lean") {
                let identity = text
                    .lines()
                    .find_map(|l| l.strip_prefix("source_version:"))
                    .unwrap();
                let identity = if identity.starts_with('"') {
                    serde_json::from_str::<String>(identity).unwrap()
                } else {
                    identity.to_owned()
                };
                identities.insert(target, identity);
            }
        }
        check_versions(&logical, version, mirror.path(), 0, &bpe, Some(&identities)).unwrap();
        let db = rusqlite::Connection::open_with_flags(
            sparse.join(".sifttext/index.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let indexed_version: String = db
            .query_row("select value from metadata where key='version'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(indexed_version, version);
        let mut statement = db.prepare("select record from nodes order by id").unwrap();
        let records = statement
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| serde_json::from_str::<Value>(&r.unwrap()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(records, logical);
        assert_eq!(snapshot(&sparse), before, "inspection changed sparse nodes");
        assert_eq!(
            snapshot(&base),
            base_files,
            "sparse workflow changed immutable base nodes"
        );
        assert_eq!(
            fs::read(base.join(".sifttext/index.sqlite")).unwrap(),
            base_db
        );
        saved
    };
    let indexed = sparse_request(&sparse, &base, "index", Some("full"));
    let version = indexed["version"].as_str().unwrap();
    let sparse_files = verify(version);
    for n in [6, 7] {
        assert!(
            !sparse.join("nodes").join(ident(n)).exists(),
            "refresh hydrated omitted authored node"
        );
    }
    assert_eq!(authored_snapshot(&sparse), sparse_authored);
    for n in [6, 7] {
        copy_incremental_store(
            &base.join("nodes").join(ident(n)),
            &sparse.join("nodes").join(ident(n)),
        );
    }
    let hydrated_authored = authored_snapshot(&sparse);
    assert_eq!(
        sparse_request(&sparse, &base, "index", Some("incremental")),
        indexed
    );
    assert_eq!(
        verify(version),
        sparse_files,
        "hydration changed logical outline bytes/costs"
    );
    assert_eq!(
        sparse_request(&sparse, &base, "index", Some("full")),
        indexed
    );
    assert_eq!(
        verify(version),
        sparse_files,
        "full rebuild changed hydrated outlines"
    );
    assert_eq!(authored_snapshot(&sparse), hydrated_authored);
}
fn copy_incremental_store(from: &Path, to: &Path) {
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(from)
            .arg(to)
            .status()
            .unwrap()
            .success()
    );
}
fn sparse_request(store: &Path, base: &Path, op: &str, mode: Option<&str>) -> Value {
    let mut req = json!({"op":op,"store":store,"base_store":base});
    if let Some(mode) = mode {
        req["mode"] = json!(mode);
    }
    let (ok, result) = native_request(req);
    assert!(ok, "{result}");
    assert_eq!(result["status"], "current");
    result
}
fn inspected_bytes(view: &Value) -> Vec<(String, String)> {
    let mut files: Vec<_> = view["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            let text = f["text"].as_str().unwrap();
            let path = Path::new(f["path"].as_str().unwrap());
            assert_eq!(fs::read_to_string(path).unwrap(), text);
            assert_eq!(f["hash"], sha(text.as_bytes()));
            (f["hash"].as_str().unwrap().to_owned(), text.to_owned())
        })
        .collect();
    files.sort();
    files
}
#[test]
fn incremental_independent_worktree_and_sparse_base_preserve_complete_outputs() {
    let bpe = ORACLE_BPE.clone();
    let dir = TempDir::new().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let store = root.join("store");
    materialize(&store, &dependency_specimen());
    let (initial, _) = incremental_call(&store);
    check_native(&store, initial["version"].as_str().unwrap(), &bpe);
    let unchanged = outline_identity(&store);
    let authored = authored_snapshot(&store);
    let twin = root.join("independent");
    copy_incremental_store(&store, &twin);
    change_field(
        &twin,
        &incremental_id(3),
        "micro_summary",
        json!("Private worktree summary."),
    );
    let (private, _) = incremental_call(&twin);
    let (original, _) = incremental_call(&store);
    assert_ne!(private["version"], original["version"]);
    assert_eq!(original, initial);
    assert_eq!(outline_identity(&store), unchanged);
    assert_eq!(authored_snapshot(&store), authored);
    assert_dependency_equality(&twin, &private, &bpe);

    let base = root.join("base");
    copy_incremental_store(&store, &base);
    let base_files = outline_identity(&base);
    let base_authored = authored_snapshot(&base);
    let sparse = root.join("sparse");
    copy_incremental_store(&base, &sparse);
    let omitted = sparse.join("nodes").join(incremental_id(3));
    fs::remove_dir_all(&omitted).unwrap();
    let sparse_authored = authored_snapshot(&sparse);
    let current = sparse_request(&sparse, &base, "index", Some("incremental"));
    let (ok, base_current) = native(&base, "index", Some("incremental"));
    assert!(ok, "{base_current}");
    assert_eq!(current, base_current);
    assert!(!omitted.exists(), "sparse refresh hydrated omitted node");
    assert_eq!(
        outline_identity(&base),
        base_files,
        "sparse refresh rewrote base navigation"
    );
    assert_eq!(authored_snapshot(&base), base_authored);
    assert_eq!(authored_snapshot(&sparse), sparse_authored);
    let sparse_view = sparse_request(&sparse, &base, "outline_inspect", None);
    let (ok, base_view) = native(&base, "outline_inspect", None);
    assert!(ok, "{base_view}");
    assert_eq!(inspected_bytes(&sparse_view), inspected_bytes(&base_view));
    // Expectations still come from complete authored base records, not native output.
    check_native(&base, current["version"].as_str().unwrap(), &bpe);
    let before_full = outline_identity(&sparse);
    assert_eq!(
        sparse_request(&sparse, &base, "index", Some("full")),
        current
    );
    assert_eq!(
        outline_identity(&sparse),
        before_full,
        "full sparse refresh rewrote navigation"
    );
    assert_eq!(
        inspected_bytes(&sparse_request(&sparse, &base, "outline_inspect", None)),
        inspected_bytes(&base_view)
    );
    assert!(
        !omitted.exists(),
        "full sparse refresh hydrated omitted node"
    );
    assert_eq!(outline_identity(&base), base_files);
    assert_eq!(authored_snapshot(&base), base_authored);
    assert_eq!(authored_snapshot(&sparse), sparse_authored);
}
