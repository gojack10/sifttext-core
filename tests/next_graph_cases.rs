// Q1 independent expected transition cases, exercised against the real library
// and the checked Bend policy compiled from laws/Spec.bend at this revision.
//
// Every expectation below is read from fixtures/next-core/cases.json, which is
// hand-prescribed data (see that file's `provenance` block and
// tests/fixture_prescriptions.rs). Nothing in this file asks the
// implementation what it thinks the answer should be and then accepts it: the
// implementation only produces the candidate state, which is compared against
// the prescribed state.
//
// Q2 adaptation: the same prescriptions are materialised in the selected
// node.md + trees/ representation. Only the spelling and materialisation
// changed; the expected UUID graphs, requests, preservation inventories, the 58
// nearby wrong-result controls and the 11 draft verdicts are the Q1 ones.
//
// Scope: the existing move/reorder/rename slice only. These tests are native
// integration evidence for this bounded fixture. They are not proof of the
// specification's adequacy, not publication or sandbox evidence, and not a
// claim about the Markdown grammar beyond what this layout requires.

use sifttext_file_graph::{Authored, Entry, Operation, Policy, Reference, Store, apply, parse_markdown, validate_draft};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

fn doc() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/next-core/cases.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn labels() -> BTreeMap<String, String> {
    serde_json::from_value(doc()["labels"].clone()).unwrap()
}

/// Resolve the `{LABEL}` tokens used by the fixture prescriptions.
fn tok(s: &str, l: &BTreeMap<String, String>) -> String {
    let mut out = s.to_string();
    for (k, v) in l {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    assert!(!out.contains('{'), "unresolved label token in {s}");
    out
}

fn children_of(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

fn order_bytes(ids: &[String]) -> Vec<u8> {
    if ids.is_empty() {
        Vec::new()
    } else {
        (ids.join("\n") + "\n").into_bytes()
    }
}

// --- Markdown representation helpers (fixture materialisation only) ---------
// These write the selected node.md grammar the way an ordinary editor would.
// They are not a second parser: the store under test re-reads every byte.

/// The canonical front-matter spelling used to author this fixture: one
/// `key: compact-JSON-value` line per field, then the verbatim body.
fn render_markdown(a: &Authored) -> Vec<u8> {
    let mut out = String::from("---\n");
    for (key, value) in [
        ("id", serde_json::to_string(&a.id).unwrap()),
        ("name", serde_json::to_string(&a.name).unwrap()),
        ("micro_summary", serde_json::to_string(&a.micro_summary).unwrap()),
        ("see_also", serde_json::to_string(&a.see_also).unwrap()),
    ] {
        out.push_str(&format!("{key}: {value}\n"));
    }
    out.push_str("---\n");
    out.push_str(&a.content);
    out.into_bytes()
}

/// Byte offset just past the closing metadata fence, if the file has one.
fn body_start(text: &str) -> usize {
    let mut offset = 0;
    let mut fences = 0;
    for line in text.split_inclusive('\n') {
        if line == "---\n" || line == "---\r\n" {
            fences += 1;
            if fences == 2 {
                return offset + line.len();
            }
        }
        offset += line.len();
    }
    panic!("fixture node.md must carry both metadata fences");
}

/// The byte span of the `name` metadata value, exactly as an editor sees it:
/// the value text after the first colon with surrounding spaces/tabs trimmed.
fn name_value_span(text: &str) -> std::ops::Range<usize> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\n', '\r']);
        if let Some((key, value)) = bare.split_once(':')
            && key.trim_matches([' ', '\t']) == "name"
        {
            let start = offset + bare.find(':').unwrap() + 1 + value.len()
                - value.trim_start_matches([' ', '\t']).len();
            return start..start + value.trim_matches([' ', '\t']).len();
        }
        offset += line.len();
    }
    panic!("fixture node.md must carry a name metadata line");
}

/// An ordinary native rename: replace only the name value with a compact JSON
/// string and touch no other byte. This is the byte form the interface requires.
fn native_rename_md(bytes: &[u8], name: &str) -> Vec<u8> {
    let mut text = std::str::from_utf8(bytes).unwrap().to_owned();
    let span = name_value_span(&text);
    text.replace_range(span, &serde_json::to_string(name).unwrap());
    text.into_bytes()
}

