//! Independent prescription coherence, not product/runtime evidence. No Core API or helper process.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const FIELDS: [&str; 5] = ["name", "micro_summary", "content", "children", "see_also"];
fn rows(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn text(v: &Value) -> &str {
    v.as_str().unwrap()
}
fn strings(v: &Value) -> Vec<String> {
    rows(v).iter().map(|v| text(v).to_owned()).collect()
}
fn set(xs: Vec<String>) -> BTreeSet<String> {
    xs.into_iter().collect()
}
fn same_inventory(a: &[String], b: &[String]) -> bool {
    let mut a = a.to_vec();
    let mut b = b.to_vec();
    a.sort();
    b.sort();
    a == b
}
fn keys(v: &Value) -> BTreeSet<String> {
    v.as_object().unwrap().keys().cloned().collect()
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
fn payload_path(path: &str) -> bool {
    let parts: Vec<_> = path.split('/').collect();
    parts.len() == 3 && parts[0] == "nodes" && uuid(parts[1]) && parts[2] == "node.md"
}
fn expand(v: &Value, labels: &Value) -> Value {
    let token = |s: &str| {
        let mut s = s.to_owned();
        for (k, v) in labels.as_object().unwrap() {
            s = s.replace(&format!("{{{k}}}"), text(v));
        }
        assert!(!s.contains('{'), "unresolved token: {s}");
        s
    };
    match v {
        Value::String(s) => json!(token(s)),
        Value::Array(a) => Value::Array(a.iter().map(|v| expand(v, labels)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (token(k), expand(v, labels)))
                .collect(),
        ),
        _ => v.clone(),
    }
}
fn baseline(d: &Value) -> Value {
    json!({"roots":d["baseline"]["roots"], "nodes":d["baseline"]["nodes"]})
}
fn overlay(s: &mut Value, patch: &Value) {
    if let Some(nodes) = patch["nodes"].as_object() {
        for (id, fields) in nodes {
            if s["nodes"].get(id).is_none() {
                s["nodes"][id] = json!({"name":"x","micro_summary":"x","content":"x","children":[],"see_also":[]});
            }
            for (k, v) in fields.as_object().unwrap() {
                s["nodes"][id][k] = v.clone();
            }
        }
    }
    if patch["roots"].is_array() {
        s["roots"] = patch["roots"].clone();
    }
}
fn expected(d: &Value, c: &Value) -> Value {
    let mut s = baseline(d);
    if c["kind"] == "valid" {
        overlay(&mut s, &c["expected"]);
    }
    s
}
fn parents(s: &Value) -> BTreeMap<String, String> {
    let mut p: BTreeMap<_, _> = keys(&s["nodes"])
        .into_iter()
        .map(|id| (id, "none".to_owned()))
        .collect();
    for (id, n) in s["nodes"].as_object().unwrap() {
        for child in strings(&n["children"]) {
            p.insert(child, id.clone());
        }
    }
    p
}
fn valid(s: &Value) -> bool {
    let ids = keys(&s["nodes"]);
    let roots = strings(&s["roots"]);
    let mut seen_children = BTreeSet::new();
    for (id, n) in s["nodes"].as_object().unwrap() {
        if !uuid(id) || keys(n) != FIELDS.map(str::to_owned).into_iter().collect() {
            return false;
        }
        for f in ["name", "micro_summary", "content"] {
            if n[f].as_str().is_none_or(|s| s.trim().is_empty()) {
                return false;
            }
        }
        if !n["children"].is_array() || !n["see_also"].is_array() {
            return false;
        }
        for c in strings(&n["children"]) {
            if !ids.contains(&c) || !seen_children.insert(c) {
                return false;
            }
        }
        for r in rows(&n["see_also"]) {
            if !r.is_object() || keys(r) != set(vec!["reason".into(), "target".into()]) {
                return false;
            }
            let Some(target) = r["target"].as_str() else {
                return false;
            };
            if !uuid(target)
                || target == id
                || !ids.contains(target)
                || r["reason"].as_str().is_none_or(|s| s.trim().is_empty())
            {
                return false;
            }
        }
    }
    if roots.len() != set(roots.clone()).len()
        || roots
            .iter()
            .any(|r| !ids.contains(r) || seen_children.contains(r))
    {
        return false;
    }
    if ids
        .difference(&seen_children)
        .cloned()
        .collect::<BTreeSet<_>>()
        != set(roots)
    {
        return false;
    }
    let p = parents(s);
    for id in &ids {
        let mut seen = BTreeSet::new();
        let mut current = id.as_str();
        while current != "none" {
            if !seen.insert(current) {
                return false;
            }
            current = &p[current];
        }
    }
    true
}
fn paths(before: &Value, after: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (id, n) in after["nodes"].as_object().unwrap() {
        if before["nodes"][id]["children"] != n["children"] {
            out.insert(format!("nodes/{id}/children.order"));
        }
        if ["name", "micro_summary", "content", "see_also"]
            .iter()
            .any(|f| before["nodes"][id][f] != n[*f])
        {
            out.insert(format!("nodes/{id}/node.md"));
        }
    }
    let entries = |s: &Value| -> BTreeSet<String> {
        s["nodes"]
            .as_object()
            .unwrap()
            .iter()
            .flat_map(|(id, n)| {
                strings(&n["children"])
                    .into_iter()
                    .map(move |c| format!("nodes/{id}/children/{c}"))
            })
            .collect()
    };
    out.extend(
        entries(before)
            .symmetric_difference(&entries(after))
            .cloned(),
    );
    out
}
fn resolve(s: &Value, id: &str) -> Option<String> {
    if uuid(id) {
        return Some(id.to_owned());
    }
    let found: Vec<_> = keys(&s["nodes"])
        .into_iter()
        .filter(|k| k.starts_with(id))
        .collect();
    (found.len() == 1).then(|| found[0].clone())
}
fn covered(s: &Value, list: &Value) -> BTreeSet<String> {
    if list == &json!(["*"]) {
        keys(&s["nodes"])
    } else {
        set(strings(list))
    }
}
fn distance(a: &Value, b: &Value, ap: &[String], bp: &[String]) -> usize {
    keys(&a["nodes"])
        .union(&keys(&b["nodes"]))
        .filter(|id| a["nodes"][*id] != b["nodes"][*id])
        .count()
        + usize::from(a["roots"] != b["roots"])
        + usize::from(!same_inventory(ap, bp))
}
// Exact complete expected-state equality is stronger than the legacy named-field
// battery for wrong controls; coherence/placement/completeness are checked separately.
fn accepts(d: &Value, c: &Value, s: &Value, declared: &[String]) -> bool {
    valid(s)
        && *s == expected(d, c)
        && same_inventory(
            declared,
            &paths(&baseline(d), s).into_iter().collect::<Vec<_>>(),
        )
        && (c["kind"] != "invalid" || declared.is_empty())
}
// serde_json's default map sorts keys; format validated JSON tokens instead so
// this byte-form check retains the author's object order without a new dependency.
fn canonical(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if bytes[i] == b'"' {
            i += 1;
            while bytes[i] != b'"' {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
        } else if b"{}[],:".contains(&bytes[i]) {
            i += 1;
        } else {
            while i < bytes.len()
                && !bytes[i].is_ascii_whitespace()
                && !b"{}[],:".contains(&bytes[i])
            {
                i += 1;
            }
        }
        let token = &raw[start..i];
        if !b"{}[],:".contains(&bytes[start]) {
            let value: Value = serde_json::from_str(token).unwrap();
            assert_eq!(serde_json::to_string(&value).unwrap(), token);
        }
        tokens.push(token);
    }
    // Parsing drops duplicate keys; formatting the original tokens alone would
    // incorrectly preserve them as canonical. Every colon names one object key.
    fn object_keys(value: &Value) -> usize {
        match value {
            Value::Object(o) => o.len() + o.values().map(object_keys).sum::<usize>(),
            Value::Array(a) => a.iter().map(object_keys).sum(),
            _ => 0,
        }
    }
    assert_eq!(
        tokens.iter().filter(|t| **t == ":").count(),
        object_keys(&serde_json::from_str(raw).unwrap()),
        "duplicate object key"
    );
    // JSON.stringify enumerates array-index keys first, in ascending order,
    // then other keys in authored order (2^32-1 is not an array index).
    let mut objects: Vec<Option<(Option<u32>, bool)>> = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        match *token {
            "{" => objects.push(Some((None, false))),
            "[" => objects.push(None),
            "}" | "]" => {
                objects.pop().unwrap();
            }
            ":" => {
                let key: String = serde_json::from_str(tokens[i - 1]).unwrap();
                let index = key
                    .parse::<u32>()
                    .ok()
                    .filter(|n| *n != u32::MAX && n.to_string() == key);
                let (previous, ordinary) = objects.last_mut().unwrap().as_mut().unwrap();
                if let Some(index) = index {
                    assert!(
                        !*ordinary && previous.is_none_or(|p| p < index),
                        "noncanonical array-index key order"
                    );
                    *previous = Some(index);
                } else {
                    *ordinary = true;
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    let mut depth = 0;
    for (i, token) in tokens.iter().enumerate() {
        match *token {
            "{" | "[" => {
                out.push_str(token);
                depth += 1;
                if !matches!(tokens[i + 1], "}" | "]") {
                    out.push('\n');
                    out.push_str(&"  ".repeat(depth));
                }
            }
            "}" | "]" => {
                depth -= 1;
                if !matches!(tokens[i - 1], "{" | "[") {
                    out.push('\n');
                    out.push_str(&"  ".repeat(depth));
                }
                out.push_str(token);
            }
            "," => {
                out.push_str(",\n");
                out.push_str(&"  ".repeat(depth));
            }
            ":" => out.push_str(": "),
            _ => out.push_str(token),
        }
    }
    out.push('\n');
    out
}
#[test]
fn canonical_requires_array_index_key_order() {
    for raw in [
        r#"{"2":"b","1":"a"}"#,
        r#"{"z":[],"0":"a"}"#,
        r#"[{"3":{},"2":{}}]"#,
    ] {
        assert!(std::panic::catch_unwind(|| canonical(raw)).is_err());
    }
    assert_eq!(
        canonical(
            r#"{"0":"a","2":{"1":"b","z":[]},"4294967294":"c","z":{},"01":"d","4294967295":"e"}"#
        ),
        "{\n  \"0\": \"a\",\n  \"2\": {\n    \"1\": \"b\",\n    \"z\": []\n  },\n  \"4294967294\": \"c\",\n  \"z\": {},\n  \"01\": \"d\",\n  \"4294967295\": \"e\"\n}\n"
    );
}
#[test]
fn canonical_rejects_duplicate_object_keys() {
    for raw in [
        r#"{"a":1,"a":1}"#,
        r#"{"a":[{"x":0,"x":1}]}"#,
        r#"{"a":[{"x":0,"\u0078":1}]}"#,
    ] {
        assert!(std::panic::catch_unwind(|| canonical(raw)).is_err());
    }
    assert_eq!(
        canonical(r#"{"a":[{"x":1},{"x":2}],"b":"colon: stays text"}"#),
        "{\n  \"a\": [\n    {\n      \"x\": 1\n    },\n    {\n      \"x\": 2\n    }\n  ],\n  \"b\": \"colon: stays text\"\n}\n"
    );
}
#[test]
fn inventory_comparisons_preserve_multiplicity() {
    let a = vec!["a".to_owned(), "b".to_owned()];
    assert!(same_inventory(&a, &["b".into(), "a".into()]));
    assert!(!same_inventory(&a, &["a".into(), "a".into(), "b".into()]));
    assert!(!same_inventory(&a, &["a".into(), "a".into()]));
}
#[test]
fn payload_paths_require_exact_uuid_layout() {
    let id = "11111111-1111-1111-1111-111111111111";
    assert!(payload_path(&format!("nodes/{id}/node.md")));
    for path in [
        "nodes/not-a-uuid/node.md".to_owned(),
        format!("nodes/{id}/extra/node.md"),
        format!("other/{id}/node.md"),
        format!("nodes/{id}/node.json"),
    ] {
        assert!(!payload_path(&path), "{path}");
    }
}
#[test]
fn fixture_prescriptions_are_coherent_complete_and_discriminating() {
    let raw = include_str!("../fixtures/next-core/cases.json");
    let original: Value = serde_json::from_str(raw).unwrap();
    assert_eq!(canonical(raw), raw);
    assert_eq!(
        canonical("{\"z\":[],\"a\":{\"x\":\"Ω\\\\n\"}}"),
        "{\n  \"z\": [],\n  \"a\": {\n    \"x\": \"Ω\\\\n\"\n  }\n}\n"
    );
    assert_ne!(canonical("{\"z\": 1}"), "{\"z\": 1}");
    let mut d = original.clone();
    for field in ["baseline", "cases", "drafts"] {
        d[field] = expand(&original[field], &original["labels"]);
    }
    let base = baseline(&d);
    let l = &d["labels"];
    assert_eq!(d["schema"], "sifttext.next-core.cases/1");
    assert!(same_inventory(
        &strings(&d["scope"]["operations"]),
        &["move".into(), "rename".into(), "reorder".into()]
    ));
    assert_eq!(d["provenance"]["implementation_used_as_oracle"], false);
    let ids: BTreeSet<_> = l
        .as_object()
        .unwrap()
        .values()
        .map(|v| text(v).to_owned())
        .collect();
    assert_eq!(ids.len(), 9);
    assert!(ids.iter().all(|s| uuid(s)));
    assert_eq!(ids, keys(&base["nodes"]));
    assert_eq!(base["roots"], json!([l["T"], l["Z"]]));
    assert!(valid(&base));
    assert_eq!(rows(&base["nodes"][text(&l["T"])]["children"]).len(), 4);
    for (a, b) in [("P", "X"), ("X", "Y")] {
        assert_eq!(base["nodes"][text(&l[a])]["children"], json!([l[b]]));
    }
    assert_eq!(base["nodes"][text(&l["Y"])]["children"], json!([]));
    let refs: Vec<_> = base["nodes"]
        .as_object()
        .unwrap()
        .iter()
        .flat_map(|(id, n)| {
            rows(&n["see_also"]).iter().map(move |r| {
                (
                    id.clone(),
                    text(&r["target"]).to_owned(),
                    text(&r["reason"]).to_owned(),
                )
            })
        })
        .collect();
    assert_eq!(refs.len(), 5);
    assert_eq!(refs.iter().map(|r| &r.2).collect::<BTreeSet<_>>().len(), 5);
    assert_eq!(
        base["nodes"]
            .as_object()
            .unwrap()
            .values()
            .map(|n| text(&n["content"]))
            .collect::<BTreeSet<_>>()
            .len(),
        9
    );
    let files: BTreeMap<_, _> = rows(&d["baseline"]["sentinels"])
        .iter()
        .chain(rows(&d["baseline"]["node_resources"]))
        .map(|f| (text(&f["path"]).to_owned(), text(&f["content"]).to_owned()))
        .collect();
    assert_eq!(
        rows(&d["baseline"]["sentinels"]).len() + rows(&d["baseline"]["node_resources"]).len(),
        5
    );
    assert_eq!(files.len(), 5);
    assert_eq!(files.values().collect::<BTreeSet<_>>().len(), 5);
    for (p, b) in &files {
        assert!(
            !p.starts_with('/')
                && !p.split('/').any(|p| ["", ".", ".."].contains(&p))
                && !["trees", "roots", ".git"].contains(&p.split('/').next().unwrap())
                && !p.contains(text(&l["T"]))
                && !b.is_empty()
        );
    }
    for (label, p) in d["prefixes"]["unique"].as_object().unwrap() {
        assert_eq!(resolve(&base, text(p)), Some(text(&l[label]).to_owned()));
    }
    assert!(
        keys(&base["nodes"])
            .iter()
            .filter(|id| id.starts_with(text(&d["prefixes"]["ambiguous"])))
            .count()
            > 1
    );
    assert!(
        !keys(&base["nodes"])
            .iter()
            .any(|id| id.starts_with(text(&d["prefixes"]["absent"])))
    );
    let cases = rows(&d["cases"]);
    let mut wrong_count = 0;
    assert_eq!(
        cases
            .iter()
            .map(|c| text(&c["id"]))
            .collect::<BTreeSet<_>>()
            .len(),
        cases.len()
    );
    assert!(
        cases.iter().filter(|c| c["kind"] == "valid").count() >= 5
            && cases.iter().filter(|c| c["kind"] == "invalid").count() >= 7
    );
    for c in cases {
        let s = expected(&d, c);
        let declared_claim = strings(&c["changed_paths"]);
        let declared = set(declared_claim.clone());
        assert!(
            accepts(&d, c, &s, &declared_claim),
            "{}: incoherent prescription",
            c["id"]
        );
        assert_eq!(keys(&s["nodes"]), ids);
        assert!(
            strings(&d["scope"]["operations"])
                .contains(&text(&c["request"]["operation"]).to_owned())
        );
        for p in &declared {
            assert!(!p.ends_with("node.json"));
            if p.contains("/node.") {
                assert!(payload_path(p));
            }
        }
        let controls = rows(&c["wrong_results"]);
        assert!(controls.len() >= if c["kind"] == "valid" { 3 } else { 2 });
        if c["kind"] == "invalid" {
            assert_eq!(s, base);
            assert!(declared.is_empty());
            assert!(
                [
                    "cyclic-move",
                    "root-moved-as-non-root",
                    "reorder-across-parents",
                    "reorder-after-self",
                    "blank-display-name",
                    "ambiguous-uuid-prefix",
                    "absent-uuid-prefix",
                    "self-parent"
                ]
                .contains(&text(&c["reason_class"]))
            );
            assert!(
                controls
                    .iter()
                    .any(|w| !rows(&w["patch"]["changed_paths_extra"]).is_empty())
            );
        } else {
            assert_eq!(json!(parents(&s)), c["expected"]["derived_parents"]);
            let subject = resolve(&base, text(&c["request"]["node"])).unwrap();
            let operation = text(&c["request"]["operation"]);
            let arg = text(&c["request"]["arg"]);
            let mut independent = base.clone();
            if operation == "rename" {
                independent["nodes"][&subject]["name"] = json!(arg);
            } else {
                let destination = resolve(&base, arg).unwrap();
                let par = parents(&base)[&subject].clone();
                let owner = if operation == "move" {
                    destination.clone()
                } else {
                    par.clone()
                };
                let mut seq = strings(&base["nodes"][&owner]["children"]);
                seq.retain(|c| c != &subject);
                let i = if operation == "move" {
                    seq.len()
                } else {
                    seq.iter().position(|c| c == &destination).unwrap() + 1
                };
                seq.insert(i, subject.clone());
                independent["nodes"][&owner]["children"] = json!(seq);
                if operation == "move" && par != destination {
                    let mut seq = strings(&base["nodes"][&par]["children"]);
                    seq.retain(|c| c != &subject);
                    independent["nodes"][&par]["children"] = json!(seq);
                }
            }
            assert_eq!(independent, s, "{}: reviewed placement", c["id"]);
            let p = &c["preserved"];
            for (field, inventory) in [
                ("content", "content_unchanged"),
                ("micro_summary", "micro_summary_unchanged"),
                ("name", "name_unchanged"),
            ] {
                let covered = covered(&base, &p[inventory]);
                for id in &ids {
                    if !declared.contains(&format!("nodes/{id}/node.md")) {
                        assert!(covered.contains(id), "missing {inventory}:{id}");
                    }
                }
                for id in covered {
                    assert_eq!(s["nodes"][&id][field], base["nodes"][&id][field]);
                }
            }
            for (src, target, reason) in &refs {
                assert!(
                    rows(&p["references_unchanged"])
                        .iter()
                        .any(|r| r["source"] == *src
                            && r["target"] == *target
                            && r["reason"] == *reason)
                );
            }
            for id in &ids {
                if !rows(&base["nodes"][id]["children"]).is_empty()
                    && !declared.contains(&format!("nodes/{id}/children.order"))
                {
                    assert!(
                        rows(&p["unrelated_sibling_order"])
                            .iter()
                            .any(|r| r["parent"] == *id)
                    );
                }
            }
            for row in rows(&p["unrelated_sibling_order"]) {
                let scope = set(strings(&row["unrelated"]));
                let seq: Vec<_> = strings(&s["nodes"][text(&row["parent"])]["children"])
                    .into_iter()
                    .filter(|c| !row["unrelated"].is_array() || scope.contains(c))
                    .collect();
                assert_eq!(seq, strings(&row["order"]));
            }
            for row in rows(&p["descendants_carried"]) {
                assert_eq!(s["nodes"][text(&row["node"])]["children"], row["children"]);
            }
            for id in strings(&p["canonical_dirs_unchanged"]) {
                assert!(
                    !declared
                        .iter()
                        .any(|p| p.starts_with(&format!("nodes/{id}/")))
                );
            }
            for field in ["asset_bytes_unchanged", "child_entries_unchanged"] {
                for path in strings(&p[field]) {
                    assert!(!declared.contains(&path));
                }
            }
            if operation == "rename" {
                for (src, target, _) in &refs {
                    if target == &subject {
                        assert!(
                            rows(&c["expected"]["navigation"])
                                .iter()
                                .any(|r| r["source"] == *src && r["target"] == *target)
                        );
                    }
                }
            }
            for row in rows(&c["expected"]["navigation"]) {
                let source = &s["nodes"][text(&row["source"])];
                let target = &s["nodes"][text(&row["target"])];
                assert!(
                    rows(&source["see_also"])
                        .iter()
                        .any(|r| r["target"] == row["target"])
                );
                assert_eq!(target["name"], row["name"]);
                assert_eq!(target["micro_summary"], row["micro_summary"]);
            }
        }
        if ["ambiguous_prefix", "absent_prefix"]
            .contains(&c["request"]["arg_kind"].as_str().unwrap_or(""))
        {
            assert!(
                resolve(&base, text(&c["request"]["node"])).is_none()
                    || resolve(&base, text(&c["request"]["arg"])).is_none()
            );
        }
        for w in controls {
            wrong_count += 1;
            let patch = &w["patch"];
            let mut wrong = s.clone();
            overlay(&mut wrong, patch);
            let changed = if patch["changed_paths"].is_array() {
                strings(&patch["changed_paths"])
            } else {
                declared_claim
                    .iter()
                    .cloned()
                    .chain(strings(&patch["changed_paths_extra"]))
                    .collect()
            };
            let distance = distance(&s, &wrong, &declared_claim, &changed);
            assert!(
                (1..=2).contains(&distance),
                "{}:{} vacuous/distant",
                c["id"],
                w["id"]
            );
            assert!(!accepts(&d, c, &wrong, &changed));
            for path in strings(&patch["changed_paths_extra"]) {
                assert!(!path.ends_with("node.json"));
                if path.contains("/node.") {
                    assert!(payload_path(&path));
                }
            }
        }
    }
    assert_eq!(wrong_count, 58);
    let drafts = rows(&d["drafts"]);
    assert_eq!(
        drafts
            .iter()
            .map(|d| text(&d["id"]))
            .collect::<BTreeSet<_>>()
            .len(),
        drafts.len()
    );
    assert!(
        drafts.iter().filter(|d| d["expect"] == "rejected").count() >= 4
            && drafts.iter().filter(|d| d["expect"] == "accepted").count() >= 3
    );
    assert!(drafts.iter().any(|d| d["expect"] == "rejected"
        && rows(&d["edits"]).len() == 1
        && d["edits"][0]["op"] == "write_node_md"
        && d["edits"][0]["byte_form"] == "reserialised"));
    for draft in drafts {
        let c = cases.iter().find(|c| c["id"] == draft["case"]).unwrap();
        let mut s = base.clone();
        let mut bytes = files.clone();
        let mut form = true;
        let mut order = true;
        for edit in rows(&draft["edits"]) {
            match text(&edit["op"]) {
                "mv" => {
                    let from: Vec<_> = text(&edit["from"]).split('/').collect();
                    let to: Vec<_> = text(&edit["to"]).split('/').collect();
                    assert!(
                        from.len() == 4
                            && to.len() == 4
                            && from[0] == "nodes"
                            && to[0] == "nodes"
                            && from[2] == "children"
                            && to[2] == "children"
                            && uuid(from[1])
                            && uuid(to[1])
                            && uuid(from[3])
                            && uuid(to[3])
                    );
                    let mut src = strings(&s["nodes"][from[1]]["children"]);
                    assert!(src.contains(&from[3].to_owned()));
                    src.retain(|c| c != from[3]);
                    s["nodes"][from[1]]["children"] = json!(src);
                    let mut dst = strings(&s["nodes"][to[1]]["children"]);
                    dst.push(to[3].to_owned());
                    s["nodes"][to[1]]["children"] = json!(dst);
                }
                "write_order" | "write_node_md" => {
                    let path: Vec<_> = text(&edit["path"]).split('/').collect();
                    assert!(path.len() == 3 && path[0] == "nodes" && uuid(path[1]));
                    if edit["op"] == "write_order" {
                        assert_eq!(path[2], "children.order");
                        order &= same_inventory(
                            &strings(&s["nodes"][path[1]]["children"]),
                            &strings(&edit["children"]),
                        );
                        s["nodes"][path[1]]["children"] = edit["children"].clone();
                    } else {
                        assert_eq!(path[2], "node.md");
                        assert!(
                            ["native_rename", "reserialised"].contains(&text(&edit["byte_form"]))
                                && edit.get("format").is_none()
                                && edit.get("final_newline").is_none()
                        );
                        for (k, v) in edit["changes"].as_object().unwrap() {
                            s["nodes"][path[1]][k] = v.clone();
                        }
                        form &= edit["byte_form"] == "native_rename";
                    }
                }
                "append_bytes" => bytes
                    .get_mut(text(&edit["path"]))
                    .unwrap()
                    .push_str(text(&edit["content"])),
                op => panic!("unknown edit {op}"),
            }
        }
        let matches = s == expected(&d, c) && bytes == files && valid(&s) && order && form;
        assert_eq!(matches, draft["expect"] == "accepted", "{}", draft["id"]);
        if !matches {
            assert!(text(&draft["why"]).len() > 20);
        }
    }
    let c1 = cases
        .iter()
        .find(|c| c["id"] == "c1-move-x-under-z")
        .unwrap();
    let c2 = cases
        .iter()
        .find(|c| c["id"] == "c2-reorder-w-after-p")
        .unwrap();
    let c3 = cases.iter().find(|c| c["id"] == "c3-rename-p").unwrap();
    let invalid: Vec<_> = cases.iter().filter(|c| c["kind"] == "invalid").collect();
    let controls = [
        (
            c1,
            json!({"nodes":{text(&l["Z"]):{"children":[l["X"],l["M"]]}}}),
        ),
        (c1, json!({"nodes":{text(&l["X"]):{"children":[]}}})),
        (
            c2,
            json!({"nodes":{text(&l["P"]):{"children":[l["X"],l["W"]]},text(&l["T"]):{"children":[l["P"],l["Q"],l["U"]]}}}),
        ),
        (c3, json!({"nodes":{text(&l["P"]):{"content":"rewritten"}}})),
        (
            c3,
            json!({"nodes":{text(&l["Q"]):{"see_also":[{"target":"Renamed Next Node P","reason":base["nodes"][text(&l["Q"])]["see_also"][0]["reason"]}]}}}),
        ),
        (
            invalid[0],
            json!({"nodes":{text(&l["T"]):{"children":[l["Q"],l["P"],l["U"],l["W"]]}}}),
        ),
        (
            invalid[1],
            json!({"nodes":{text(&l["M"]):{"see_also":[{"target":base["nodes"][text(&l["M"])]["see_also"][0]["target"],"reason":"  "}]}}}),
        ),
        (
            invalid[2],
            json!({"nodes":{text(&l["Q"]):{"children":[l["P"]]}}}),
        ),
        (
            invalid[3],
            json!({"nodes":{text(&l["Y"]):{"children":[l["T"]]},text(&l["T"]):{"children":[l["Q"],l["U"],l["W"]]},text(&l["P"]):{"children":[l["X"],l["T"]]}}}),
        ),
        (
            c2,
            json!({"nodes":{text(&l["T"]):{"children":[l["P"],l["Q"],l["U"]]}}}),
        ),
    ];
    for (c, patch) in controls {
        let mut s = expected(&d, c);
        overlay(&mut s, &patch);
        assert!(!accepts(&d, c, &s, &strings(&c["changed_paths"])));
    }
    let mut declared = strings(&c2["changed_paths"]);
    declared.push(format!(
        "nodes/{}/children/{}",
        text(&l["T"]),
        text(&l["W"])
    ));
    assert!(!accepts(&d, c2, &expected(&d, c2), &declared));
    println!(
        "fixture-only: {} cases, {} drafts, {wrong_count} wrong results, 11 self-violations passed; no product API",
        cases.len(),
        drafts.len()
    );
}
