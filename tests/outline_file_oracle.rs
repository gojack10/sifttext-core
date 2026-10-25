// Rust replacement for outline_files.mjs assembler and native-index checks.
// Legacy consumers remain until source-bound integration passes.
// Expectations come from authored records, a test-only pinned scalar/row encoder and
// tiktoken-rs, never from assembler output. No Bend build or live knowledge is needed.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use tempfile::TempDir;

// Share immutable pinned encoding tables, not a mutex held across native calls.
static ORACLE_BPE: std::sync::LazyLock<std::sync::Arc<tiktoken_rs::CoreBPE>> =
    std::sync::LazyLock::new(|| std::sync::Arc::new(tiktoken_rs::o200k_base().unwrap()));
include!("outline_oracle/index.rs");
include!("outline_oracle/shared_observer_stop.rs");
include!("outline_oracle/dogfood.rs");
include!("outline_oracle/dogfood_reads.rs");
include!("outline_oracle/dogfood_controls.rs");
include!("outline_oracle/incremental.rs");
include!("outline_oracle/sparse.rs");
include!("outline_oracle/forgeries.rs");
include!("outline_oracle/recovery.rs");
include!("outline_oracle/watch.rs");
include!("outline_oracle/resources.rs");
include!("outline_oracle/identity_costs.rs");
include!("outline_oracle/capacity.rs");
include!("outline_oracle/scaling.rs");
include!("outline_oracle/publication_contention.rs");
include!("outline_oracle/forest_transport.rs");
include!("outline_oracle/sequential_helper_stop.rs");