/// Same graph, deliberately different bytes: re-emit the parsed fields in a
/// different metadata order with extra spacing and a blank metadata line.
fn reserialise_md(bytes: &[u8], changes: &serde_json::Map<String, Value>) -> Vec<u8> {
    let mut a = parse_markdown(std::str::from_utf8(bytes).unwrap()).unwrap();
    apply_changes(&mut a, changes);
    let mut out = String::from("---\n\n");
    for (key, value) in [
        ("see_also", serde_json::to_string(&a.see_also).unwrap()),
        ("micro_summary", serde_json::to_string(&a.micro_summary).unwrap()),
        ("id", serde_json::to_string(&a.id).unwrap()),
        ("name", serde_json::to_string(&a.name).unwrap()),
    ] {
        out.push_str(&format!("{key}:  {value}\n"));
    }
    out.push_str("---\n");
    out.push_str(&a.content);
    out.into_bytes()
}

fn apply_changes(a: &mut Authored, changes: &serde_json::Map<String, Value>) {
    if let Some(name) = changes.get("name") {
        a.name = name.as_str().unwrap().to_owned();
    }
    if let Some(summary) = changes.get("micro_summary") {
        a.micro_summary = summary.as_str().unwrap().to_owned();
    }
    if let Some(content) = changes.get("content") {
        a.content = content.as_str().unwrap().to_owned();
    }
    if let Some(see_also) = changes.get("see_also") {
        a.see_also = serde_json::from_value(see_also.clone()).unwrap();
    }
}

fn temp(label: &str) -> TempDir {
    match std::env::var("SIFTTEXT_EVIDENCE_DIR") {
        Ok(p) => tempfile::Builder::new()
            .prefix(&format!("q1-next-core-{label}-"))
            .tempdir_in(p)
            .unwrap(),
        Err(_) => tempfile::tempdir().unwrap(),
    }
}

fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// Write the independently authored baseline into the selected node.md + trees/
/// physical layout.
fn materialize(root: &Path) {
    let l = labels();
    let d = doc();
    fs::create_dir_all(root.join("trees")).unwrap();
    let nodes = d["baseline"]["nodes"].as_object().unwrap();
    for (id, n) in nodes {
        let dir = root.join(format!("nodes/{id}"));
        fs::create_dir_all(dir.join("children")).unwrap();
        fs::write(dir.join("children/.keep"), []).unwrap();
        let authored = Authored {
            id: id.clone(),
            name: n["name"].as_str().unwrap().to_string(),
            micro_summary: n["micro_summary"].as_str().unwrap().to_string(),
            content: n["content"].as_str().unwrap().to_string(),
            see_also: serde_json::from_value::<Vec<Reference>>(n["see_also"].clone()).unwrap(),
        };
        fs::write(dir.join("node.md"), render_markdown(&authored)).unwrap();
        let children = children_of(&n["children"]);
        fs::write(dir.join("children.order"), order_bytes(&children)).unwrap();
        for child in &children {
            std::os::unix::fs::symlink(format!("../../{child}"), dir.join(format!("children/{child}"))).unwrap();
        }
    }
    for r in d["baseline"]["roots"].as_array().unwrap() {
        let id = r.as_str().unwrap();
        std::os::unix::fs::symlink(format!("../nodes/{id}"), root.join(format!("trees/{id}"))).unwrap();
    }
    for file in d["baseline"]["sentinels"].as_array().unwrap() {
        let path = root.join(file["path"].as_str().unwrap());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, file["content"].as_str().unwrap()).unwrap();
    }
    for file in d["baseline"]["node_resources"].as_array().unwrap() {
        let path = root.join(tok(file["path"].as_str().unwrap(), &l));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, file["content"].as_str().unwrap()).unwrap();
    }
}

fn get(root: &Path) -> Store {
    Store::read(root).unwrap()
}

/// Build the request exactly the way the CLI does: resolve identifiers against
/// the store, so an absent or ambiguous prefix is itself a rejection.
fn build_op(store: &Store, case: &Value) -> Result<Operation, String> {
    let l = labels();
    let req = &case["request"];
    let kind = req["operation"].as_str().unwrap();
    let node = req["node"].as_str().unwrap();
    let arg = req["arg"].as_str().unwrap();
    let node = store.resolve(&tok(node, &l)).map_err(|e| e.to_string())?;
    match kind {
        "move" => {
            let parent = store.resolve(&tok(arg, &l)).map_err(|e| e.to_string())?;
            Ok(Operation::Move { node, parent })
        }
        "reorder" => {
            let after = store.resolve(&tok(arg, &l)).map_err(|e| e.to_string())?;
            Ok(Operation::Reorder { node, after })
        }
        "rename" => Ok(Operation::Rename {
            node,
            name: tok(arg, &l),
        }),
        other => Err(format!("unsupported operation {other}")),
    }
}

