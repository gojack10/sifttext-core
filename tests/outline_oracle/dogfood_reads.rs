// Initial inspection/scoped-search slice of dogfood_local.mjs.
// Publication/refusal/recovery and oracle-corruption slices retain separate owners.
fn dogfood_read_request(store: &Path, request: Value) -> Value {
    dogfood_expected_request(store, request, "current")
}
fn dogfood_expected_request(store: &Path, mut request: Value, expected: &str) -> Value {
    request["store"] = json!(store);
    let bin = std::env::var_os("CORE_OBSERVE_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/core-observe"));
    let out = capacity::observer_output(Command::new(bin), request.to_string().into_bytes());
    dogfood_outcome(out.status, &out.stdout, expected).unwrap_or_else(|e| {
        panic!(
            "{request}: {e}; stderr {}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}
#[test]
fn dogfood_inspect_scoped_search_cards_and_exact_pagination() {
    let dir = TempDir::new().unwrap();
    let store = fs::canonicalize(dir.path()).unwrap().join("store");
    let nodes = dogfood_nodes();
    materialize(&store, &nodes);
    assert_eq!(authored_records(&store), nodes);
    let before = authored_snapshot(&store);
    let ident = |i| format!("7a000000-0000-4000-8000-{i:012}");
    let all = nodes
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let path = |i: &str| store.join("nodes").join(i).join("node.md");
    let out = dogfood_graph_output(&[Path::new("inspect"), &store]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let inspection: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(inspection["roots"], json!([ident(1), ident(6)]));
    assert_eq!(
        inspection["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        all
    );
    for node in inspection["nodes"].as_array().unwrap() {
        let n = nodes.iter().find(|n| n["id"] == node["id"]).unwrap();
        assert_eq!(node["path"], json!(path(n["id"].as_str().unwrap())));
        assert_eq!(node["content"], n["content"]);
        assert_eq!(node["children"], n["children"]);
        assert!(!node["micro_summary"].as_str().unwrap().trim().is_empty());
        let navigation=n["see_also"].as_array().unwrap().iter().map(|r| {
            let target=nodes.iter().find(|n|n["id"]==r["target"]).unwrap();
            json!({"target":r["target"],"name":target["name"],"micro_summary":target["micro_summary"],
                "reason":r["reason"],"path":path(r["target"].as_str().unwrap())})
        }).collect::<Vec<_>>();
        assert_eq!(node["navigation"], json!(navigation));
    }
    let index = dogfood_read_request(&store, json!({"op":"index","mode":"full"}));
    let version = index["version"].as_str().unwrap();
    assert_eq!(version.len(), 64);
    assert!(
        version
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    assert_eq!(index["nodes"], json!(nodes));
    assert_eq!(
        index["links"],
        json!([
        {"source":ident(4),"target":ident(1),"reason":"Read the linked synthetic node."},
        {"source":ident(5),"target":ident(4),"reason":"Read the linked synthetic node."}])
    );
    let search =
        |query: &str, under: Option<String>, readable: &[String], limit: usize, offset: usize| {
            let v = dogfood_read_request(
                &store,
                json!({"op":"search","query":query,"under_node_id":under,
            "limit":limit,"offset":offset,"readable_ids":readable}),
            );
            assert_eq!(v["version"], version);
            assert_eq!(v["other_groups"], json!([]));
            v
        };
    for (query, under, readable, wanted) in [
        ("knowledge", None, all.clone(), vec![1, 2, 3, 4, 5, 6]),
        ("knowledge", Some(ident(1)), all.clone(), vec![1, 2, 3, 5]),
        ("knowledge", Some(ident(6)), all.clone(), vec![4, 6]),
        ("knowledge", Some(ident(2)), all.clone(), vec![2, 5]),
        ("knowledge", Some(ident(3)), all.clone(), vec![3]),
        ("gamma", Some(ident(1)), all.clone(), vec![]),
        ("gamma", Some(ident(6)), all.clone(), vec![4]),
        ("alpha", None, all.clone(), vec![2]),
        ("root", None, all.clone(), vec![1, 6]),
        (
            "knowledge",
            None,
            vec![ident(1), ident(2), ident(3), ident(6)],
            vec![1, 2, 3, 6],
        ),
        (
            "gamma",
            None,
            all.iter().filter(|i| **i != ident(4)).cloned().collect(),
            vec![],
        ),
        (
            "gamma",
            Some(ident(6)),
            all.iter().filter(|i| **i != ident(4)).cloned().collect(),
            vec![],
        ),
    ] {
        let v = search(query, under, &readable, 6, 0);
        let mut actual = v["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        actual.sort();
        assert_eq!(actual, wanted.into_iter().map(ident).collect::<Vec<_>>());
        for card in v["results"].as_array().unwrap() {
            let n = nodes.iter().find(|n| n["id"] == card["id"]).unwrap();
            assert_eq!(card["title"], n["name"]);
            assert_eq!(card["group"], "permitted");
            let text = fs::read_to_string(path(card["id"].as_str().unwrap())).unwrap();
            assert_eq!(
                text[4..].split_once("\n---\n").unwrap().1,
                n["content"].as_str().unwrap()
            );
        }
    }
    for card in search("knowledge", None, &all, 6, 0)["results"]
        .as_array()
        .unwrap()
    {
        assert!(
            card["snippet"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("knowledge")
        );
        assert!(card["rank"].as_u64().is_some_and(|rank| rank >= 1));
        assert!(card["relevance"].is_number());
    }
    let mut pages = vec![];
    for offset in [0, 2, 4] {
        let v = search("knowledge", None, &all, 2, offset);
        assert_eq!(v["total_matches"], all.len());
        pages.extend(
            v["results"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["id"].as_str().unwrap().to_owned()),
        );
    }
    assert_eq!(pages.iter().collect::<BTreeSet<_>>().len(), pages.len());
    pages.sort();
    assert_eq!(pages, all);
    let outline = dogfood_read_request(&store, json!({"op":"outline_inspect"}));
    assert_eq!(outline["version"], version);
    let expected_paths: BTreeSet<_> = all.iter().flat_map(|id| {
        ["outline.lean", "outline.full.lean", "outline.lean.meta.json", "outline.full.lean.meta.json"]
            .map(|name| store.join("nodes").join(id).join(name))
    }).collect();
    let files = outline["files"].as_array().unwrap();
    assert_eq!(files.len(), expected_paths.len(), "inspection has no duplicate or missing artifacts");
    let actual_paths: BTreeSet<_> = files.iter().map(|f| PathBuf::from(f["path"].as_str().unwrap())).collect();
    assert_eq!(actual_paths, expected_paths, "all physical nodes expose both views and sidecars");
    for f in files {
        let bytes = fs::read(f["path"].as_str().unwrap()).unwrap();
        assert_eq!(f["text"], std::str::from_utf8(&bytes).unwrap());
        assert_eq!(f["hash"], sha(&bytes));
    }
    check_native(&store, version, &ORACLE_BPE);
    assert_eq!(
        authored_snapshot(&store),
        before,
        "reads modified authored knowledge"
    );
    eprintln!("DOGFOOD_RUST_INSPECT_SCOPE_CARDS_PAGES_CHECKED");
}