fn id(n: usize) -> String {
    format!("70000000-0000-4000-8000-{n:012}")
}
fn forest() -> Vec<Value> {
    let specs = [
        (1, "Root", "in_progress", vec![2, 6, 8]),
        (11, "Wide three", "discarded", vec![]),
        (2, "Branch", "stuck", vec![3, 4]),
        (10, "Wide two", "in_progress", vec![12]),
        (3, "Discarded", "discarded", vec![]),
        (6, "Resolved", "resolved", vec![]),
        (4, "Deep", "deferred", vec![5]),
        (9, "Wide one", "not_examined", vec![]),
        (5, "Deepest", "resolved", vec![]),
        (8, "Wide", "in_progress", vec![11, 9, 10]),
        (12, "Leaf", "not_examined", vec![]),
        (7, "Stuck leaf", "stuck", vec![]),
        (13, "Other root", "in_progress", vec![14, 7]),
        (14, "Other leaf", "resolved", vec![]),
    ];
    specs.into_iter().map(|(n,name,status,children)| {
        let mut node = json!({"id":id(n),"name":name,"status":status,
            "micro_summary":format!("{name} owns a bounded navigation branch."),
            "scope":"NEVER IN AN OUTLINE", "children":children.into_iter().map(id).collect::<Vec<_>>()});
        match status {
            "stuck" => node["blocker"] = json!(if n == 2 {"Wait for input.\nSecond line."} else {"External wait."}),
            "discarded" => node["discard_reason"] = json!(if n == 3 {"Rejected for a stated reason.\nKeep the LF."} else {"No longer worth examining."}),
            "deferred" => node["deferred_reason"] = json!("Resume after the index lands."),
            "resolved" => node["content"] = json!("Checked answer, never displayed."), _ => ()
        }
        if n == 2 || n == 4 { node["warnings"] = json!("Unicode Ω漢字 plus \"quotes\", a \\ backslash and a\ttab.\nA second line."); }
        node
    }).collect()
}
fn children(n: &Value) -> Vec<String> {
    n["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect()
}
fn active(status: &str) -> Option<&'static str> {
    match status {
        "stuck" => Some("blocker"),
        "deferred" => Some("deferred_reason"),
        "discarded" => Some("discard_reason"),
        _ => None,
    }
}
fn snapshot(store: &Path) -> BTreeMap<PathBuf, String> {
    let mut out = BTreeMap::new();
    fn visit(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, String>) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            let meta = fs::symlink_metadata(&p).unwrap();
            if meta.is_dir() {
                visit(root, &p, out);
                continue;
            }
            let digest = if meta.is_symlink() {
                format!("link:{}", fs::read_link(&p).unwrap().display())
            } else {
                sha(&fs::read(&p).unwrap())
            };
            out.insert(p.strip_prefix(root).unwrap().to_path_buf(), digest);
        }
    }
    visit(store, &store.join("nodes"), &mut out);
    out
}
fn uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn count(text: &str, bpe: &tiktoken_rs::CoreBPE) -> usize {
    bpe.encode_ordinary(text).len()
}
fn scalar(v: &Value, cell: bool) -> String {
    if let Some(s) = v.as_str() {
        let quote = s.is_empty()
            || ["T", "F", "_"].contains(&s)
            || s.trim() != s
            || s.parse::<f64>().is_ok()
            || s.contains(['\t', '\n', '\\', '"']);
        if !quote {
            return s.into();
        }
        let escaped = s
            .replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('"', if cell { "\"\"" } else { "\\\"" });
        format!("\"{escaped}\"")
    } else {
        v.to_string()
    }
}
// Only the scalar-object-array subset used by generated outlines; no product renderer call.
fn render(doc: &Value) -> String {
    let mut lines = vec![];
    for (key, v) in doc.as_object().unwrap() {
        if key != "rows" {
            lines.push(format!("{key}:{}", scalar(v, false)));
            continue;
        }
        let rows = v.as_array().unwrap();
        let keys: Vec<_> = rows[0]
            .as_object()
            .unwrap()
            .keys()
            .filter(|k| rows.iter().all(|r| r.get(*k).is_some()))
            .cloned()
            .collect();
        let uniform = rows
            .iter()
            .all(|r| r.as_object().unwrap().keys().cloned().collect::<Vec<_>>() == keys);
        let mut tab = vec![format!(
            "rows[{}]:{}{}",
            rows.len(),
            keys.join("\t"),
            if uniform { "" } else { "\t~" }
        )];
        for row in rows {
            let mut cells: Vec<_> = keys.iter().map(|k| scalar(&row[k], true)).collect();
            cells.extend(
                row.as_object()
                    .unwrap()
                    .iter()
                    .filter(|(k, _)| !keys.contains(k))
                    .map(|(k, v)| format!("{k}:{}", scalar(v, true))),
            );
            tab.push(format!("  {}", cells.join("\t")));
        }
        let mut dashed = vec![format!("rows[{}]:", rows.len())];
        for row in rows {
            for (i, (k, v)) in row.as_object().unwrap().iter().enumerate() {
                dashed.push(format!(
                    "  {}{k}:{}",
                    if i == 0 { "- " } else { "  " },
                    scalar(v, false)
                ));
            }
        }
        let cost = |ls: &[String]| {
            ls.iter()
                .map(|s| s.encode_utf16().count() + 1)
                .sum::<usize>()
        };
        lines.extend(if uniform || cost(&tab) < cost(&dashed) {
            tab
        } else {
            dashed
        });
    }
    lines.join("\n") + "\n"
}
fn walk(
    by: &BTreeMap<String, Value>,
    owner: &str,
    depth: usize,
    out: &mut Vec<(String, usize)>,
    seen: &mut BTreeSet<String>,
) -> Result<(), String> {
    if !seen.insert(owner.into()) {
        return Err("cycle".into());
    }
    let node = by.get(owner).ok_or("missing child")?;
    out.push((owner.into(), depth));
    for child in children(node) {
        walk(by, &child, depth + 1, out, seen)?;
    }
    Ok(())
}
fn expectations(
    nodes: &[Value],
    version: &str,
    bpe: &tiktoken_rs::CoreBPE,
) -> Result<BTreeMap<String, Value>, String> {
    if nodes.is_empty() || version.trim().is_empty() {
        return Err("missing inputs".into());
    }
    let mut by = BTreeMap::new();
    let mut parents = BTreeSet::new();
    for n in nodes {
        let ident = n["id"].as_str().ok_or("identity")?;
        if !uuid(ident) || by.insert(ident.to_owned(), n.clone()).is_some() {
            return Err("identity".into());
        }
        for key in ["name", "micro_summary"] {
            if n[key].as_str().is_none_or(|s| s.trim().is_empty()) {
                return Err(format!("blank {key}"));
            }
        }
        let status = n["status"].as_str().ok_or("status")?;
        if ![
            "not_examined",
            "in_progress",
            "resolved",
            "stuck",
            "discarded",
            "deferred",
        ]
        .contains(&status)
        {
            return Err("status".into());
        }
        if let Some(key) = active(status)
            && n[key].as_str().is_none_or(|s| s.trim().is_empty())
        {
            return Err(format!("blank {key}"));
        }
        if status == "resolved"
            && n["content"]
                .as_str()
                .is_none_or(|s| s.trim().is_empty())
        {
            return Err("blank resolved body".into());
        }
        for child in children(n) {
            if !uuid(&child) || !parents.insert(child) {
                return Err("duplicate parent".into());
            }
        }
    }
    let mut traversals = BTreeMap::new();
    for ident in by.keys() {
        let mut out = vec![];
        walk(&by, ident, 0, &mut out, &mut BTreeSet::new())?;
        traversals.insert(ident.clone(), out);
    }
    let document = |ident: &str, coverage: &str, costs: &BTreeMap<String, usize>| {
        let rows: Vec<_> = traversals[ident].iter().filter(|(_,d)|coverage=="full" || *d<=1).map(|(rid,d)| {
            let n=&by[rid];
            let mut r=json!({"children":children(n).len(),"depth":d,"descendants":traversals[rid].len()-1,"id":rid,
                "micro_summary":n["micro_summary"],"name":n["name"],"status":n["status"]});
            if let Some(k)=active(n["status"].as_str().unwrap()) {r[k]=n[k].clone();}
            if n["warnings"].as_str().is_some_and(|s|!s.is_empty()) {r["warnings"]=n["warnings"].clone();}
            if coverage=="shallow" {r["full_view_tokens"]=json!(costs[rid]);} r
        }).collect();
        json!({"coverage":coverage,"path_template":"nodes/{id}/node.md","rows":rows,"source_version":version,"source_version_scope":"outline_view","token_encoding":"o200k_base"})
    };
    let full: BTreeMap<_, _> = by
        .keys()
        .map(|i| (i.clone(), document(i, "full", &BTreeMap::new())))
        .collect();
    let costs: BTreeMap<_, _> = full
        .iter()
        .map(|(i, d)| (i.clone(), count(&render(d), bpe)))
        .collect();
    Ok(full.into_iter().map(|(i, full_doc)| {
        let mut views=json!({});
        for (coverage, doc) in [("full", full_doc), ("shallow", document(&i,"shallow",&costs))] {
            let text=render(&doc); let tokens=if coverage=="full" {costs[&i]} else {count(&text,bpe)}; let mut meta=json!({"hash":sha(text.as_bytes()),"tokens":tokens,"encoding":"o200k_base"});
            if coverage=="shallow" {meta["full_view_tokens"]=json!(costs[&i]);meta["branch_full_tokens"]=json!(children(&by[&i]).iter().map(|c|(c.clone(),costs[c])).collect::<BTreeMap<_,_>>());}
            views[coverage]=json!({"document":doc,"text":text,"meta":meta});
        }
        (i,views)
    }).collect())
}
fn filename(coverage: &str) -> &'static str {
    if coverage == "full" {
        "outline.full.lean"
    } else {
        "outline.lean"
    }
}
fn file(store: &Path, id: &str, coverage: &str) -> PathBuf {
    store.join("nodes").join(id).join(filename(coverage))
}
fn publish(store: &Path, owners: &Value, nodes: &[Value], surface: usize) {
    fs::create_dir_all(store.join("nodes")).unwrap();
    for n in nodes {
        let i = n["id"].as_str().unwrap();
        fs::create_dir_all(store.join("nodes").join(i)).unwrap();
        for coverage in ["full", "shallow"] {
            if children(n).is_empty() && (surface == 2 || surface == 1 && coverage == "shallow") {
                continue;
            }
            let p = file(store, i, coverage);
            fs::write(&p, owners[i][coverage]["text"].as_str().unwrap()).unwrap();
            fs::write(
                p.with_file_name(format!("{}.meta.json", filename(coverage))),
                serde_json::to_vec(&owners[i][coverage]["meta"]).unwrap(),
            )
            .unwrap();
        }
    }
}
fn check(
    nodes: &[Value],
    version: &str,
    store: &Path,
    surface: usize,
    bpe: &tiktoken_rs::CoreBPE,
) -> Result<(), String> {
    check_versions(nodes, version, store, surface, bpe, None)
}
fn check_versions(
    nodes: &[Value],
    version: &str,
    store: &Path,
    surface: usize,
    bpe: &tiktoken_rs::CoreBPE,
    versions: Option<&BTreeMap<PathBuf, String>>,
) -> Result<(), String> {
    let mut expected = expectations(nodes, version, bpe)?;
    if let Some(versions) = versions {
        for n in nodes {
            let i = n["id"].as_str().unwrap();
            for coverage in ["full", "shallow"] {
                let v = versions
                    .get(&file(store, i, coverage))
                    .ok_or("missing checked identity")?;
                if v.trim().is_empty() {
                    return Err("missing inputs".into());
                }
                let view = &mut expected.get_mut(i).unwrap()[coverage];
                view["document"]["source_version"] = json!(v);
                // Identity changes bytes/costs, not authored rows. Reprice full
                // views once; shallow bytes/costs are recomputed below.
                if coverage == "full" {
                    let text = render(&view["document"]);
                    view["text"] = json!(text);
                    view["meta"] = json!({"hash":sha(text.as_bytes()),
                        "tokens":count(&text,bpe),"encoding":"o200k_base"});
                }
            }
        }
        let costs: BTreeMap<_, _> = expected
            .iter()
            .map(|(i, v)| (i.clone(), v["full"]["meta"]["tokens"].clone()))
            .collect();
        for (i, views) in &mut expected {
            let doc = &mut views["shallow"]["document"];
            for row in doc["rows"].as_array_mut().unwrap() {
                row["full_view_tokens"] = costs[row["id"].as_str().unwrap()].clone();
            }
            let text = render(doc);
            let n = nodes.iter().find(|n| n["id"] == *i).unwrap();
            views["shallow"]["text"] = json!(text);
            views["shallow"]["meta"] = json!({"hash":sha(text.as_bytes()), "tokens":count(&text,bpe), "encoding":"o200k_base",
                "full_view_tokens":costs[i], "branch_full_tokens":children(n).iter().map(|c|(c.clone(),costs[c].clone())).collect::<BTreeMap<_,_>>()});
        }
    }
    let allowed = [
        "outline.lean",
        "outline.full.lean",
        "outline.lean.meta.json",
        "outline.full.lean.meta.json",
    ];
    let disk: BTreeSet<_> = fs::read_dir(store.join("nodes"))
        .map_err(|e| e.to_string())?
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    if disk != expected.keys().cloned().collect() {
        return Err("node directories differ".into());
    }
    for n in nodes {
        let i = n["id"].as_str().unwrap();
        let dir = store.join("nodes").join(i);
        if !fs::symlink_metadata(&dir)
            .map_err(|e| e.to_string())?
            .is_dir()
        {
            return Err("not directory".into());
        }
        for e in fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let name = e.unwrap().file_name().to_string_lossy().into_owned();
            if name.to_lowercase().starts_with("outline") && !allowed.contains(&name.as_str()) {
                return Err("unrecognised generated file".into());
            }
        }
        for coverage in ["full", "shallow"] {
            let p = file(store, i, coverage);
            let m = p.with_file_name(format!("{}.meta.json", filename(coverage)));
            if !p.exists() {
                if !children(n).is_empty() {
                    return Err("missing owner artifact".into());
                }
                if m.exists() {
                    return Err("orphan sidecar".into());
                }
                continue;
            }
            if children(n).is_empty() && (surface == 2 || surface == 1 && coverage == "shallow") {
                return Err("leaf surface".into());
            }
            let text = fs::read_to_string(&p).map_err(|e| e.to_string())?;
            if text != expected[i][coverage]["text"].as_str().unwrap() {
                return Err("artifact differs".into());
            }
            let meta: Value = serde_json::from_slice(&fs::read(&m).map_err(|_| "missing sidecar")?)
                .map_err(|e| e.to_string())?;
            if meta != expected[i][coverage]["meta"] {
                return Err("sidecar differs".into());
            }
        }
    }
    Ok(())
}
fn assembler_output(supplied: &[u8]) -> std::process::Output {
    let bin = std::env::var_os("CORE_OUTLINE_FOREST_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/outline-forest-observe")
        });
    assert!(
        bin.is_file(),
        "build the actual outline-forest-observe binary first: {}",
        bin.display()
    );
    capacity::observer_output(Command::new(bin), supplied.to_vec())
}
fn assembler(nodes: &[Value], version: &str) -> Value {
    let out = assembler_output(
        &serde_json::to_vec(&json!({"source_version":version,"nodes":nodes})).unwrap(),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v.as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["owners", "source_version"]
    );
    assert_eq!(v["source_version"], version);
    assert_eq!(
        v["owners"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>(),
        nodes
            .iter()
            .map(|n| n["id"].as_str().unwrap().to_owned())
            .collect()
    );
    v["owners"].clone()
}
#[test]
fn shared_oracle_tokenizer_preserves_concurrent_encodings() {
    let direct = tiktoken_rs::o200k_base().unwrap();
    let corpus = [
        "",
        "hello world",
        "Ω漢字\n\"quoted\"\\\t",
        "full_view_tokens:123\n",
    ];
    let expected: Vec<_> = corpus
        .iter()
        .map(|text| direct.encode_ordinary(text))
        .collect();
    std::thread::scope(|scope| {
        for _ in 0..16 {
            let expected = &expected;
            scope.spawn(move || {
                let bpe = ORACLE_BPE.clone();
                assert!(std::sync::Arc::ptr_eq(&bpe, &ORACLE_BPE));
                for (text, ids) in corpus.iter().zip(expected) {
                    assert_eq!(&bpe.encode_ordinary(text), ids);
                    assert_eq!(bpe.decode(ids.clone()).unwrap(), *text);
                }
            });
        }
    });
}

#[test]
fn assembler_files_match_independent_authored_expectations_and_all_controls() {
    let bpe = ORACLE_BPE.clone();
    let mut moved = forest();
    for n in &mut moved {
        if n["id"] == id(10) {
            n["children"] = json!([]);
        }
        if n["id"] == id(6) {
            n["children"] = json!([id(12)]);
        }
    }
    let mut reordered = forest();
    reordered.rotate_left(7);
    let roots = [(13,"Second root","in_progress",vec![14]), (1,"Root","in_progress",vec![12]),
        (14,"Second leaf","resolved",vec![]), (7,"Third root","stuck",vec![]), (12,"Leaf","not_examined",vec![])]
        .into_iter().map(|(i,name,status,cs)| {
            let mut n=json!({"id":id(i),"name":name,"status":status,"micro_summary":format!("{name} owns a bounded navigation branch."),
                "scope":"NEVER IN AN OUTLINE","children":cs.into_iter().map(id).collect::<Vec<_>>()});
            if status=="resolved" {n["content"]=json!("Answered.");}
            if status=="stuck" {n["blocker"]=json!("Nothing else is reachable from here.");} n
        }).collect::<Vec<_>>();
    let mut baseline = None;
    for (nodes, version) in [
        (forest(), "files-deep-wide-v1"),
        (forest(), "  files-padded-v1  "),
        (reordered, "files-deep-wide-v1"),
        (moved, "files-moved-v1"),
        (roots, "files-multiple-roots-v1"),
    ] {
        let owners = assembler(&nodes, version);
        for surface in 0..3 {
            let dir = TempDir::new().unwrap();
            publish(dir.path(), &owners, &nodes, surface);
            let before = snapshot(dir.path());
            check(&nodes, version, dir.path(), surface, &bpe).unwrap();
            assert_eq!(snapshot(dir.path()), before, "checker must be read-only");
            if surface == 0 && version == "files-deep-wide-v1" {
                if let Some(ref original) = baseline {
                    assert_eq!(
                        &before, original,
                        "input-array order must not change file bytes"
                    );
                } else {
                    baseline = Some(before);
                }
                assert_eq!(
                    check(&nodes, version, dir.path(), 1, &bpe).unwrap_err(),
                    "leaf surface"
                );
                assert_eq!(
                    check(&nodes, version, dir.path(), 2, &bpe).unwrap_err(),
                    "leaf surface"
                );
            }
        }
    }
    let nodes = forest();
    let version = "files-deep-wide-v1";
    let expected = expectations(&nodes, version, &bpe).unwrap();
    let owners = assembler(&nodes, version);
    // Re-seal each content mutation's exact bytes and token count. Equality must
    // still reject it: agreement between a publisher and its sidecar is insufficient.
    for case in [
        "omitted-row",
        "wrong-version",
        "changed-warning",
        "changed-reason",
        "inactive-reason",
        "changed-order",
        "changed-count",
        "changed-depth",
        "changed-name",
        "changed-summary",
        "forbidden-row-content",
        "leaked-authored-scope",
        "unexpected-document-key",
    ] {
        let dir = TempDir::new().unwrap();
        publish(dir.path(), &owners, &nodes, 0);
        let mut doc = expected[&id(1)]["full"]["document"].clone();
        match case {
            "omitted-row" => {
                doc["rows"].as_array_mut().unwrap().pop();
            }
            "wrong-version" => doc["source_version"] = json!("planted-v9"),
            "changed-warning" => doc["rows"][1]["warnings"] = json!("planted"),
            "changed-reason" => doc["rows"][2]["discard_reason"] = json!("planted"),
            "inactive-reason" => doc["rows"][2]["blocker"] = json!("planted"),
            "changed-order" => doc["rows"].as_array_mut().unwrap().swap(1, 2),
            "changed-count" => doc["rows"][0]["descendants"] = json!(99),
            "changed-depth" => doc["rows"][1]["depth"] = json!(99),
            "changed-name" => doc["rows"][1]["name"] = json!("planted"),
            "changed-summary" => doc["rows"][1]["micro_summary"] = json!("planted"),
            "forbidden-row-content" => doc["rows"][1]["ruled_out"] = json!("planted"),
            "leaked-authored-scope" => doc["rows"][1]["scope"] = json!("planted"),
            _ => doc["chunking"] = json!("none"),
        }
        let text = render(&doc);
        let p = file(dir.path(), &id(1), "full");
        fs::write(&p, &text).unwrap();
        fs::write(p.with_file_name("outline.full.lean.meta.json"),serde_json::to_vec(&json!({"hash":sha(text.as_bytes()),"tokens":count(&text,&bpe),"encoding":"o200k_base"})).unwrap()).unwrap();
        assert_eq!(
            check(&nodes, version, dir.path(), 0, &bpe).unwrap_err(),
            "artifact differs",
            "{case}"
        );
    }
    for (case, key, value) in [
        ("wrong-hash", "hash", json!("wrong")),
        ("wrong-token", "tokens", json!(999)),
        ("wrong-encoding", "encoding", json!("cl100k_base")),
        ("wrong-branch-cost", "branch_full_tokens", json!({})),
        ("wrong-owner-cost", "full_view_tokens", json!(999)),
        ("leaf-cost-drift", "branch_full_tokens", json!({id(12):999})),
        ("existing-branch-cost-drift", "branch_full_tokens", {
            let mut costs = expected[&id(8)]["shallow"]["meta"]["branch_full_tokens"].clone();
            costs[id(10)] = json!(costs[&id(10)].as_u64().unwrap() + 1);
            costs
        }),
        ("existing-leaf-cost-drift", "branch_full_tokens", {
            let mut costs = expected[&id(10)]["shallow"]["meta"]["branch_full_tokens"].clone();
            costs[id(12)] = json!(costs[&id(12)].as_u64().unwrap() + 3);
            costs
        }),
    ] {
        let dir = TempDir::new().unwrap();
        publish(dir.path(), &owners, &nodes, 0);
        let p = dir
            .path()
            .join("nodes")
            .join(id(if case == "existing-leaf-cost-drift" {
                10
            } else {
                8
            }))
            .join("outline.lean.meta.json");
        let mut meta: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        meta[key] = value;
        fs::write(&p, serde_json::to_vec(&meta).unwrap()).unwrap();
        assert_eq!(
            check(&nodes, version, dir.path(), 0, &bpe).unwrap_err(),
            "sidecar differs",
            "{case}"
        );
    }
    for (case, reason) in [
        ("omitted-owner", "node directories differ"),
        ("omitted-file", "missing owner artifact"),
        ("omitted-sidecar", "missing sidecar"),
        ("orphan-sidecar", "orphan sidecar"),
        ("absent-publication", "missing owner artifact"),
        ("stale-owner-directory", "node directories differ"),
        ("display-name-directory", "node directories differ"),
        ("unrecognised-generated-file", "unrecognised generated file"),
        ("leaf-full-holds-shallow", "artifact differs"),
    ] {
        let dir = TempDir::new().unwrap();
        publish(dir.path(), &owners, &nodes, 0);
        let p = file(dir.path(), &id(8), "full");
        match case {
            "omitted-owner" => fs::remove_dir_all(p.parent().unwrap()).unwrap(),
            "omitted-file" => fs::remove_file(&p).unwrap(),
            "omitted-sidecar" => {
                fs::remove_file(p.with_file_name("outline.full.lean.meta.json")).unwrap()
            }
            "orphan-sidecar" => fs::remove_file(file(dir.path(), &id(12), "full")).unwrap(),
            "absent-publication" => {
                for n in &nodes {
                    let d = dir.path().join("nodes").join(n["id"].as_str().unwrap());
                    for e in fs::read_dir(d).unwrap() {
                        fs::remove_file(e.unwrap().path()).unwrap();
                    }
                }
            }
            "stale-owner-directory" | "display-name-directory" => fs::create_dir(
                dir.path()
                    .join("nodes")
                    .join(if case == "display-name-directory" {
                        "Wide".into()
                    } else {
                        id(99)
                    }),
            )
            .unwrap(),
            "unrecognised-generated-file" => {
                fs::copy(&p, p.with_file_name("outline.chunk.lean")).unwrap();
            }
            _ => {
                let source = file(dir.path(), &id(12), "shallow");
                fs::copy(&source, file(dir.path(), &id(12), "full")).unwrap();
                fs::copy(
                    source.with_file_name("outline.lean.meta.json"),
                    file(dir.path(), &id(12), "full").with_file_name("outline.full.lean.meta.json"),
                )
                .unwrap();
            }
        }
        assert_eq!(
            check(&nodes, version, dir.path(), 0, &bpe).unwrap_err(),
            reason,
            "{case}"
        );
    }
    let dir = TempDir::new().unwrap();
    publish(dir.path(), &owners, &nodes, 0);
    for case in [
        "planted-name",
        "planted-order",
        "planted-summary",
        "planted-version",
        "refuses-blank-summary",
        "refuses-blank-reason",
        "refuses-blank-resolved",
    ] {
        let mut wrong = nodes.clone();
        let n = wrong.iter_mut().find(|n| n["id"] == id(8)).unwrap();
        match case {
            "planted-name" => n["name"] = json!("planted"),
            "planted-order" => n["children"].as_array_mut().unwrap().reverse(),
            "planted-summary" => n["micro_summary"] = json!("planted"),
            "refuses-blank-summary" => n["micro_summary"] = json!(" "),
            "refuses-blank-reason" => {
                n["status"] = json!("stuck");
                n["blocker"] = json!(" ");
            }
            "refuses-blank-resolved" => {
                n["status"] = json!("resolved");
                n["content"] = json!(" ");
            }
            _ => (),
        }
        assert!(
            check(
                &wrong,
                if case == "planted-version" {
                    "wrong"
                } else {
                    version
                },
                dir.path(),
                0,
                &bpe
            )
            .is_err(),
            "{case}"
        );
    }
}