/// Full comparable state, keyed by UUID, of everything the fixture prescribes.
type State = BTreeMap<String, (String, String, String, Vec<String>, Vec<(String, String)>)>;

fn state_of_store(s: &Store) -> State {
    s.nodes
        .iter()
        .map(|(id, n)| {
            (
                id.clone(),
                (
                    n.name.clone(),
                    n.micro_summary.clone(),
                    n.content.clone(),
                    s.children[id].clone(),
                    n.see_also.iter().map(|r| (r.target.clone(), r.reason.clone())).collect(),
                ),
            )
        })
        .collect()
}

/// The prescribed state for a case: the baseline with the case's authored overrides.
fn state_of_prescription(case: &Value) -> State {
    let d = doc();
    let l = labels();
    let mut state: State = BTreeMap::new();
    for (id, n) in d["baseline"]["nodes"].as_object().unwrap() {
        state.insert(
            id.clone(),
            (
                n["name"].as_str().unwrap().to_string(),
                n["micro_summary"].as_str().unwrap().to_string(),
                n["content"].as_str().unwrap().to_string(),
                children_of(&n["children"]),
                serde_json::from_value::<Vec<Reference>>(n["see_also"].clone())
                    .unwrap()
                    .into_iter()
                    .map(|r| (r.target, r.reason))
                    .collect(),
            ),
        );
    }
    if case["kind"].as_str() == Some("valid") {
        let overrides = case["expected"]["nodes"].as_object().unwrap();
        for (id, patch) in overrides {
            let id = tok(id, &l);
            let entry = state.get(&id).unwrap_or_else(|| panic!("unknown node {id}")).clone();
            let (name, summary, content, children, refs) = entry;
            state.insert(
                id,
                (
                    patch["name"].as_str().map(str::to_owned).unwrap_or(name),
                    patch["micro_summary"].as_str().map(str::to_owned).unwrap_or(summary),
                    patch["content"].as_str().map(str::to_owned).unwrap_or(content),
                    patch["children"].as_array().map(|_| children_of(&patch["children"])).unwrap_or(children),
                    patch["see_also"]
                        .as_array()
                        .map(|_| {
                            serde_json::from_value::<Vec<Reference>>(patch["see_also"].clone())
                                .unwrap()
                                .into_iter()
                                .map(|r| (r.target, r.reason))
                                .collect()
                        })
                        .unwrap_or(refs),
                ),
            );
        }
        for (id, n) in case["expected"]["nodes"].as_object().unwrap() {
            let id = tok(id, &l);
            if !state.contains_key(&id) && n["name"].is_string() {
                // A wrong-result control may introduce a node; never used for the prescribed state.
                panic!("prescribed state must not add node {id}");
            }
        }
    }
    state
}

