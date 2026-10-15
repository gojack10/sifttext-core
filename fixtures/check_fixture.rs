//! Independent declarative fixture checks; no Core parser, CLI or Bend calls.
//! Replaces fixtures/check_fixture.mjs; shasum supplies only the UUIDv5 SHA-1 primitive.
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{MapAccess, SeqAccess, Visitor},
    ser::{SerializeMap, SerializeSeq},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

// Preserve source object order so canonical-byte checks have the same meaning as
// JSON.stringify(JSON.parse(text), null, 2), without enabling a new JSON feature.
enum Ordered {
    Object(Vec<(String, Ordered)>),
    Array(Vec<Ordered>),
    Scalar(Value),
}
impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Read;
        impl<'de> Visitor<'de> for Read {
            type Value = Ordered;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("fixture JSON")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut m: M) -> Result<Ordered, M::Error> {
                let mut fields = vec![];
                while let Some(field) = m.next_entry()? {
                    fields.push(field);
                }
                Ok(Ordered::Object(fields))
            }
            fn visit_seq<S: SeqAccess<'de>>(self, mut s: S) -> Result<Ordered, S::Error> {
                let mut values = vec![];
                while let Some(v) = s.next_element()? {
                    values.push(v);
                }
                Ok(Ordered::Array(values))
            }
            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Ordered, E> {
                Ok(Ordered::Scalar(json!(s)))
            }
            fn visit_bool<E: serde::de::Error>(self, b: bool) -> Result<Ordered, E> {
                Ok(Ordered::Scalar(json!(b)))
            }
            fn visit_u64<E: serde::de::Error>(self, n: u64) -> Result<Ordered, E> {
                Ok(Ordered::Scalar(json!(n)))
            }
            fn visit_i64<E: serde::de::Error>(self, n: i64) -> Result<Ordered, E> {
                Ok(Ordered::Scalar(json!(n)))
            }
            fn visit_f64<E: serde::de::Error>(self, n: f64) -> Result<Ordered, E> {
                Ok(Ordered::Scalar(json!(n)))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Ordered, E> {
                Ok(Ordered::Scalar(Value::Null))
            }
        }
        d.deserialize_any(Read)
    }
}
impl Serialize for Ordered {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Object(fields) => {
                let mut m = s.serialize_map(Some(fields.len()))?;
                for (k, v) in fields {
                    m.serialize_entry(k, v)?;
                }
                m.end()
            }
            Self::Array(values) => {
                let mut seq = s.serialize_seq(Some(values.len()))?;
                for v in values {
                    seq.serialize_element(v)?;
                }
                seq.end()
            }
            Self::Scalar(v) => v.serialize(s),
        }
    }
}
fn canonical(text: &str) -> Value {
    let ordered: Ordered = serde_json::from_str(text).unwrap();
    assert_eq!(
        serde_json::to_string_pretty(&ordered).unwrap() + "\n",
        text,
        "noncanonical JSON bytes"
    );
    serde_json::from_str(text).unwrap()
}
fn load(path: &Path) -> Value {
    canonical(&fs::read_to_string(path).unwrap())
}
fn array(v: &Value) -> &[Value] {
    v.as_array().unwrap()
}
fn text(v: &Value) -> &str {
    v.as_str().unwrap()
}
fn node<'a>(g: &'a Value, id: &str) -> &'a Value {
    array(&g["nodes"]).iter().find(|n| n["id"] == id).unwrap()
}
fn node_mut<'a>(g: &'a mut Value, id: &str) -> &'a mut Value {
    g["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|n| n["id"] == id)
        .unwrap()
}
fn parents(g: &Value) -> Value {
    let mut p = serde_json::Map::new();
    for n in array(&g["nodes"]) {
        p.insert(text(&n["id"]).into(), json!("none"));
    }
    for n in array(&g["nodes"]) {
        for c in array(&n["children"]) {
            p.insert(text(c).into(), n["id"].clone());
        }
    }
    Value::Object(p)
}
fn uuid5(label: &str) -> String {
    let namespace = "6ba7b8109dad11d180b400c04fd430c8";
    let mut bytes: Vec<u8> = (0..32)
        .step_by(2)
        .map(|i| u8::from_str_radix(&namespace[i..i + 2], 16).unwrap())
        .collect();
    bytes.extend_from_slice(format!("sifttext-file-graph-fixture:{label}").as_bytes());
    let mut child = Command::new("shasum")
        .args(["-a", "1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let hash = String::from_utf8(out.stdout).unwrap();
    assert_eq!(hash.split_whitespace().next().unwrap().len(), 40);
    let mut digest: Vec<u8> = (0..32)
        .step_by(2)
        .map(|i| u8::from_str_radix(&hash[i..i + 2], 16).unwrap())
        .collect();
    digest[6] = (digest[6] & 15) | 0x50;
    digest[8] = (digest[8] & 63) | 0x80;
    let h: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}
fn uuid_format(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
        && s.as_bytes()[14] == b'5'
        && b"89ab".contains(&s.as_bytes()[19])
}
fn violations(g: &Value, labels: &Value) -> BTreeSet<&'static str> {
    let mut errors = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut edges = BTreeMap::new();
    for n in array(&g["nodes"]) {
        let id = text(&n["id"]);
        if !uuid_format(id) {
            errors.insert("bad_id_format");
        }
        if !ids.insert(id) {
            errors.insert("dup_node_id");
        }
        for field in ["name", "micro_summary", "content"] {
            if n[field].as_str().is_none_or(|s| s.trim().is_empty()) {
                errors.insert("blank_field");
            }
        }
        if !n["children"].is_array() {
            errors.insert("bad_children_shape");
        }
        if !n["see_also"].is_array() {
            errors.insert("bad_see_also_shape");
        }
        edges.insert(id, n["children"].as_array().map_or(&[][..], Vec::as_slice));
    }
    let mut counts: BTreeMap<&str, usize> = ids.iter().map(|id| (*id, 0)).collect();
    for n in array(&g["nodes"]) {
        let mut seen = BTreeSet::new();
        for c in n["children"].as_array().into_iter().flatten() {
            let id = text(c);
            if !ids.contains(id) {
                errors.insert("unknown_child");
            }
            if !seen.insert(id) {
                errors.insert("dup_child_in_node");
            }
            if let Some(count) = counts.get_mut(id) {
                *count += 1;
            }
        }
        for r in n["see_also"].as_array().into_iter().flatten() {
            match r["target"].as_str() {
                None | Some("") => {
                    errors.insert("bad_reference_shape");
                }
                Some(id) if n["id"] == id => {
                    errors.insert("self_reference");
                }
                Some(id) if !ids.contains(id) => {
                    errors.insert("dangling_reference");
                }
                _ => (),
            }
            if r["reason"].as_str().is_none_or(|s| s.trim().is_empty()) {
                errors.insert("blank_reason");
            }
        }
    }
    if counts.values().any(|n| *n > 1) {
        errors.insert("dup_parentage");
    }
    let roots = g["roots"].as_array();
    if roots.is_none_or(Vec::is_empty) {
        errors.insert("bad_roots");
    }
    let mut unique = BTreeSet::new();
    for r in roots.into_iter().flatten() {
        let id = text(r);
        if !ids.contains(id) {
            errors.insert("unknown_root");
        }
        if counts.get(id).is_some_and(|n| *n > 0) {
            errors.insert("root_has_parent");
        }
        if !unique.insert(id) {
            errors.insert("dup_root");
        }
    }
    if counts.iter().any(|(id, n)| *n == 0 && !unique.contains(id)) {
        errors.insert("orphan_node");
    }
    fn visit<'a>(
        id: &'a str,
        edges: &BTreeMap<&'a str, &'a [Value]>,
        stack: &mut BTreeSet<&'a str>,
    ) -> bool {
        if !stack.insert(id) {
            return true;
        }
        let cycle = edges
            .get(id)
            .into_iter()
            .flat_map(|cs| cs.iter())
            .any(|c| visit(text(c), edges, stack));
        stack.remove(id);
        cycle
    }
    if ids.iter().any(|id| visit(id, &edges, &mut BTreeSet::new())) {
        errors.insert("cycle");
    }
    let mut sentinels = BTreeSet::new();
    for s in array(&g["sentinels"]) {
        let p = s["path"].as_str().unwrap_or("");
        if p.is_empty()
            || p.starts_with(['/', '\\'])
            || p.as_bytes().get(1) == Some(&b':')
            || p.split(['/', '\\'])
                .any(|part| ["", ".", ".."].contains(&part))
        {
            errors.insert("bad_sentinel_path");
        }
        if labels
            .as_object()
            .unwrap()
            .values()
            .any(|id| p.contains(text(id)))
        {
            errors.insert("sentinel_path_collides_node_uuid");
        }
        if s["content"].as_str().is_none_or(str::is_empty) {
            errors.insert("blank_sentinel_content");
        }
        if !sentinels.insert(p) {
            errors.insert("dup_sentinel_path");
        }
    }
    errors
}

