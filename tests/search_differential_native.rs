//! Independent Leviathan expectations, acquired without invoking Core.
//! Set SIFTTEXT_TEST_SEARCH_ORACLE to revalidate the recorded upstream corpus.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

// JSON/JS has one numeric type: 0 and 0.0 are equal, without score tolerance.
fn same_json(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_json(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, a)| b.get(k).is_some_and(|b| same_json(a, b)))
        }
        _ => a == b,
    }
}
fn corpus() -> Value {
    serde_json::from_str(include_str!("../fixtures/search-oracle/leviathan.json")).unwrap()
}
fn command(bin: &Path) -> Command {
    let mut c = Command::new(bin);
    for (key, _) in std::env::vars_os() {
        if key.as_encoded_bytes().starts_with(b"GIT_") {
            c.env_remove(key);
        }
    }
    c.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c
}
// An exit/refusal must agree with its JSON; signals and malformed replies never pass.
fn outcome(
    code: Option<i32>,
    signaled: bool,
    stdout: &[u8],
    current: bool,
) -> Result<Value, String> {
    if signaled || code != Some(if current { 0 } else { 1 }) {
        return Err("abnormal or inconsistent exit".into());
    }
    let v: Value = serde_json::from_slice(stdout).map_err(|e| e.to_string())?;
    if if current {
        v["status"] != "current"
    } else {
        !["stale", "error"].contains(&v["status"].as_str().unwrap_or(""))
    } {
        return Err("unexpected status".into());
    }
    Ok(v)
}
#[test]
fn outcome_parser_rejects_false_success_and_malformed_processes() {
    assert!(same_json(&json!({"score":0}), &json!({"score":0.0})));
    assert!(!same_json(
        &json!({"score":1.0}),
        &json!({"score":1.000001})
    ));
    assert!(!same_json(&json!([0]), &json!([0, 0])));
    assert!(outcome(Some(1), false, br#"{"status":"error"}"#, false).is_ok());
    for (code, signal, text, current) in [
        (Some(0), false, br#"{"status":"error"}"#.as_slice(), false),
        (Some(1), false, br#"{"status":"current"}"#.as_slice(), false),
        (Some(1), false, br#"{"status":"unknown"}"#.as_slice(), false),
        (Some(1), false, b"not JSON".as_slice(), false),
        (None, true, br#"{"status":"error"}"#.as_slice(), false),
        (Some(1), false, br#"{"status":"current"}"#.as_slice(), true),
    ] {
        assert!(outcome(code, signal, text, current).is_err());
    }
}
fn fixture(store: &Path, nodes: &[Value]) {
    fs::create_dir_all(store.join("trees")).unwrap();
    for n in nodes {
        let id = n["id"].as_str().unwrap();
        let p = store.join("nodes").join(id);
        fs::create_dir_all(p.join("children")).unwrap();
        let mut text = String::from("---\n");
        for (k, v) in n.as_object().unwrap() {
            if !["content", "children"].contains(&k.as_str()) {
                text.push_str(&format!("{k}: {v}\n"));
            }
        }
        text.push_str(&format!("---\n{}", n["content"].as_str().unwrap()));
        fs::write(p.join("node.md"), text).unwrap();
        let mut order = String::new();
        for c in n["children"].as_array().unwrap() {
            let c = c.as_str().unwrap();
            order.push_str(c);
            order.push('\n');
            symlink(format!("../../{c}"), p.join("children").join(c)).unwrap();
        }
        fs::write(p.join("children.order"), order).unwrap();
    }
    for n in nodes {
        if !nodes
            .iter()
            .any(|p| p["children"].as_array().unwrap().contains(&n["id"]))
        {
            let id = n["id"].as_str().unwrap();
            symlink(format!("../nodes/{id}"), store.join("trees").join(id)).unwrap();
        }
    }
}
fn authored(store: &Path, nodes: &[Value]) -> Vec<Vec<u8>> {
    nodes
        .iter()
        .flat_map(|n| {
            ["node.md", "children.order"].map(|f| {
                fs::read(store.join("nodes").join(n["id"].as_str().unwrap()).join(f)).unwrap()
            })
        })
        .collect()
}
fn upstream(oracle: &Path, evidence: &Path, group: usize, nodes: &[Value], cases: &[Value]) {
    let prefix = evidence.join(format!("upstream-{group}"));
    let first = &cases[0];
    let mut selected = Vec::new();
    fn walk(nodes: &[Value], id: &Value, out: &mut Vec<Value>) {
        out.push(id.clone());
        for c in nodes.iter().find(|n| n["id"] == *id).unwrap()["children"]
            .as_array()
            .unwrap()
        {
            walk(nodes, c, out);
        }
    }
    if first["scope"].is_null() {
        selected.extend(nodes.iter().map(|n| n["id"].clone()));
    } else {
        walk(nodes, &first["scope"], &mut selected);
    }
    selected.retain(|id| first["readable"].as_array().unwrap().contains(id));
    let projected=nodes.iter().map(|n|json!({"id":n["id"],"title":n["name"],"body":format!("{}\n{}\n{}",n["micro_summary"].as_str().unwrap(),n["scope"].as_str().unwrap(),n["content"].as_str().unwrap()),"grp":if selected.contains(&n["id"]) {"permitted"} else {"outside"},"boost_tag":if first["ranking"]["boosts"][n["id"].as_str().unwrap()]==2.5 {"yes"} else {"no"}}).to_string()).collect::<Vec<_>>().join("\n")+"\n";
    let source = prefix.with_extension("jsonl");
    let db = prefix.with_extension("db");
    fs::write(&source, projected).unwrap();
    let mut c = command(oracle);
    c.args([
        "--index",
        db.to_str().unwrap(),
        "--json",
        "index",
        source.to_str().unwrap(),
        "--strict",
        "--force",
    ]);
    if !first["ranking"].is_null() {
        let config = prefix.with_extension("toml");
        fs::write(&config,"[fields]\nid = \"id\"\ntitle = [\"title\"]\ntext = [\"body\"]\ngroup = \"grp\"\n[rank]\ntitle_weight = 3\n[[rank.boost]]\nfield = [\"boost_tag\"]\nequals = \"yes\"\nweight = 1.5\n").unwrap();
        c.args(["--config", config.to_str().unwrap()]);
    } else {
        c.args([
            "--id", "id", "--title", "title", "--text", "body", "--group", "grp",
        ]);
    }
    let out = c.output().unwrap();
    assert!(out.status.success(), "{out:?}");
    for case in cases {
        let out = command(oracle)
            .args([
                "--index",
                db.to_str().unwrap(),
                "--json",
                "search",
                "--group",
                "permitted",
                "--no-fallback",
                "--limit",
                "3",
                "--offset",
                &case["offset"].to_string(),
                "--",
                case["query"].as_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        assert!(
            same_json(
                &serde_json::from_slice::<Value>(&out.stdout).unwrap(),
                &case["expected"]
            ),
            "upstream corpus drift: {case}"
        );
    }
}
#[test]
fn native_search_preserves_legacy_login_scope_resources() {
    let temp = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("SIFTTEXT_TEST_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("evidence"))
        .join("legacy-login-scopes");
    fs::create_dir_all(&evidence).unwrap();
    let c = corpus();
    let mut nodes = c["nodes"].as_array().unwrap().clone();
    // Legacy resources are observed after the accepted capture-barrier edit.
    nodes[1]["name"] = json!("Newer accepted identity");
    let store = evidence.join("store");
    fixture(&store, &nodes);
    let before = authored(&store, &nodes);
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_BIN_EXE_core-observe"))
                .parent()
                .unwrap()
                .into()
        })
        .join("core-observe");
    let bpe = tiktoken_rs::o200k_base().unwrap();
    let mut measurements = Vec::new();
    for (i, scope) in [
        Value::Null,
        nodes[0]["id"].clone(),
        nodes[1]["id"].clone(),
        nodes[3]["id"].clone(),
        nodes[4]["id"].clone(),
    ]
    .into_iter()
    .enumerate()
    {
        let q = json!({"op":"search","store":store,"query":"login","under_node_id":scope,"readable_ids":nodes.iter().map(|n|n["id"].clone()).collect::<Vec<_>>(),"limit":3,"offset":0});
        let mut cmd = command(Path::new("/usr/bin/time"));
        if cfg!(target_os = "macos") {
            cmd.arg("-l");
        } else {
            cmd.arg("-v");
        }
        let start = Instant::now();
        let mut child = cmd.arg(&bin).spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(q.to_string().as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        let stderr = String::from_utf8_lossy(&out.stderr);
        let rss = if cfg!(target_os = "macos") {
            stderr
                .lines()
                .find(|s| s.contains("maximum resident set size"))
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .parse::<u64>()
                .unwrap()
        } else {
            stderr
                .lines()
                .find(|s| s.contains("Maximum resident set size (kbytes):"))
                .unwrap()
                .split(':')
                .nth(1)
                .unwrap()
                .trim()
                .parse::<u64>()
                .unwrap()
                * 1024
        };
        assert!(rss > 0, "missing resource receipt");
        let actual = outcome(
            out.status.code(),
            out.status.code().is_none(),
            &out.stdout,
            true,
        )
        .unwrap();
        assert!(actual["version"].is_string());
        assert_eq!(actual["other_groups"], json!([]));
        if i == 3 {
            assert_eq!(actual["results"], json!([]));
        }
        let text = actual["results"].to_string();
        measurements.push(json!({"scope":scope,"ms":ms,"rss_bytes":rss,"returned_o200k_base_tokens":bpe.encode_ordinary(&text).len(),"returned_results_json":text}));
        fs::write(evidence.join(format!("scope-{i}.json")),serde_json::to_vec_pretty(&json!({"request":q,"exit":out.status.code(),"stdout":String::from_utf8_lossy(&out.stdout),"stderr":stderr,"ms":ms})).unwrap()).unwrap();
    }
    assert_eq!(authored(&store, &nodes), before);
    fs::write(evidence.join("resources.json"),serde_json::to_vec_pretty(&json!({"measurements":measurements,"sqlite_bytes":fs::metadata(store.join(".sifttext/index.sqlite")).unwrap().len(),"limits":["8 synthetic nodes, not aggregate scale","accepted source edit reconstructed; live barrier covered separately","memory tools differ on macOS/Linux; no capacity threshold"]})).unwrap()).unwrap();
}
#[test]
fn native_search_preserves_252_upstream_comparisons_and_resources() {
    let temp = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("SIFTTEXT_TEST_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("evidence"));
    fs::create_dir_all(&evidence).unwrap();
    let c = corpus();
    let nodes = c["nodes"].as_array().unwrap();
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 252);
    if let Some(oracle) = std::env::var_os("SIFTTEXT_TEST_SEARCH_ORACLE") {
        for (group, cases) in cases.chunks(36).enumerate() {
            upstream(Path::new(&oracle), &evidence, group, nodes, cases);
        }
    }
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_BIN_EXE_core-observe"))
                .parent()
                .unwrap()
                .into()
        })
        .join("core-observe");
    let bpe = tiktoken_rs::o200k_base().unwrap();
    // ponytail: at most four isolated stores/CLI consumers; tune only with measured contention.
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get().min(4));
    let (measurements, sqlite_bytes) = std::thread::scope(|scope| {
        let handles = cases.chunks(cases.len().div_ceil(workers)).enumerate().map(|(worker, chunk)| {
            let (evidence, bin, bpe) = (&evidence, &bin, &bpe);
            scope.spawn(move || {
                let store = evidence.join(if worker == 0 { "store".into() } else { format!("store-{worker}") });
                fixture(&store, nodes);
                let before = authored(&store, nodes);
                let mut measurements = Vec::new();
                for (index, case) in chunk.iter().enumerate() {
                    let i = worker * cases.len().div_ceil(workers) + index;
        let mut q = json!({"op":"search","store":store,"query":case["query"],"offset":case["offset"],"limit":3,"under_node_id":case["scope"],"readable_ids":case["readable"]});
        if !case["ranking"].is_null() {
            q["ranking"] = case["ranking"].clone();
        }
        let mut cmd = command(Path::new("/usr/bin/time"));
        if cfg!(target_os = "macos") {
            cmd.arg("-l");
        } else {
            cmd.args(["-v"]);
        }
        let start = Instant::now();
        let mut child = cmd.arg(bin).spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(q.to_string().as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        let stderr = String::from_utf8_lossy(&out.stderr);
        fs::write(evidence.join(format!("native-{i}.json")),serde_json::to_vec_pretty(&json!({"request":q,"exit":out.status.code(),"stdout":String::from_utf8_lossy(&out.stdout),"stderr":stderr,"ms":ms})).unwrap()).unwrap();
        let rss = if cfg!(target_os = "macos") {
            stderr
                .lines()
                .find(|s| s.contains("maximum resident set size"))
                .and_then(|s| s.split_whitespace().next())
                .unwrap()
                .parse::<u64>()
                .unwrap()
        } else {
            stderr
                .lines()
                .find(|s| s.contains("Maximum resident set size (kbytes):"))
                .and_then(|s| s.split(':').nth(1))
                .unwrap()
                .trim()
                .parse::<u64>()
                .unwrap()
                * 1024
        };
        assert!(rss > 0, "missing resource receipt");
        let actual = outcome(
            out.status.code(),
            out.status.code().is_none(),
            &out.stdout,
            true,
        )
        .unwrap();
        assert!(actual["version"].is_string());
        assert_eq!(
            actual["total_matches"], case["expected"]["total_matches"],
            "{case}"
        );
        assert!(
            same_json(&actual["results"], &case["expected"]["results"]),
            "result mismatch: {actual} versus {case}"
        );
        assert_eq!(actual["other_groups"], json!([]));
        let result_text = actual["results"].to_string();
        measurements.push(json!({"case":i,"ms":ms,"rss_bytes":rss,"returned_o200k_base_tokens":bpe.encode_ordinary(&result_text).len(),"returned_results_json":result_text}));
    }
                assert_eq!(authored(&store, nodes), before);
                (measurements, fs::metadata(store.join(".sifttext/index.sqlite")).unwrap().len())
            })
        }).collect::<Vec<_>>();
        let mut measurements = Vec::new();
        let mut sqlite_bytes = Vec::new();
        for handle in handles {
            let (rows, bytes) = handle.join().unwrap();
            measurements.extend(rows);
            sqlite_bytes.push(bytes);
        }
        (measurements, sqlite_bytes)
    });
    assert_eq!(measurements.len(), cases.len());
    for (i, row) in measurements.iter().enumerate() {
        assert_eq!(row["case"], i);
    }
    fs::write(evidence.join("resources.json"),serde_json::to_vec_pretty(&json!({"measurements":measurements,"workers":workers,"sqlite_bytes":sqlite_bytes[0],"sqlite_bytes_by_worker":sqlite_bytes,"provenance":c["provenance"],"limits":["8 synthetic nodes, not aggregate scale","macOS and Linux memory tools differ","exact-score tie order is observed, not guaranteed","per-call latency observed under bounded concurrent load"]})).unwrap()).unwrap();
}