fn roots_of(case: &Value) -> Vec<String> {
    let d = doc();
    let l = labels();
    if case["kind"].as_str() == Some("valid") && case["expected"]["roots"].is_array() {
        return case["expected"]["roots"].as_array().unwrap().iter().map(|v| tok(v.as_str().unwrap(), &l)).collect();
    }
    d["baseline"]["roots"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect()
}

fn changed_paths(before: &Store, after: &Store) -> Vec<String> {
    let keys: BTreeSet<PathBuf> = before.inventory.keys().chain(after.inventory.keys()).cloned().collect();
    keys.into_iter()
        .filter(|k| before.inventory.get(k) != after.inventory.get(k))
        .map(|k| k.to_string_lossy().into_owned())
        .collect()
}

fn prescribed_paths(case: &Value) -> Vec<String> {
    let l = labels();
    case["changed_paths"].as_array().unwrap().iter().map(|v| tok(v.as_str().unwrap(), &l)).collect()
}

fn cases(kind: &str) -> Vec<Value> {
    doc()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"].as_str() == Some(kind))
        .cloned()
        .collect()
}

fn short(p: &str) -> String {
    let mut out = String::new();
    for seg in p.split('/') {
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(&seg.chars().take(8).collect::<String>());
    }
    out
}

#[test]
fn prescribed_valid_requests_succeed_with_the_independent_expected_state() {
    let valid = cases("valid");
    assert!(valid.len() >= 5, "expected several valid cases, got {}", valid.len());
    for case in valid {
        let id = case["id"].as_str().unwrap().to_string();
        let dir = temp("valid");
        materialize(dir.path());
        let p = policy();
        let before = get(dir.path());
        p.validate(&before).unwrap();
        let op = build_op(&before, &case).unwrap_or_else(|e| panic!("{id}: request must resolve: {e}"));
        let after = apply(&p, &before, &op)
            .unwrap_or_else(|e| panic!("{id}: prescribed valid request was rejected: {e}"));

        assert_eq!(
            state_of_store(&after),
            state_of_prescription(&case),
            "{id}: graph keyed by UUID differs from the independently prescribed state"
        );
        assert_eq!(after.roots, roots_of(&case), "{id}: roots");
        assert_eq!(
            after.nodes.keys().collect::<Vec<_>>(),
            before.nodes.keys().collect::<Vec<_>>(),
            "{id}: identity inventory changed"
        );

        let mut got = changed_paths(&before, &after);
        got.sort();
        let mut want = prescribed_paths(&case);
        want.sort();
        assert_eq!(
            got,
            want,
            "{id}: changed filesystem paths differ from the prescribed inventory (got {:?})",
            got.iter().map(|p| short(&p.to_string())).collect::<Vec<_>>()
        );

        // Named preservation inventories, checked directly rather than only via the diff.
        let preserved = &case["preserved"];
        for v in preserved["canonical_dirs_unchanged"].as_array().unwrap_or(&Vec::new()).iter() {
            let dir = format!("nodes/{}/", tok(v.as_str().unwrap(), &labels()));
            assert!(
                !got.iter().any(|p| p.starts_with(&dir)),
                "{id}: canonical directory {dir} was touched"
            );
        }
        for v in preserved["asset_bytes_unchanged"].as_array().unwrap_or(&Vec::new()).iter() {
            let path = PathBuf::from(tok(v.as_str().unwrap(), &labels()));
            assert_eq!(
                before.inventory.get(&path),
                after.inventory.get(&path),
                "{id}: unrelated ordinary file {}",
                path.display()
            );
        }
        for v in preserved["child_entries_unchanged"].as_array().unwrap_or(&Vec::new()).iter() {
            let path = tok(v.as_str().unwrap(), &labels());
            assert_eq!(
                before.inventory.get(&PathBuf::from(&path)),
                after.inventory.get(&PathBuf::from(&path)),
                "{id}: child entry {path} was rewritten"
            );
        }
        for row in preserved["references_unchanged"].as_array().unwrap_or(&Vec::new()).iter() {
            let l = labels();
            let src = tok(row["source"].as_str().unwrap(), &l);
            let dst = tok(row["target"].as_str().unwrap(), &l);
            let reason = row["reason"].as_str().unwrap();
            let refs = after.nodes[&src].see_also.clone();
            assert!(
                refs.iter().any(|r| r.target == dst && r.reason == reason),
                "{id}: reasoned reference {src} -> {dst} lost or rewritten"
            );
        }
        for row in preserved["unrelated_sibling_order"].as_array().unwrap_or(&Vec::new()).iter() {
            let l = labels();
            let parent = tok(row["parent"].as_str().unwrap(), &l);
            let scope = row["unrelated"].as_array().map(|a| {
                a.iter()
                    .map(|v| tok(v.as_str().unwrap(), &l))
                    .collect::<BTreeSet<String>>()
            });
            let got_order: Vec<String> = after.children[&parent]
                .iter()
                .filter(|c| scope.as_ref().is_none_or(|s| s.contains(*c)))
                .cloned()
                .collect();
            let want_order: Vec<String> = row["order"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| tok(v.as_str().unwrap(), &l))
                .collect();
            assert_eq!(got_order, want_order, "{id}: unrelated sibling order under {parent}");
        }
        for row in preserved["descendants_carried"].as_array().unwrap_or(&Vec::new()).iter() {
            let l = labels();
            let node = tok(row["node"].as_str().unwrap(), &l);
            let want: Vec<String> = row["children"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| tok(v.as_str().unwrap(), &l))
                .collect();
            assert_eq!(after.children[&node], want, "{id}: moved subtree kept its own children");
        }

        // Derived navigation must reflect the prescribed display names.
        for row in case["expected"]["navigation"].as_array().unwrap_or(&Vec::new()).iter() {
            let l = labels();
            let src = tok(row["source"].as_str().unwrap(), &l);
            let dst = tok(row["target"].as_str().unwrap(), &l);
            let view = after.view();
            let node = view["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["id"] == Value::String(src.clone()))
                .unwrap_or_else(|| panic!("{id}: no view node for {src}"));
            let nav = node["navigation"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["target"] == Value::String(dst.clone()))
                .unwrap_or_else(|| panic!("{id}: no navigation entry {src} -> {dst}"));
            assert_eq!(nav["name"], row["name"], "{id}: navigation name {src} -> {dst}");
            assert_eq!(
                nav["micro_summary"], row["micro_summary"],
                "{id}: navigation micro_summary {src} -> {dst}"
            );
        }

        // Q2 representation fidelity: the Markdown store reports exact full-UUID
        // canonical paths, and a root shortcut is never part of a transition.
        let view = after.view();
        for row in view["nodes"].as_array().unwrap() {
            let nid = row["id"].as_str().unwrap();
            assert_eq!(
                row["path"],
                Value::String(after.path.join(format!("nodes/{nid}/node.md")).to_str().unwrap().to_owned()),
                "{id}: inspected path is not the canonical node.md location for {nid}"
            );
            for nav in row["navigation"].as_array().unwrap() {
                let target = nav["target"].as_str().unwrap();
                assert_eq!(
                    nav["path"],
                    Value::String(
                        after.path.join(format!("nodes/{target}/node.md")).to_str().unwrap().to_owned()
                    ),
                    "{id}: reference navigation path is not canonical for {target}"
                );
            }
        }
        for r in &after.roots {
            assert!(
                matches!(
                    after.inventory.get(&PathBuf::from(format!("trees/{r}"))),
                    Some(Entry::Link(_))
                ),
                "{id}: root shortcut trees/{r} is not a link in the inventory"
            );
        }
        assert!(
            !got.iter().any(|p| p.starts_with("trees/")),
            "{id}: a trees/ root shortcut changed"
        );
        println!("PASS {id}: prescribed valid request reproduced the independent expected state");
    }
}

#[test]
fn prescribed_invalid_requests_reject_and_preserve_every_byte() {
    let invalid = cases("invalid");
    assert!(invalid.len() >= 7, "expected several invalid cases, got {}", invalid.len());
    for case in invalid {
        let id = case["id"].as_str().unwrap().to_string();
        let dir = temp("invalid");
        materialize(dir.path());
        let p = policy();
        let before = get(dir.path());
        p.validate(&before).unwrap();
        let original = before.inventory.clone();
        let outcome = build_op(&before, &case).and_then(|op| {
            apply(&p, &before, &op).map(|_| ()).map_err(|e| e.to_string())
        });
        assert!(outcome.is_err(), "{id}: prescribed invalid request was accepted");
        let now = get(dir.path());
        assert_eq!(now.inventory, original, "{id}: rejected request mutated the store");
        assert_eq!(
            state_of_store(&now),
            state_of_prescription(&case),
            "{id}: rejected request changed the prescribed graph"
        );
        assert!(prescribed_paths(&case).is_empty(), "{id}: invalid case must prescribe no changed paths");
        println!("PASS {id}: rejected without mutation ({})", case["reason_class"]);
    }
}

#[test]
fn prescribed_drafts_are_accepted_or_rejected_by_exact_comparison() {
    let l = labels();
    let drafts = doc()["drafts"].as_array().unwrap().clone();
    assert!(drafts.len() >= 7, "expected several draft controls, got {}", drafts.len());
    let accepted = drafts.iter().filter(|d| d["expect"] == "accepted").count();
    assert!(accepted >= 3, "expected at least three accepted drafts, got {accepted}");
    for draft in &drafts {
        let id = draft["id"].as_str().unwrap().to_string();
        let case = doc()["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"].as_str() == Some(draft["case"].as_str().unwrap()))
            .unwrap_or_else(|| panic!("{id}: unknown case"))
            .clone();
        let a = temp("draft-before");
        let b = temp("draft-after");
        materialize(a.path());
        materialize(b.path());
        let before = get(a.path());
        for edit in draft["edits"].as_array().unwrap() {
            let op = edit["op"].as_str().unwrap();
            if op == "mv" {
                let from = b.path().join(tok(edit["from"].as_str().unwrap(), &l));
                let to = b.path().join(tok(edit["to"].as_str().unwrap(), &l));
                let ok = Command::new("mv").arg(&from).arg(&to).status().unwrap().success();
                assert!(ok, "{id}: native mv failed");
            } else if op == "write_order" {
                let path = b.path().join(tok(edit["path"].as_str().unwrap(), &l));
                let children: Vec<String> = edit["children"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| tok(v.as_str().unwrap(), &l))
                    .collect();
                fs::write(path, order_bytes(&children)).unwrap();
            } else if op == "write_node_md" {
                let path = b.path().join(tok(edit["path"].as_str().unwrap(), &l));
                let original = fs::read(&path).unwrap();
                let changes = edit["changes"].as_object().unwrap();
                let bytes = match edit["byte_form"].as_str().unwrap() {
                    // Exact helper-equivalent candidate bytes: only the name value.
                    "native_rename" => {
                        let mut bytes = native_rename_md(&original, changes["name"].as_str().unwrap());
                        if let Some(content) = changes.get("content") {
                            let mut text = String::from_utf8(bytes).unwrap();
                            text.truncate(body_start(&text));
                            text.push_str(content.as_str().unwrap());
                            bytes = text.into_bytes();
                        }
                        bytes
                    }
                    // Same graph, different bytes.
                    "reserialised" => reserialise_md(&original, changes),
                    other => panic!("{id}: unsupported byte form {other}"),
                };
                fs::write(path, bytes).unwrap();
            } else if op == "append_bytes" {
                use std::io::Write;
                let path = b.path().join(tok(edit["path"].as_str().unwrap(), &l));
                let mut file = fs::OpenOptions::new().append(true).open(path).unwrap();
                file.write_all(edit["content"].as_str().unwrap().as_bytes()).unwrap();
            } else {
                panic!("{id}: unsupported edit op {op}");
            }
        }
        let after = Store::read(b.path());
        let op = build_op(&before, &case).unwrap_or_else(|e| panic!("{id}: request must resolve: {e}"));
        let outcome = match &after {
            Ok(store) => validate_draft(&policy(), &before, store, &op)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Err(e) => Err(format!("draft store is unreadable: {e}")),
        };
        if draft["expect"].as_str() == Some("accepted") {
            outcome.unwrap_or_else(|e| panic!("{id}: ordinary native draft rejected: {e}"));
            assert_eq!(
                state_of_store(after.as_ref().unwrap()),
                state_of_prescription(&case),
                "{id}: accepted draft must equal the prescribed state"
            );
            println!("PASS {id}: ordinary native draft matches the checked operation");
        } else {
            let reason = outcome.expect_err(&format!("{id}: nearby wrong draft was accepted as the checked operation"));
            println!("PASS {id}: nearby wrong draft rejected ({reason}) - {}", draft["why"]);
        }
    }
}

#[test]
fn nearby_wrong_expected_results_are_detected_in_the_actual_result() {
    let l = labels();
    let p = policy();
    let cases = doc()["cases"].as_array().unwrap().clone();
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get().min(4));
    std::thread::scope(|scope| {
        let handles = (0..workers)
            .map(|worker| {
                let (cases, l, p) = (&cases, &l, &p);
                scope.spawn(move || check_wrong_results(cases, l, p, worker, workers))
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().unwrap();
        }
    });
}

fn check_wrong_results(
    cases: &[Value],
    l: &BTreeMap<String, String>,
    p: &Policy,
    worker: usize,
    workers: usize,
) {
    for case in cases.iter().skip(worker).step_by(workers) {
        let id = case["id"].as_str().unwrap().to_string();
        let dir = temp("wrong");
        materialize(dir.path());
        let before = get(dir.path());
        let actual = match build_op(&before, case) {
            Ok(op) => match apply(p, &before, &op) {
                Ok(after) => (state_of_store(&after), changed_paths(&before, &after)),
                // A rejection leaves the store untouched; for invalid cases that is
                // exactly the prescribed outcome, and an unexpected acceptance here
                // would make the next assertion fail.
                Err(_) => (state_of_store(&before), vec![]),
            },
            Err(_) => (state_of_store(&before), vec![]),
        };
        let (actual_state, mut got_paths) = actual;
        got_paths.sort();
        let want_state = state_of_prescription(case);
        assert_eq!(
            actual_state, want_state,
            "{id}: actual result must equal the prescribed state before testing its wrong controls"
        );
        assert_eq!(
            got_paths,
            {
                let mut v = prescribed_paths(case);
                v.sort();
                v
            },
            "{id}: actual changed-path inventory differs from the prescription"
        );

        let mut rejected = 0;
        let mut vacuous = 0;
        let mut unresolved = 0;
        for control in case["wrong_results"].as_array().unwrap() {
            let cid = control["id"].as_str().unwrap();
            let patch = &control["patch"];
            let mut wrong = want_state.clone();
            if let Some(nodes) = patch["nodes"].as_object() {
                for (key, fields) in nodes {
                    let key = tok(key, l);
                    let base = wrong.get(&key).cloned();
                    match base {
                        Some((name, summary, content, children, refs)) => {
                            wrong.insert(
                                key.clone(),
                                (
                                    fields["name"].as_str().map(str::to_owned).unwrap_or(name),
                                    fields["micro_summary"].as_str().map(str::to_owned).unwrap_or(summary),
                                    fields["content"].as_str().map(str::to_owned).unwrap_or(content),
                                    fields["children"].as_array().map(|_| children_of(&fields["children"])).unwrap_or(children),
                                    fields["see_also"].as_array().map(|_| {
                                        serde_json::from_value::<Vec<Reference>>(fields["see_also"].clone())
                                            .unwrap()
                                            .into_iter()
                                            .map(|r| (r.target, r.reason))
                                            .collect::<Vec<_>>()
                                    }).unwrap_or(refs),
                                ),
                            );
                        }
                        // A control may only add a whole node (identity churn), never edit one blindly.
                        None if fields["name"].is_string() => {
                            wrong.insert(
                                key.clone(),
                                (
                                    fields["name"].as_str().unwrap().to_string(),
                                    fields["micro_summary"].as_str().unwrap_or_default().to_string(),
                                    fields["content"].as_str().unwrap_or_default().to_string(),
                                    children_of(&fields["children"]),
                                    serde_json::from_value::<Vec<Reference>>(fields["see_also"].clone())
                                        .unwrap_or_default()
                                        .into_iter()
                                        .map(|r| (r.target, r.reason))
                                        .collect(),
                                ),
                            );
                        }
                        None => panic!("{id} / {cid}: control targets an unknown node {key}"),
                    }
                }
            }
            let extra: Vec<String> = patch["changed_paths_extra"]
                .as_array()
                .map(|a| a.iter().map(|v| tok(v.as_str().unwrap(), l)).collect())
                .unwrap_or_default();
            let want_roots = roots_of(case);
            let wrong_roots: Vec<String> = patch["roots"]
                .as_array()
                .map(|a| a.iter().map(|v| tok(v.as_str().unwrap(), l)).collect())
                .unwrap_or_else(|| want_roots.clone());

            if wrong != want_state || wrong_roots != want_roots {
                // The control really changes something the actual result must not have.
                for key in wrong.keys() {
                    if wrong.get(key) != want_state.get(key) {
                        assert_ne!(
                            actual_state.get(key),
                            wrong.get(key),
                            "{id} / {cid}: the actual result matches this vacuous control"
                        );
                    }
                }
                // Materialise the wrong state as an ordinary private draft: the checked
                // comparison must refuse to call it the result of the prescribed request.
                let draft = temp("wrongdraft");
                write_draft(draft.path(), &wrong, &wrong_roots);
                let outcome = match Store::read(draft.path()) {
                    Ok(store) => match build_op(&before, case) {
                        Ok(op) => Some(validate_draft(p, &before, &store, &op).is_err()),
                        // The prescribed request itself does not resolve, so there is no
                        // checked result for this draft to differ from; the per-node
                        // assertion above already proved the control is not vacuous.
                        Err(_) => None,
                    },
                    // The wrong state is not even a readable store.
                    Err(_) => Some(true),
                };
                match outcome {
                    Some(true) => rejected += 1,
                    Some(false) => vacuous += 1,
                    None => unresolved += 1,
                }
            }
            if !extra.is_empty() {
                let got: BTreeSet<String> = got_paths.iter().cloned().collect();
                for path in &extra {
                    assert!(
                        !got.contains(path),
                        "{id} / {cid}: the actual result really did change {path}, so this inventory claim is not a control"
                    );
                }
            }
        }
        let graph_controls = case["wrong_results"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["patch"]["nodes"].is_object() || c["patch"]["roots"].is_array())
            .count();
        assert_eq!(
            vacuous, 0,
            "{id}: {vacuous} of {graph_controls} graph controls were accepted as the checked result"
        );
        println!(
            "PASS {id}: {rejected} nearby wrong states rejected as drafts, {unresolved} controls with an unresolvable request held, inventory controls held"
        );
    }
}

/// Write a whole (possibly wrong) state into the node.md + trees/ layout so
/// the checked comparison can judge it as an ordinary private draft.
fn write_draft(root: &Path, state: &State, roots: &[String]) {
    fs::create_dir_all(root.join("trees")).unwrap();
    for (id, (name, summary, content, children, refs)) in state {
        let dir = root.join(format!("nodes/{id}"));
        fs::create_dir_all(dir.join("children")).unwrap();
        fs::write(dir.join("children/.keep"), []).unwrap();
        let authored = Authored {
            id: id.clone(),
            name: name.clone(),
            micro_summary: summary.clone(),
            content: content.clone(),
            see_also: refs.iter().map(|(target, reason)| Reference { target: target.clone(), reason: reason.clone() }).collect(),
        };
        fs::write(dir.join("node.md"), render_markdown(&authored)).unwrap();
        fs::write(dir.join("children.order"), order_bytes(children)).unwrap();
        for entry in fs::read_dir(dir.join("children")).unwrap().flatten() {
            let name = entry.file_name();
            if name == ".keep" || !children.contains(&name.to_string_lossy().into_owned()) {
                let _ = fs::remove_file(entry.path());
            }
        }
        for child in children {
            let link = dir.join(format!("children/{child}"));
            if link.symlink_metadata().is_err() {
                std::os::unix::fs::symlink(format!("../../{child}"), link).unwrap();
            }
        }
    }
    for entry in fs::read_dir(root.join("trees")).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !roots.contains(&name) {
            let _ = fs::remove_file(entry.path());
        }
    }
    for r in roots {
        let link = root.join(format!("trees/{r}"));
        if link.symlink_metadata().is_err() {
            fs::create_dir_all(root.join("trees")).unwrap();
            std::os::unix::fs::symlink(format!("../nodes/{r}"), link).unwrap();
        }
    }
}