pub fn check_fixture(project: &Path) {
    let fixtures = project.join("fixtures");
    let contract = load(&fixtures.join("fixture-contract.json"));
    let labels = &contract["labels"];
    let names = ["R", "A", "B", "C", "D", "S", "E"];
    assert_eq!(contract["schema"], "sifttext.file-graph.fixture/1");
    assert_eq!(
        labels
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        names.into_iter().collect()
    );
    for label in names {
        assert_eq!(labels[label], uuid5(label));
    }
    assert_eq!(contract["roots"], json!([labels["R"], labels["S"]]));
    assert_eq!(
        array(&contract["nodes"])
            .iter()
            .map(|n| n["id"].clone())
            .collect::<Vec<_>>(),
        names.map(|l| labels[l].clone())
    );
    assert!(array(&contract["sentinels"]).len() >= 2);
    assert!(violations(&contract, labels).is_empty());
    for n in array(&contract["nodes"]) {
        for key in ["parent", "parents", "order", "children.order"] {
            assert!(n.get(key).is_none());
        }
        for r in array(&n["see_also"]) {
            assert_eq!(
                r.as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                ["reason", "target"]
            );
        }
    }
    for (src, dst) in [("A", "D"), ("E", "B")] {
        let refs: Vec<_> = array(&node(&contract, text(&labels[src]))["see_also"])
            .iter()
            .filter(|r| r["target"] == labels[dst])
            .collect();
        assert_eq!(refs.len(), 1);
        assert!(!text(&refs[0]["reason"]).trim().is_empty());
    }
    let before = load(&fixtures.join("expected/before.json"));
    assert_eq!(before["schema"], "sifttext.file-graph.expected/1");
    for key in ["nodes", "roots", "sentinels"] {
        assert_eq!(before[key], contract[key]);
    }
    assert_eq!(before["derived_parents"], parents(&before));
    for case in ["move-b-under-a", "reorder-b-after-c", "rename-b"] {
        let got = load(&fixtures.join(format!("expected/{case}.json")));
        assert_eq!(got["schema"], "sifttext.file-graph.expected/1");
        assert_eq!(got["case"], case);
        assert!(
            got["transition"]
                .as_object()
                .unwrap()
                .values()
                .all(Value::is_string)
        );
        assert!(violations(&got, labels).is_empty());
        let mut want = contract.clone();
        match case {
            "move-b-under-a" => {
                node_mut(&mut want, text(&labels["R"]))["children"] =
                    json!([labels["A"], labels["C"]]);
                node_mut(&mut want, text(&labels["A"]))["children"] = json!([labels["B"]]);
            }
            "reorder-b-after-c" => {
                node_mut(&mut want, text(&labels["R"]))["children"] =
                    json!([labels["A"], labels["C"], labels["B"]])
            }
            _ => {
                let new_name = &got["transition"]["new_display_name"];
                assert!(
                    new_name.is_string()
                        && *new_name != node(&contract, text(&labels["B"]))["name"]
                );
                node_mut(&mut want, text(&labels["B"]))["name"] = new_name.clone();
            }
        }
        for key in ["nodes", "roots", "sentinels"] {
            assert_eq!(
                got[key], want[key],
                "{case}: exact independent transition {key}"
            );
        }
        assert_eq!(got["derived_parents"], parents(&want));
    }
    let ghost = uuid5("ghost");
    for code in [
        "cycle",
        "root_has_parent",
        "dup_node_id",
        "dup_parentage",
        "dup_child_in_node",
        "unknown_child",
        "orphan_node",
        "dangling_reference",
        "blank_reason",
        "blank_field",
        "bad_id_format",
        "bad_sentinel_path_escape",
        "bad_sentinel_path_absolute",
        "sentinel_path_collides_node_uuid",
        "dup_sentinel_path",
    ] {
        let mut g = contract.clone();
        let r = text(&labels["R"]);
        let a = text(&labels["A"]);
        let b = text(&labels["B"]);
        let c = text(&labels["C"]);
        let d = text(&labels["D"]);
        let s = text(&labels["S"]);
        match code {
            "cycle" => {
                node_mut(&mut g, r)["children"] = json!([labels["A"], labels["C"]]);
                node_mut(&mut g, d)["children"] = json!([labels["B"]]);
            }
            "root_has_parent" => node_mut(&mut g, s)["children"]
                .as_array_mut()
                .unwrap()
                .push(labels["R"].clone()),
            "dup_node_id" => {
                let copy = g["nodes"][3].clone();
                g["nodes"].as_array_mut().unwrap().push(copy);
            }
            "dup_parentage" => node_mut(&mut g, a)["children"] = json!([labels["B"]]),
            "dup_child_in_node" => node_mut(&mut g, r)["children"]
                .as_array_mut()
                .unwrap()
                .push(labels["C"].clone()),
            "unknown_child" => node_mut(&mut g, r)["children"]
                .as_array_mut()
                .unwrap()
                .push(json!(ghost)),
            "orphan_node" => node_mut(&mut g, s)["children"] = json!([]),
            "dangling_reference" => node_mut(&mut g, a)["see_also"][0]["target"] = json!(ghost),
            "blank_reason" => node_mut(&mut g, a)["see_also"][0]["reason"] = json!("   "),
            "blank_field" => node_mut(&mut g, c)["content"] = json!(""),
            "bad_id_format" => node_mut(&mut g, c)["id"] = json!("node-C"),
            "bad_sentinel_path_escape" => g["sentinels"][0]["path"] = json!("../outside.txt"),
            "bad_sentinel_path_absolute" => g["sentinels"][0]["path"] = json!("/etc/passwd"),
            "sentinel_path_collides_node_uuid" => {
                g["sentinels"][0]["path"] = json!(format!("nodes/{b}/payload.txt"))
            }
            _ => {
                let copy = g["sentinels"][1].clone();
                g["sentinels"].as_array_mut().unwrap().push(copy);
            }
        }
        let expected = if code.starts_with("bad_sentinel_path_") {
            "bad_sentinel_path"
        } else {
            code
        };
        assert!(
            violations(&g, labels).contains(expected),
            "mutation {code} was accepted"
        );
    }
    assert!(
        violations(&contract, labels).is_empty(),
        "mutation controls changed their baseline"
    );
}
