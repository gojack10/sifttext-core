// The predecessor recomputed the whole authored forest for each file identity.
// Keep that algorithm as a bounded independent fixture for exact byte/cost parity.
#[test]
fn mixed_file_identities_preserve_predecessor_bytes_costs_and_refusals() {
    let bpe = ORACLE_BPE.clone();
    let nodes = forest();
    let dir = TempDir::new().unwrap();
    let store = dir.path();
    let identities = ["short", "Ω\tquoted \"identity\"\nnext", "long-".repeat(80).as_str()]
        .map(str::to_owned);
    let mut versions = BTreeMap::new();
    let mut expected = BTreeMap::new();
    for (index, n) in nodes.iter().enumerate() {
        let i = n["id"].as_str().unwrap();
        let mut views = json!({});
        for (offset, coverage) in ["full", "shallow"].into_iter().enumerate() {
            let identity = &identities[(index + offset) % identities.len()];
            versions.insert(file(store, i, coverage), identity.clone());
            views[coverage] = expectations(&nodes, identity, &bpe).unwrap()[i][coverage].clone();
        }
        expected.insert(i.to_owned(), views);
    }
    let costs: BTreeMap<_, _> = expected.iter()
        .map(|(i,v)| (i.clone(), v["full"]["meta"]["tokens"].clone())).collect();
    for (i, views) in &mut expected {
        let doc = &mut views["shallow"]["document"];
        for row in doc["rows"].as_array_mut().unwrap() {
            row["full_view_tokens"] = costs[row["id"].as_str().unwrap()].clone();
        }
        let text = render(doc);
        let n = nodes.iter().find(|n| n["id"] == *i).unwrap();
        views["shallow"]["text"] = json!(text);
        views["shallow"]["meta"] = json!({"hash":sha(text.as_bytes()), "tokens":count(&text,&bpe), "encoding":"o200k_base",
            "full_view_tokens":costs[i], "branch_full_tokens":children(n).iter().map(|c|(c.clone(),costs[c].clone())).collect::<BTreeMap<_,_>>()});
    }
    publish(store, &json!(expected), &nodes, 0);
    let before = snapshot(store);
    check_versions(&nodes, "global-version", store, 0, &bpe, Some(&versions)).unwrap();
    assert_eq!(snapshot(store), before);
    // Complete file identities are mandatory even though rows do not depend on them.
    let missing = versions.remove(&file(store, &id(12), "full")).unwrap();
    assert_eq!(check_versions(&nodes, "global-version", store, 0, &bpe, Some(&versions)).unwrap_err(), "missing checked identity");
    versions.insert(file(store, &id(12), "full"), missing);
    for coverage in ["full", "shallow"] {
        let path = file(store, &id(1), coverage);
        let original = versions[&path].clone();
        for blank in ["", " \t\n"] {
            versions.insert(path.clone(), blank.into());
            assert_eq!(check_versions(&nodes, "global-version", store, 0, &bpe, Some(&versions)).unwrap_err(), "missing inputs");
        }
        versions.insert(path, original);
    }
    let p = file(store, &id(1), "shallow");
    let mut doc = expected[&id(1)]["shallow"]["document"].clone();
    doc["rows"][1]["full_view_tokens"] = json!(999999);
    let text = render(&doc);
    fs::write(&p, &text).unwrap();
    let mut meta = expected[&id(1)]["shallow"]["meta"].clone();
    meta["hash"] = json!(sha(text.as_bytes()));
    meta["tokens"] = json!(count(&text, &bpe));
    fs::write(p.with_file_name("outline.lean.meta.json"), meta.to_string()).unwrap();
    assert_eq!(check_versions(&nodes, "global-version", store, 0, &bpe, Some(&versions)).unwrap_err(), "artifact differs");
}

#[test]
#[ignore = "explicit paired 257-node checker timing, not a runtime acceptance gate"]
fn file_identity_checker_257_timing() {
    let bpe = ORACLE_BPE.clone();
    let nodes: Vec<_> = (0..257).map(|i| json!({"id":id(i),"name":format!("Capacity item {i}"),
        "micro_summary":format!("Summary {i} Ω"),"status":"in_progress",
        "children":if i == 0 {(1..257).rev().map(id).collect::<Vec<_>>()} else {vec![]}})).collect();
    let dir = TempDir::new().unwrap();
    let store = dir.path();
    let expected = expectations(&nodes, "file-identity", &bpe).unwrap();
    publish(store, &json!(expected), &nodes, 0);
    let versions = nodes.iter().flat_map(|n| ["full","shallow"].map(|coverage|
        (file(store, n["id"].as_str().unwrap(), coverage), "file-identity".to_owned()))).collect();
    let before = snapshot(store);
    for sample in 0..3 {
        let start = std::time::Instant::now();
        check_versions(&nodes, "global-version", store, 0, &bpe, Some(&versions)).unwrap();
        eprintln!("identity-checker-257 sample={sample} ms={:.3}", start.elapsed().as_secs_f64()*1000.);
        assert_eq!(snapshot(store), before);
    }
}