#[test]
fn baseline_itself_is_a_valid_store_and_the_tests_are_not_vacuous() {
    let dir = temp("baseline");
    materialize(dir.path());
    let p = policy();
    let before = get(dir.path());
    p.validate(&before).unwrap();
    assert_eq!(before.nodes.len(), 9);
    assert_eq!(before.roots.len(), 2);
    // A deliberate violating state must fail the same checks the cases rely on.
    let l = labels();
    // Violation A: the authored sequence no longer matches the authored child entry.
    let a = temp("violating");
    materialize(a.path());
    fs::write(
        a.path().join(format!("nodes/{}/children.order", l["T"])),
        order_bytes(&[l["Q"].clone(), l["U"].clone(), l["W"].clone()]),
    )
    .unwrap();
    assert!(
        Store::read(a.path()).is_err(),
        "dropping an authored child entry from the sequence must not read as a valid store"
    );
    assert_eq!(
        fs::read(a.path().join("attachments/alpha.txt")).unwrap(),
        fs::read(dir.path().join("attachments/alpha.txt")).unwrap(),
        "unrelated ordinary file bytes are unaffected by the violation"
    );

    // Violation B: a well-formed store whose authored reference points at nothing.
    let b = temp("violating-ref");
    materialize(b.path());
    let path = b.path().join(format!("nodes/{}/node.md", l["Q"]));
    let mut node = parse_markdown(&fs::read_to_string(&path).unwrap()).unwrap();
    node.see_also[0].target = "deadbeef-0000-4000-8000-0000000000ff".to_string();
    fs::write(&path, render_markdown(&node)).unwrap();
    let violating = get(b.path());
    assert!(
        p.validate(&violating).is_err(),
        "a dangling authored reference must not pass the checked policy"
    );
    let entry = violating.inventory.get(&PathBuf::from(format!(
        "nodes/{}/children/{}",
        l["T"], l["P"]
    )));
    assert!(matches!(entry, Some(Entry::Link(_))), "child entry should still exist on disk");
    assert_eq!(
        fs::read(b.path().join("notes/beta.md")).unwrap(),
        fs::read(dir.path().join("notes/beta.md")).unwrap(),
        "unrelated ordinary file bytes are unaffected by the dangling reference"
    );
}
