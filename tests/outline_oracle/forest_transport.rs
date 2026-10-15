// Actual observer transport controls, not just assemble_forest/serde unit results.
#[test]
fn forest_observer_rejects_invalid_requests_without_partial_success() {
    let nodes = forest();
    let patch = |target: usize, changed: Value| {
        let mut nodes = nodes.clone();
        let node = nodes
            .iter_mut()
            .find(|node| node["id"] == id(target))
            .unwrap();
        for (key, value) in changed.as_object().unwrap() {
            node[key] = value.clone();
        }
        json!({"source_version":"v1", "nodes":nodes}).to_string()
    };
    let mut duplicate = nodes.clone();
    duplicate.push(
        nodes
            .iter()
            .find(|node| node["id"] == id(12))
            .unwrap()
            .clone(),
    );
    for (label, supplied) in [
        ("malformed JSON", "{\"source_version\":\"v\"".into()),
        ("empty input", String::new()),
        (
            "trailing document",
            "{\"source_version\":\"v\",\"nodes\":[]} {}".into(),
        ),
        ("non-object", "[1,2,3]".into()),
        ("missing version", json!({"nodes":nodes}).to_string()),
        ("missing nodes", json!({"source_version":"v"}).to_string()),
        (
            "extra owner",
            json!({"source_version":"v","nodes":[],"owner":id(1)}).to_string(),
        ),
        (
            "extra coverage",
            json!({"source_version":"v","nodes":[],"coverage":"full"}).to_string(),
        ),
        (
            "null version",
            json!({"source_version":null,"nodes":[]}).to_string(),
        ),
        (
            "blank version",
            json!({"source_version":"","nodes":nodes}).to_string(),
        ),
        (
            "whitespace version",
            json!({"source_version":" \t ","nodes":nodes}).to_string(),
        ),
        (
            "blank empty forest",
            json!({"source_version":"","nodes":[]}).to_string(),
        ),
        (
            "whitespace empty forest",
            json!({"source_version":" \t\n","nodes":[]}).to_string(),
        ),
        (
            "numeric version",
            json!({"source_version":42,"nodes":nodes}).to_string(),
        ),
        (
            "nodes not array",
            json!({"source_version":"v","nodes":{}}).to_string(),
        ),
        (
            "node not object",
            json!({"source_version":"v","nodes":[id(1)]}).to_string(),
        ),
        (
            "blank outside summary",
            patch(14, json!({"micro_summary":" "})),
        ),
        (
            "whitespace outside summary",
            patch(14, json!({"micro_summary":"\t"})),
        ),
        ("blank outside reason", patch(7, json!({"blocker":" "}))),
        (
            "blank outside resolved",
            patch(14, json!({"crystallization":""})),
        ),
        (
            "duplicate identity",
            json!({"source_version":"v","nodes":duplicate}).to_string(),
        ),
        ("malformed identity", patch(12, json!({"id":"not-a-uuid"}))),
        ("missing child", patch(13, json!({"children":[id(99)]}))),
        (
            "duplicate child",
            patch(13, json!({"children":[id(14),id(14)]})),
        ),
        (
            "multiple parents",
            patch(13, json!({"children":[id(14),id(6)]})),
        ),
        ("two-node cycle", patch(12, json!({"children":[id(10)]}))),
        ("self cycle", patch(12, json!({"children":[id(12)]}))),
        ("unknown status", patch(12, json!({"status":"archived"}))),
    ] {
        let out = assembler_output(supplied.as_bytes());
        assert!(
            out.status.code().is_some(),
            "{label}: observer did not exit normally"
        );
        assert!(!out.status.success(), "{label}: invalid request succeeded");
        assert!(
            out.stdout.is_empty(),
            "{label}: rejected request emitted partial success"
        );
        assert!(
            String::from_utf8_lossy(&out.stderr).starts_with("rejected: "),
            "{label}: refusal lost its reason"
        );
    }
    let out = assembler_output(b"{\"source_version\":\"empty-v1\",\"nodes\":[]}");
    assert!(out.status.success(), "empty forest: {:?}", out);
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        json!({"source_version":"empty-v1","owners":{}})
    );
}
