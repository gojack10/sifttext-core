// Native producer only supplies freshness/identity, never semantic expectations.
// Exact deep/wide/two-root authored corpus from successor/outlines.mjs.
fn special_text_specimen() -> Vec<Value> {
    let strange = "Ω漢字 👩🏽‍💻 \"quote\" \\ slash\ttab\nline\r\u{1}";
    let ident = |n| format!("90000000-0000-4000-8000-{n:012}");
    (1..=18)
        .map(|n| {
            let cs = match n {
                1 => vec![2, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                2..=6 => vec![n + 1],
                17 => vec![18],
                _ => vec![],
            };
            let status = match n {
                7 => "resolved",
                9 => "discarded",
                10 => "stuck",
                11 => "deferred",
                _ => "in_progress",
            };
            let mut node = json!({"id":ident(n),"name":format!("Node {n} {strange}"),
            "micro_summary":format!("Summary {n} {strange}"),"status":status,
            "children":cs.into_iter().map(ident).collect::<Vec<_>>(),
            "content":format!("Body {n} excluded.\n"),"scope":"excluded scope",
            "see_also":[],"warnings":strange});
            if let Some(reason) = active(status) {
                node[reason] = json!(strange);
            }
            node
        })
        .collect()
}
#[test]
fn native_special_text_corpus_cold_warm_and_moved_full_equality() {
    let bpe = ORACLE_BPE.clone();
    let dir = TempDir::new().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let store = root.join("store");
    let nodes = special_text_specimen();
    assert_eq!(nodes.len(), 18);
    materialize(&store, &nodes);
    let authored = authored_snapshot(&store);
    let roots = || {
        fs::read_dir(store.join("trees"))
            .unwrap()
            .map(|e| {
                let p = e.unwrap().path();
                (p.file_name().unwrap().to_owned(), fs::read_link(p).unwrap())
            })
            .collect::<BTreeMap<_, _>>()
    };
    let root_links = roots();
    assert_eq!(root_links.len(), 2);
    let (ok, pending) = native(&store, "outline_inspect", None);
    assert!(!ok);
    assert_eq!(pending["status"], "pending");
    assert_eq!(pending["version"], Value::Null);
    assert_eq!(pending["files"], json!([]));
    let verify = |version: &str| {
        let independent = capacity::identities(&capacity::inventory(&store)).0;
        assert_eq!(
            version, independent,
            "global identity is authored, not native"
        );
        assert!(versions(&store, "planted-global-v9").is_err());
        assert_eq!(versions(&store, version).unwrap().len(), 36);
        check_native(&store, version, &bpe);
        let db = rusqlite::Connection::open_with_flags(
            store.join(".sifttext/index.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let indexed_version: String = db
            .query_row("select value from metadata where key='version'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(indexed_version, version);
        let mut stmt = db.prepare("select record from nodes order by id").unwrap();
        let indexed = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| serde_json::from_str::<Value>(&r.unwrap()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(indexed, authored_records(&store));
        assert_eq!(roots(), root_links);
        generated_snapshot(&store)
    };
    let full = refresh(&store, "full");
    let initial = verify(&full);
    assert_eq!(authored_snapshot(&store), authored);
    let warm = refresh(&store, "incremental");
    assert_eq!(warm, full);
    assert_eq!(verify(&warm), initial);
    let ident = |n| format!("90000000-0000-4000-8000-{n:012}");
    let strange = "Ω漢字 👩🏽‍💻 \"quote\" \\ slash\ttab\nline\r\u{1}";
    change_field(
        &store,
        &ident(7),
        "name",
        json!(format!("Renamed {strange}")),
    );
    change_field(
        &store,
        &ident(7),
        "micro_summary",
        json!(format!("Edited {strange}")),
    );
    fs::rename(
        store
            .join("nodes")
            .join(ident(5))
            .join("children")
            .join(ident(6)),
        store
            .join("nodes")
            .join(ident(8))
            .join("children")
            .join(ident(6)),
    )
    .unwrap();
    for (parent, cs) in [
        (5, vec![]),
        (8, vec![6]),
        (1, vec![16, 15, 14, 13, 12, 11, 10, 9, 8, 2]),
    ] {
        fs::write(
            store
                .join("nodes")
                .join(ident(parent))
                .join("children.order"),
            cs.into_iter()
                .map(|c| format!("{}\n", ident(c)))
                .collect::<String>(),
        )
        .unwrap();
    }
    let (ok, stale) = native(&store, "outline_inspect", None);
    assert!(!ok);
    assert_eq!(stale["status"], "stale");
    assert_eq!(stale["version"], Value::Null);
    assert_eq!(stale["files"], json!([]));
    let moved = refresh(&store, "incremental");
    assert_ne!(moved, full);
    let changed = verify(&moved);
    assert_ne!(
        changed[&PathBuf::from(format!("nodes/{}/outline.full.lean", ident(5)))],
        initial[&PathBuf::from(format!("nodes/{}/outline.full.lean", ident(5)))]
    );
    let rebuilt = refresh(&store, "full");
    assert_eq!(rebuilt, moved);
    assert_eq!(verify(&rebuilt), changed);
}
// Native invalid-source controls retained from outlines.mjs, not merely
// candidate-validation refusals. Neither CLI surface may advertise old output.
#[test]
fn native_invalid_outline_sources_refuse_without_repair_or_publication() {
    let bpe = ORACLE_BPE.clone();
    let dir = TempDir::new().unwrap();
    let store = fs::canonicalize(dir.path()).unwrap().join("store");
    materialize(&store, &special_text_specimen());
    let baseline = refresh(&store, "full");
    let generated = generated_snapshot(&store);
    let db_path = store.join(".sifttext/index.sqlite");
    let db = fs::read(&db_path).unwrap();
    let source = store.join("nodes/90000000-0000-4000-8000-000000000007/node.md");
    let bytes = fs::read(&source).unwrap();
    let root = store.join("trees/90000000-0000-4000-8000-000000000017");
    let target = fs::read_link(&root).unwrap();
    for label in [
        "empty-summary",
        "whitespace-summary",
        "malformed",
        "disconnected",
    ] {
        match label {
            "empty-summary" | "whitespace-summary" => change_field(
                &store,
                "90000000-0000-4000-8000-000000000007",
                "micro_summary",
                json!(if label == "empty-summary" { "" } else { " \t" }),
            ),
            "malformed" => fs::write(&source, "---\nmalformed").unwrap(),
            "disconnected" => fs::remove_file(&root).unwrap(),
            _ => unreachable!(),
        }
        let invalid = authored_snapshot(&store);
        for (op, mode) in [("index", Some("full")), ("outline_inspect", None)] {
            let (ok, view) = native(&store, op, mode);
            assert!(!ok, "{label}/{op}: invalid source exited zero");
            assert_eq!(view["status"], "error", "{label}/{op}");
            assert_eq!(view["version"], Value::Null, "{label}/{op}");
            dogfood_empty_unavailable(&view, op);
            assert_eq!(
                authored_snapshot(&store),
                invalid,
                "{label}/{op}: repaired source"
            );
            assert_eq!(
                generated_snapshot(&store),
                generated,
                "{label}/{op}: published output"
            );
            assert_eq!(
                fs::read(&db_path).unwrap(),
                db,
                "{label}/{op}: changed SQLite"
            );
            if label == "disconnected" {
                assert!(!root.exists(), "{op}: repaired disconnected root");
            } else {
                assert_eq!(fs::read_link(&root).unwrap(), target);
            }
        }
        fs::write(&source, &bytes).unwrap();
        if label == "disconnected" {
            std::os::unix::fs::symlink(&target, &root).unwrap();
        }
    }
    assert_eq!(refresh(&store, "incremental"), baseline);
    assert_eq!(generated_snapshot(&store), generated);
    check_native(&store, &baseline, &bpe);
}
fn specimen() -> Vec<Value> {
    let ident = |i| format!("10000000-0000-4000-8000-{i:012}");
    [(1,"Root","navigation root","in_progress",vec![2,3,4]),
     (2,"alpha login","first branch","resolved",vec![]),
     (3,"beta reset","second branch","deferred",vec![5]),
     (4,"gamma","rejected branch","discarded",vec![]),
     (5,"delta login reset","deep leaf","stuck",vec![]),
     (6,"Other root","second tree","in_progress",vec![7]),
     (7,"login","outside branch","not_examined",vec![])]
    .into_iter().enumerate().map(|(index,(i,name,micro,status,cs))| {
        let mut n=json!({"id":ident(i),"name":name,"micro_summary":micro,"status":status,
            "scope":format!("Scope {index}"),"content":format!("Body {index} login reset. Next sentence.\n"),
            "children":cs.into_iter().map(ident).collect::<Vec<_>>(),"see_also":[]});
        match i {
            2 => n["see_also"]=json!([{"target":ident(5),"reason":"A needs D\nKeep reason."}]),
            3 => n["deferred_reason"]=json!("Wait; resume after check."),
            4 => n["discard_reason"]=json!("Rejected example.\nNever repeat\tthis."),
            5 => { n["blocker"]=json!("External wait."); n["warnings"]=json!("Never lose\nUnicode Ω\tquotes \" or \\."); },
            7 => n["see_also"]=json!([{"target":ident(3),"reason":"Outside points to B."}]), _ => ()
        } n
    }).collect()
}
fn materialize(store: &Path, nodes: &[Value]) {
    use std::os::unix::fs::symlink;
    fs::create_dir_all(store.join("trees")).unwrap();
    let child_ids: BTreeSet<_> = nodes.iter().flat_map(children).collect();
    for n in nodes {
        let i = n["id"].as_str().unwrap();
        let d = store.join("nodes").join(i);
        fs::create_dir_all(d.join("children")).unwrap();
        let fields = n
            .as_object()
            .unwrap()
            .iter()
            .filter(|(k, _)| !["children", "content"].contains(&k.as_str()))
            .map(|(k, v)| format!("{k}: {v}\n"))
            .collect::<String>();
        fs::write(
            d.join("node.md"),
            format!("---\n{fields}---\n{}", n["content"].as_str().unwrap()),
        )
        .unwrap();
        fs::write(
            d.join("children.order"),
            children(n)
                .iter()
                .map(|c| format!("{c}\n"))
                .collect::<String>(),
        )
        .unwrap();
        for c in children(n) {
            symlink(format!("../../{c}"), d.join("children").join(c)).unwrap();
        }
        if !child_ids.contains(i) {
            symlink(format!("../nodes/{i}"), store.join("trees").join(i)).unwrap();
        }
    }
}
fn authored_records(store: &Path) -> Vec<Value> {
    let mut out = vec![];
    for entry in fs::read_dir(store.join("nodes")).unwrap() {
        let d = entry.unwrap().path();
        let p = d.join("node.md");
        if !p.is_file() {
            continue;
        }
        let text = fs::read_to_string(p).unwrap();
        assert!(text.starts_with("---\n"));
        let (fields, body) = text[4..].split_once("\n---\n").unwrap();
        let mut n = json!({});
        for line in fields.lines().filter(|l| !l.is_empty()) {
            let (k, v) = line.split_once(':').unwrap();
            n[k.trim()] = serde_json::from_str(v).unwrap();
        }
        n["content"] = json!(body);
        n["children"] = json!(
            fs::read_to_string(d.join("children.order"))
                .unwrap()
                .lines()
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
        );
        out.push(n);
    }
    out.sort_by_key(|n| n["id"].as_str().unwrap().to_owned());
    out
}
fn authored_snapshot(store: &Path) -> BTreeMap<PathBuf, String> {
    snapshot(store)
        .into_iter()
        .filter(|(p, _)| {
            !p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("outline")
                && p.file_name().unwrap() != "node.meta.json"
        })
        .collect()
}
fn generated_snapshot(store: &Path) -> BTreeMap<PathBuf, String> {
    snapshot(store)
        .into_iter()
        .filter(|(p, _)| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("outline")
        })
        .collect()
}
fn native(store: &Path, op: &str, mode: Option<&str>) -> (bool, Value) {
    let mut req = json!({"op":op,"store":store});
    if let Some(m) = mode {
        req["mode"] = json!(m);
    }
    native_request(req)
}
fn native_request(req: Value) -> (bool, Value) {
    let op = req["op"].as_str().unwrap();
    let bin = std::env::var_os("CORE_OBSERVE_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/core-observe"));
    assert!(bin.is_file(), "build core-observe first: {}", bin.display());
    let mut command = Command::new(&bin);
    command.current_dir(bin.parent().unwrap().parent().unwrap().parent().unwrap());
    let input = serde_json::to_vec(&req).unwrap();
    let out = capacity::observer_output(command, input);
    let v = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{op}: {e}; stderr {}", String::from_utf8_lossy(&out.stderr)));
    (out.status.success(), v)
}
fn refresh(store: &Path, mode: &str) -> String {
    let before = authored_snapshot(store);
    let (ok, v) = native(store, "index", Some(mode));
    assert!(ok, "{v}");
    assert_eq!(v["status"], "current");
    assert_eq!(
        authored_snapshot(store),
        before,
        "refresh changed authored bytes"
    );
    let ver = v["version"].as_str().unwrap();
    assert!(!ver.trim().is_empty());
    ver.into()
}
fn versions(store: &Path, global: &str) -> Result<BTreeMap<PathBuf, String>, String> {
    let (ok, v) = native(store, "outline_inspect", None);
    if !ok {
        return Err("not current at expected global version".into());
    }
    inspected_versions(store, global, &v)
}
fn inspected_versions(
    store: &Path,
    global: &str,
    v: &Value,
) -> Result<BTreeMap<PathBuf, String>, String> {
    if v["status"] != "current" || v["version"] != global {
        return Err("not current at expected global version".into());
    }
    let wanted: BTreeSet<_> = authored_records(store)
        .iter()
        .flat_map(|n| {
            [
                "outline.full.lean",
                "outline.lean",
                "outline.full.lean.meta.json",
                "outline.lean.meta.json",
            ]
            .map(|name| {
                store
                    .join("nodes")
                    .join(n["id"].as_str().unwrap())
                    .join(name)
            })
        })
        .collect();
    let mut paths = BTreeSet::new();
    let mut versions = BTreeMap::new();
    for f in v["files"].as_array().unwrap() {
        let p = PathBuf::from(f["path"].as_str().unwrap());
        if !wanted.contains(&p) || !paths.insert(p.clone()) {
            return Err("duplicate or unexpected inspection path".into());
        }
        let text = fs::read_to_string(&p).unwrap();
        assert_eq!(f["text"], text);
        assert_eq!(f["hash"], sha(text.as_bytes()));
        if p.extension().is_some_and(|e| e == "lean") {
            let ident = text
                .lines()
                .find_map(|l| l.strip_prefix("source_version:"))
                .unwrap();
            let ident = if ident.starts_with('"') {
                serde_json::from_str::<String>(ident).unwrap()
            } else {
                ident.into()
            };
            if ident.trim().is_empty() {
                return Err("blank identity".into());
            }
            versions.insert(p, ident);
        }
    }
    if versions.is_empty() || paths != wanted {
        return Err("incomplete inspected artifacts".into());
    }
    Ok(versions)
}
#[test]
fn inspected_outline_paths_require_exact_authored_sidecar_set() {
    let dir = TempDir::new().unwrap();
    let store = fs::canonicalize(dir.path()).unwrap();
    materialize(&store, &specimen());
    let mut files = vec![];
    for n in authored_records(&store) {
        for name in [
            "outline.full.lean",
            "outline.lean",
            "outline.full.lean.meta.json",
            "outline.lean.meta.json",
        ] {
            let p = store
                .join("nodes")
                .join(n["id"].as_str().unwrap())
                .join(name);
            let text = if name.ends_with(".lean") {
                "source_version:file-identity\n"
            } else {
                "{}\n"
            };
            fs::write(&p, text).unwrap();
            files.push(json!({"path":p,"text":text,"hash":sha(text.as_bytes())}));
        }
    }
    let valid = json!({"status":"current","version":"global","files":files});
    assert_eq!(
        inspected_versions(&store, "global", &valid).unwrap().len(),
        14
    );
    let before = snapshot(&store);
    for control in ["missing", "duplicate", "substitute", "stale", "escaped"] {
        let mut v = valid.clone();
        match control {
            "missing" => {
                v["files"].as_array_mut().unwrap().pop();
            }
            "duplicate" => {
                v["files"][3] = v["files"][2].clone();
            }
            "substitute" => {
                // Preserve count, readable text and hash: only exact path coverage is wrong.
                let p = store
                    .join("nodes")
                    .join(specimen()[0]["id"].as_str().unwrap())
                    .join("node.md");
                let text = fs::read_to_string(&p).unwrap();
                v["files"][3] = json!({"path":p,"text":text,"hash":sha(text.as_bytes())});
            }
            "stale" => {
                v["version"] = json!("old");
            }
            "escaped" => {
                v["files"][3]["path"] = json!("/outside/outline.lean.meta.json");
            }
            _ => unreachable!(),
        }
        assert!(
            inspected_versions(&store, "global", &v).is_err(),
            "{control}"
        );
        assert_eq!(snapshot(&store), before, "{control}: checker changed store");
    }
}
fn check_native(store: &Path, global: &str, bpe: &tiktoken_rs::CoreBPE) {
    check_native_using(store, global, bpe, || {
        let (ok, reply) = native(store, "outline_inspect", None);
        assert!(ok, "{reply}");
        reply
    });
}
fn check_native_using(
    store: &Path,
    global: &str,
    bpe: &tiktoken_rs::CoreBPE,
    inspect: impl FnOnce() -> Value,
) {
    let before = snapshot(store);
    let vs = inspected_versions(store, global, &inspect()).unwrap();
    check_versions(&authored_records(store), global, store, 0, bpe, Some(&vs)).unwrap();
    assert_eq!(snapshot(store), before, "checker changed store");
}
fn change_field(store: &Path, i: &str, key: &str, value: Value) {
    let p = store.join("nodes").join(i).join("node.md");
    let text = fs::read_to_string(&p).unwrap();
    let prefix = format!("{key}:");
    assert!(text.lines().any(|l| l.starts_with(&prefix)));
    fs::write(
        p,
        text.lines()
            .map(|l| {
                if l.starts_with(&prefix) {
                    format!("{key}: {value}\n")
                } else {
                    format!("{l}\n")
                }
            })
            .collect::<String>(),
    )
    .unwrap();
}
// Re-seal bytes, exact counts, sidecar and SQLite manifest, not just a bad hash.
fn reseal_file_identity(store: &Path, owner: &str, bpe: &tiktoken_rs::CoreBPE) {
    let p = file(store, owner, "full");
    let text = fs::read_to_string(&p).unwrap();
    assert!(text.lines().any(|l| l.starts_with("source_version:")));
    let text = text
        .lines()
        .map(|l| {
            if l.starts_with("source_version:") {
                "source_version:planted-file-identity-v9"
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(&p, &text).unwrap();
    let mp = p.with_file_name("outline.full.lean.meta.json");
    let mut meta: Value = serde_json::from_slice(&fs::read(&mp).unwrap()).unwrap();
    meta["hash"] = json!(sha(text.as_bytes()));
    meta["tokens"] = json!(count(&text, bpe));
    fs::write(&mp, serde_json::to_vec(&meta).unwrap()).unwrap();
    let db = rusqlite::Connection::open(store.join(".sifttext/index.sqlite")).unwrap();
    let raw: String = db
        .query_row("select value from metadata where key='outlines'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let mut manifest: Value = serde_json::from_str(&raw).unwrap();
    for p in [&p, &mp] {
        manifest["files"][p.strip_prefix(store).unwrap().to_str().unwrap()] =
            json!(sha(&fs::read(p).unwrap()));
    }
    assert_eq!(
        db.execute(
            "update metadata set value=? where key='outlines'",
            [manifest.to_string()]
        )
        .unwrap(),
        1
    );
}
#[test]
fn native_index_files_freshness_resealed_refusal_and_authored_transitions() {
    let bpe = ORACLE_BPE.clone();
    let dir = TempDir::new().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let store = root.join("store");
    materialize(&store, &specimen());
    let initial = authored_records(&store);
    assert_eq!(initial.len(), 7);
    let full = refresh(&store, "full");
    check_native(&store, &full, &bpe);
    let baseline = generated_snapshot(&store);
    assert!(versions(&store, "planted-global-v9").is_err());
    // Re-seal bytes, exact counts, sidecar and SQLite manifest, not just a bad hash.
    let forged = root.join("forged");
    assert!(
        Command::new("cp")
            .args(["-R"])
            .arg(&store)
            .arg(&forged)
            .status()
            .unwrap()
            .success()
    );
    let owner = initial.iter().find(|n| !children(n).is_empty()).unwrap()["id"]
        .as_str()
        .unwrap();
    reseal_file_identity(&forged, owner, &bpe);
    let (ok, rejected) = native(&forged, "outline_inspect", None);
    assert!(!ok);
    assert_eq!(rejected["status"], "stale");
    assert!(versions(&forged, &full).is_err());
    assert_eq!(authored_snapshot(&forged), authored_snapshot(&store));
    let inc = refresh(&store, "incremental");
    assert_eq!(inc, full);
    assert_eq!(generated_snapshot(&store), baseline);
    check_native(&store, &inc, &bpe);
    let child_ids: BTreeSet<_> = initial.iter().flat_map(children).collect();
    let ren = initial
        .iter()
        .find(|n| {
            !children(n).is_empty()
                && child_ids.contains(n["id"].as_str().unwrap())
                && n["status"] != "resolved"
        })
        .unwrap();
    let ident = ren["id"].as_str().unwrap();
    change_field(&store, ident, "name", json!("Renamed node Ω"));
    let renamed = refresh(&store, "incremental");
    assert_ne!(renamed, full);
    check_native(&store, &renamed, &bpe);
    let parent = initial
        .iter()
        .find(|n| children(n).contains(&ident.to_owned()))
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let text = fs::read_to_string(file(&store, parent, "full")).unwrap();
    assert!(text.contains("Renamed node Ω"));
    assert!(!text.contains(ren["name"].as_str().unwrap()));
    let before = authored_records(&store);
    let reorder = before.iter().find(|n| children(n).len() > 1).unwrap();
    let ident = reorder["id"].as_str().unwrap();
    let mut order = children(reorder);
    order.reverse();
    fs::write(
        store.join("nodes").join(ident).join("children.order"),
        order.iter().map(|c| format!("{c}\n")).collect::<String>(),
    )
    .unwrap();
    let reordered = refresh(&store, "incremental");
    assert_ne!(reordered, renamed);
    check_native(&store, &reordered, &bpe);
    let before = authored_records(&store);
    let old = before
        .iter()
        .find(|n| {
            children(n)
                .iter()
                .any(|c| children(before.iter().find(|n| n["id"] == *c).unwrap()).is_empty())
        })
        .unwrap();
    let child = children(old)
        .into_iter()
        .find(|c| children(before.iter().find(|n| n["id"] == *c).unwrap()).is_empty())
        .unwrap();
    let old_id = old["id"].as_str().unwrap();
    let by = before
        .iter()
        .map(|n| (n["id"].as_str().unwrap().to_owned(), n.clone()))
        .collect();
    let mut excluded = vec![];
    walk(&by, old_id, 0, &mut excluded, &mut BTreeSet::new()).unwrap();
    let new = before
        .iter()
        .find(|n| {
            !children(n).is_empty()
                && !excluded.iter().any(|(i, _)| n["id"] == *i)
                && n["status"] != "resolved"
        })
        .unwrap();
    let new_id = new["id"].as_str().unwrap();
    fs::rename(
        store
            .join("nodes")
            .join(old_id)
            .join("children")
            .join(&child),
        store
            .join("nodes")
            .join(new_id)
            .join("children")
            .join(&child),
    )
    .unwrap();
    for (n, add) in [(old, false), (new, true)] {
        let mut cs = children(n);
        cs.retain(|c| c != &child);
        if add {
            cs.push(child.clone());
        }
        fs::write(
            store
                .join("nodes")
                .join(n["id"].as_str().unwrap())
                .join("children.order"),
            cs.iter().map(|c| format!("{c}\n")).collect::<String>(),
        )
        .unwrap();
    }
    let moved = refresh(&store, "incremental");
    assert_ne!(moved, reordered);
    check_native(&store, &moved, &bpe);
    let before = generated_snapshot(&store);
    let final_version = refresh(&store, "full");
    assert_eq!(final_version, moved);
    check_native(&store, &final_version, &bpe);
    assert_eq!(generated_snapshot(&store), before);
    for node in authored_records(&store) {
        for marker in ["source_version:", "path_template:", "full_view_tokens"] {
            assert!(
                !node["content"].as_str().unwrap().contains(marker),
                "generated marker {marker} copied into authored node {}",
                node["id"]
            );
        }
    }
}
