// Q2 independent Markdown / native boundary checks for the node.md + trees/
// representation.
//
// These checks are prescribed by this task from the storage contract (README
// "Markdown files" grammar, the reviewed placement decision, the Restore
// Markdown Storage check list and the Strengthen Graph Assurance native
// obligations). Each rejection carries the reason class that contract states.
// Positive cases reach the real compiled boundary: the checked Bend executable
// through `Policy::checked`/`apply`, and the actual CLI process for native
// draft, hostile-subprocess and restoration evidence.
//
// Every accepted claim is paired with a neighbouring state that must NOT be
// accepted, so a PASS cannot be vacuous.
//
// Limits: bounded single-writer private drafts, 256 nodes / 1 MiB per file. No
// claim about crash atomicity, concurrency, publication, sandboxing, migration
// or full Core readiness. The builders here are test scaffolding, not a second
// product parser: the store under test re-reads every byte itself.

use sifttext_file_graph::{
    Entry, Operation, Policy, Reference, Store, apply, invoke, parse_markdown, sha256,
    validate_draft,
};
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

#[path = "common/hostile_policy.rs"]
mod hostile_policy;

// ---------------------------------------------------------------- identities
fn id(n: usize) -> String {
    format!("d2000000-0000-4000-8000-{n:012x}")
}
const R: usize = 1;
const A: usize = 2;
const B: usize = 3;
const C: usize = 4;
const S: usize = 5;
const D: usize = 6;

const SENTINEL: &str = "Q2-SENTINEL-01 é 中 🦀 stay byte-identical\n";

// ------------------------------------------------------- prescribed content
// Hand-authored Unicode payloads. These constants are the expectation; nothing
// here is derived from a parser round-trip.
const NAME_R: &str = "Q2 Boundary Root — café 中文";
const NAME_A: &str = "Ünïcode \"quoted\" \\ backslash é→中 and a separator \u{2028} here";
const NAME_B: &str = "B — 🦀 crab, a tab\tand a BEL \u{0007}";
const NAME_C: &str = "C plain";
const NAME_S: &str = "Second tree root";
const NAME_D: &str = "D leaf";
const SUMMARY_A: &str = "Carries unknown metadata, a Unicode reason and a body with fences.";
const REASON_A_B: &str = "Read B for the é→中 evidence; keep \u{2028} and \"quotes\" exactly.";
const REASON_D_R: &str = "D navigates to the boundary root.";

/// Body with a leading blank line, a code fence containing a `---` line, prose
/// that looks like a reference, multibyte text and no final newline.
const BODY_R: &str = "\n# Boundary root\n\n```text\n---\na fenced line that is not a metadata fence\n```\n\nProse mentions [[d2000000-0000-4000-8000-000000000003]] and [[not-an-edge]] create no edges.\nMultibyte: é 中 🦀, and no trailing newline";
const BODY_A: &str = "A body: the closing fence above is the last structural fence.\n";
const BODY_B: &str = "B line one\r\nCRLF line preserved exactly\r\n";
const BODY_C: &str = "C body\n";
const BODY_S: &str = "S body\n";
const BODY_D: &str = "D body\n";

/// Opaque metadata that must survive byte-for-byte and carry no Core meaning.
const UNKNOWN_A: [(&str, &str); 3] = [
    ("custom", "{\"nested\":[1,2,{\"ünïcode\":\"✓\"}]}"),
    ("flag", "true"),
    ("arr", "[\"---\", \"name: not-a-field\"]"),
];

fn temp(tag: &str) -> TempDir {
    match std::env::var("SIFTTEXT_EVIDENCE_DIR") {
        Ok(p) => tempfile::Builder::new()
            .prefix(&format!("q2-markdown-boundary-{tag}-"))
            .tempdir_in(p)
            .unwrap(),
        Err(_) => tempfile::tempdir().unwrap(),
    }
}

fn node_file(root: &Path, n: usize) -> PathBuf {
    root.join(format!("nodes/{}/node.md", id(n)))
}

fn body_of(n: usize) -> &'static str {
    match n {
        1 => BODY_R,
        2 => BODY_A,
        3 => BODY_B,
        4 => BODY_C,
        5 => BODY_S,
        _ => BODY_D,
    }
}

fn name_of(n: usize) -> &'static str {
    match n {
        1 => NAME_R,
        2 => NAME_A,
        3 => NAME_B,
        4 => NAME_C,
        5 => NAME_S,
        _ => NAME_D,
    }
}

/// Author one node.md in the selected grammar. Unknown metadata lines are
/// written verbatim, exactly as an ordinary editor would leave them.
fn node_md(n: usize, unknown: &[(&str, &str)], body: &str) -> Vec<u8> {
    let refs: Vec<Reference> = match n {
        2 => vec![Reference { target: id(B), reason: REASON_A_B.into() }],
        6 => vec![Reference { target: id(R), reason: REASON_D_R.into() }],
        _ => vec![],
    };
    let summary = if n == A { SUMMARY_A } else { "A Q2 boundary node." };
    let mut out = String::from("---\n");
    out.push_str(&format!("id: {}\n", serde_json::to_string(&id(n)).unwrap()));
    out.push_str(&format!("name: {}\n", serde_json::to_string(name_of(n)).unwrap()));
    out.push_str(&format!("micro_summary: {}\n", serde_json::to_string(summary).unwrap()));
    out.push_str(&format!("see_also: {}\n", serde_json::to_string(&refs).unwrap()));
    for (key, value) in unknown {
        out.push_str(&format!("{key}: {value}\n"));
    }
    out.push_str("---\n");
    out.push_str(body);
    out.into_bytes()
}

fn order(root: &Path, n: usize, cs: &[usize]) {
    let text = cs.iter().map(|&c| id(c) + "\n").collect::<String>();
    fs::write(root.join(format!("nodes/{}/children.order", id(n))), text).unwrap();
}

fn link(root: &Path, parent: usize, child: usize) {
    std::os::unix::fs::symlink(
        format!("../../{}", id(child)),
        root.join(format!("nodes/{}/children/{}", id(parent), id(child))),
    )
    .unwrap();
}

/// The independent boundary fixture: two explicit roots, one depth-two subtree,
/// Unicode payloads, opaque unknown metadata and unrelated ordinary files.
fn fixture(root: &Path) {
    fs::create_dir_all(root.join("trees")).unwrap();
    for n in [R, A, B, C, S, D] {
        fs::create_dir_all(root.join(format!("nodes/{}/children", id(n)))).unwrap();
        fs::write(root.join(format!("nodes/{}/children/.keep", id(n))), []).unwrap();
        let unknown: &[(&str, &str)] = if n == A { &UNKNOWN_A } else { &[] };
        fs::write(node_file(root, n), node_md(n, unknown, body_of(n))).unwrap();
        order(root, n, &[]);
    }
    for (parent, cs) in [(R, vec![A, B, C]), (A, vec![D])] {
        for &child in &cs {
            link(root, parent, child);
        }
        order(root, parent, &cs);
    }
    for r in [R, S] {
        std::os::unix::fs::symlink(
            format!("../nodes/{}", id(r)),
            root.join(format!("trees/{}", id(r))),
        )
        .unwrap();
    }
    fs::create_dir_all(root.join("attachments")).unwrap();
    fs::write(root.join("attachments/keep.txt"), SENTINEL).unwrap();
    fs::create_dir_all(root.join(format!("nodes/{}/code", id(B)))).unwrap();
    fs::write(
        root.join(format!("nodes/{}/code/blob.bin", id(B))),
        [0u8, 255, 12, 128, 10],
    )
    .unwrap();
}

fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn get(root: &Path) -> Store {
    Store::read(root).unwrap()
}

fn err_at(root: &Path) -> String {
    match Store::read(root) {
        Ok(_) => panic!("expected the store to reject, but it read as valid"),
        Err(e) => e.to_string(),
    }
}

/// Complete byte/path inventory of a store as one hash, so "nothing else moved"
/// is a single checkable claim instead of a narrative.
fn inventory_digest(store: &Store) -> String {
    let mut dump = Vec::new();
    for (path, entry) in &store.inventory {
        dump.extend_from_slice(path.to_string_lossy().as_bytes());
        dump.push(0);
        match entry {
            Entry::File(bytes) => {
                dump.extend_from_slice(b"f");
                dump.extend_from_slice(bytes);
            }
            Entry::Link(target) => {
                dump.extend_from_slice(b"l");
                dump.extend_from_slice(target.to_string_lossy().as_bytes());
            }
            Entry::Dir => dump.extend_from_slice(b"d"),
        }
        dump.push(0);
    }
    sha256(&dump)
}

fn sentinels_untouched(root: &Path, label: &str) {
    assert_eq!(
        fs::read(root.join("attachments/keep.txt")).unwrap(),
        SENTINEL.as_bytes(),
        "{label}: unrelated ordinary file touched"
    );
    assert_eq!(
        fs::read(root.join(format!("nodes/{}/code/blob.bin", id(B)))).unwrap(),
        vec![0u8, 255, 12, 128, 10],
        "{label}: unrelated resource touched"
    );
    assert_eq!(
        fs::read(node_file(root, R)).unwrap(),
        node_md(R, &[], BODY_R),
        "{label}: unrelated authored node touched"
    );
}

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn cli() -> PathBuf {
    let path = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug"))
        .join("sifttext-file-graph");
    assert!(path.exists(), "{} missing: run sh scripts/build.sh", path.display());
    path
}

fn run_cli(args: &[&str]) -> (bool, String, String) {
    let out = Command::new(cli()).args(args).output().unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn run(program: &str, args: &[&str]) {
    let out = Command::new(program).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "{program} {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn append_bytes(path: &Path, bytes: &[u8]) {
    let mut file = fs::OpenOptions::new().append(true).open(path).unwrap();
    file.write_all(bytes).unwrap();
}

/// Byte span of the `name` metadata value, as an editor would see it.
fn name_span(text: &str) -> std::ops::Range<usize> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\n', '\r']);
        if let Some((key, value)) = bare.split_once(':')
            && key.trim_matches([' ', '\t']) == "name"
        {
            let start = offset + bare.find(':').unwrap() + 1
                + value.len()
                - value.trim_start_matches([' ', '\t']).len();
            return start..start + value.trim_matches([' ', '\t']).len();
        }
        offset += line.len();
    }
    panic!("no name metadata line");
}

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
    panic!("no closing metadata fence");
}

/// One prescribed unsafe-path shape and the reason its rejection must state.
struct UnsafeCase {
    label: &'static str,
    reason: Reason,
    tamper: fn(&Path),
}

/// A reason the grammar names exactly, versus any JSON value/type rejection:
/// the precise serde wording is deliberately not treated as a contract.
#[derive(Clone, Copy)]
enum Reason {
    Named(&'static str),
    Json,
}

fn expect_reason(label: &str, got: &str, reason: Reason) {
    match reason {
        Reason::Named(want) => assert!(
            got.contains(want),
            "{label}: rejection reason was {got:?}, the contract states {want:?}"
        ),
        Reason::Json => assert!(
            got.contains("at line 1 column")
                || got.contains("expected")
                || got.contains("invalid type")
                || got.contains("key must be a string")
                || got.contains("trailing characters"),
            "{label}: expected a JSON metadata rejection, got {got:?}"
        ),
    }
}

// ------------------------------------------------------ 1. fidelity at the boundary
#[test]
fn unicode_unknown_and_body_payloads_survive_the_compiled_boundary() {
    let dir = temp("fidelity");
    fixture(dir.path());
    let store = get(dir.path());

    // Exact authored values, compared against the constants above.
    assert_eq!(store.nodes[&id(R)].name, NAME_R);
    assert_eq!(store.nodes[&id(A)].name, NAME_A);
    assert_eq!(store.nodes[&id(B)].name, NAME_B);
    assert_eq!(store.nodes[&id(A)].micro_summary, SUMMARY_A);
    assert_eq!(store.nodes[&id(R)].content, BODY_R, "body bytes changed");
    assert_eq!(store.nodes[&id(B)].content, BODY_B, "CRLF body was normalised");
    assert!(!store.nodes[&id(R)].content.ends_with('\n'), "a body newline was invented");
    assert!(
        store.nodes[&id(R)].content.contains("---\n"),
        "the fixture body must contain a fence-like line"
    );
    assert_eq!(store.children[&id(R)], vec![id(A), id(B), id(C)]);
    assert_eq!(store.children[&id(A)], vec![id(D)]);
    assert_eq!(store.roots, vec![id(R), id(S)]);

    // References: exact UUID plus exact reason; prose creates no edge.
    let refs = &store.nodes[&id(A)].see_also;
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].target, id(B));
    assert_eq!(refs[0].reason, REASON_A_B);
    assert!(store.nodes[&id(R)].see_also.is_empty(), "a prose mention became an edge");

    // The real compiled policy accepts this store.
    let p = policy();
    p.validate(&store)
        .expect("the checked policy must accept a well-formed Markdown store");

    // Reading never rewrites: every authored file is still the bytes written.
    for n in [R, A, B, C, S, D] {
        let unknown: &[(&str, &str)] = if n == A { &UNKNOWN_A } else { &[] };
        assert_eq!(
            fs::read(node_file(dir.path(), n)).unwrap(),
            node_md(n, unknown, body_of(n)),
            "node {n} was rewritten by reading"
        );
    }

    // Inspected paths are exact full-UUID canonical Markdown locations.
    let view = store.view();
    let rows = view["nodes"].as_array().unwrap();
    for n in [R, A, B, C, S, D] {
        let row = rows
            .iter()
            .find(|x| x["id"] == serde_json::json!(id(n)))
            .unwrap();
        assert_eq!(
            row["path"],
            serde_json::json!(store
                .path
                .join(format!("nodes/{}/node.md", id(n)))
                .to_str()
                .unwrap()),
            "inspected path is not the canonical node.md location"
        );
    }
    let nav_a = rows
        .iter()
        .find(|x| x["id"] == serde_json::json!(id(A)))
        .unwrap()["navigation"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(nav_a.len(), 1);
    assert_eq!(nav_a[0]["name"], serde_json::json!(NAME_B));
    assert_eq!(nav_a[0]["reason"], serde_json::json!(REASON_A_B));
    assert_eq!(
        nav_a[0]["path"],
        serde_json::json!(store
            .path
            .join(format!("nodes/{}/node.md", id(B)))
            .to_str()
            .unwrap())
    );
    assert!(
        rows.iter()
            .find(|x| x["id"] == serde_json::json!(id(R)))
            .unwrap()["navigation"]
            .as_array()
            .unwrap()
            .is_empty(),
        "a prose UUID mention produced navigation"
    );

    // Root shortcuts are links in the inventory, not a second hierarchy source.
    for r in [R, S] {
        assert_eq!(
            store.inventory.get(&PathBuf::from(format!("trees/{}", id(r)))),
            Some(&Entry::Link(PathBuf::from(format!("../nodes/{}", id(r))))),
            "trees/ shortcut is not the canonical relative link"
        );
    }

    // A native move through the compiled executable must not touch any other
    // byte, including the opaque unknown metadata.
    let after = apply(
        &p,
        &store,
        &Operation::Move {
            node: id(C),
            parent: id(D),
        },
    )
    .expect("prescribed valid move rejected");
    assert_eq!(after.children[&id(R)], vec![id(A), id(B)]);
    assert_eq!(after.children[&id(D)], vec![id(C)]);
    assert_eq!(after.nodes[&id(A)], store.nodes[&id(A)], "reference payload changed");
    assert_eq!(
        fs::read(node_file(dir.path(), A)).unwrap(),
        node_md(A, &UNKNOWN_A, BODY_A),
        "unknown metadata was normalised by a move elsewhere"
    );
    assert_eq!(
        fs::read(node_file(dir.path(), R)).unwrap(),
        node_md(R, &[], BODY_R),
        "the Unicode body was rewritten by a move elsewhere"
    );
    sentinels_untouched(dir.path(), "after move");

    // A rename through the compiled executable changes exactly the name value.
    let new_name = "Renamed — 中文 café \"q\" \\ 🦀 \u{2028}";
    let original = fs::read(node_file(dir.path(), B)).unwrap();
    let renamed = apply(
        &p,
        &after,
        &Operation::Rename {
            node: id(B),
            name: new_name.to_owned(),
        },
    )
    .expect("prescribed Unicode rename rejected");
    assert_eq!(renamed.nodes[&id(B)].name, new_name);
    let got = fs::read(node_file(dir.path(), B)).unwrap();
    assert_eq!(
        parse_markdown(std::str::from_utf8(&got).unwrap()).unwrap().name,
        new_name
    );
    let mut expected = String::from_utf8(original.clone()).unwrap();
    let span = name_span(&expected);
    expected.replace_range(span, &serde_json::to_string(new_name).unwrap());
    assert_eq!(
        got,
        expected.into_bytes(),
        "rename touched more than the name value"
    );
    assert!(
        String::from_utf8(got).unwrap().ends_with(BODY_B),
        "the CRLF body did not survive a rename of its own node"
    );
    assert_eq!(fs::read(node_file(dir.path(), A)).unwrap(), node_md(A, &UNKNOWN_A, BODY_A));
    sentinels_untouched(dir.path(), "after rename");
    println!("PASS fidelity: 6 Unicode/unknown/body nodes read, validated by the compiled policy, moved and renamed with exact byte inventories");
}

// ------------------------------------------------------- 2. malformed metadata
fn drop_line(text: &str, prefix: &str) -> String {
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        if !line.starts_with(prefix) {
            out.push_str(line);
        }
    }
    out
}

/// Replace the authored see_also line with a prescribed spelling.
fn with_see_also(text: &str, value: &str) -> String {
    let authored = serde_json::to_string(&vec![Reference {
        target: id(B),
        reason: REASON_A_B.to_owned(),
    }])
    .unwrap();
    let line = format!("see_also: {authored}\n");
    assert!(text.contains(&line), "fixture reference line changed shape");
    text.replace(&line, &format!("see_also: {value}\n"))
}

#[test]
fn malformed_and_duplicate_metadata_rejects_with_the_prescribed_reason() {
    let dir = temp("malformed-source");
    fixture(dir.path());
    let valid = fs::read_to_string(node_file(dir.path(), A)).unwrap();
    let uuid_a = id(A);
    let name_line = format!("name: {}\n", serde_json::to_string(NAME_A).unwrap());
    let summary_line = format!("micro_summary: {}\n", serde_json::to_string(SUMMARY_A).unwrap());

    let cases: Vec<(&str, Reason, Vec<u8>)> = vec![
        ("no opening fence", Reason::Named("missing Markdown opening fence"), b(valid.replacen("---\n", "", 1))),
        ("empty file", Reason::Named("missing Markdown metadata"), b(String::new())),
        ("no closing fence", Reason::Named("missing Markdown closing fence"), b(format!("{}\n", &valid[..body_start(&valid) - 4]))),
        ("closing fence without a newline", Reason::Named("unterminated Markdown metadata"), b(valid[..body_start(&valid) - 1].to_owned())),
        ("duplicate name key", Reason::Named("duplicate metadata key"), b(valid.replacen("---\n", "---\nname: \"twice\"\n", 1))),
        ("duplicate id key", Reason::Named("duplicate metadata key"), b(valid.replacen("---\n", &format!("---\nid: \"{uuid_a}\"\n"), 1))),
        ("duplicate see_also key", Reason::Named("duplicate metadata key"), b(valid.replacen("---\n", "---\nsee_also: []\n", 1))),
        ("missing see_also", Reason::Named("missing required metadata"), b(drop_line(&valid, "see_also:"))),
        ("missing micro_summary", Reason::Named("missing required metadata"), b(drop_line(&valid, "micro_summary:"))),
        ("missing id", Reason::Named("missing required metadata"), b(drop_line(&valid, "id:"))),
        ("missing name", Reason::Named("missing required metadata"), b(drop_line(&valid, "name:"))),
        ("metadata line without a colon", Reason::Named("malformed metadata line"), b(valid.replacen("---\n", "---\norphan\n", 1))),
        ("uppercase key", Reason::Named("malformed metadata key"), b(valid.replacen("---\n", "---\nBad: 1\n", 1))),
        ("key containing a space", Reason::Named("malformed metadata key"), b(valid.replacen("---\n", "---\nbad key: 1\n", 1))),
        ("unquoted name value", Reason::Json, b(valid.replace(&name_line, "name: bare\n"))),
        ("null id", Reason::Json, b(valid.replace(&format!("id: \"{uuid_a}\"\n"), "id: null\n"))),
        ("numeric name", Reason::Json, b(valid.replace(&name_line, "name: 7\n"))),
        ("see_also as an object", Reason::Json, b(with_see_also(&valid, "{}"))),
        ("see_also entry without a reason", Reason::Json, b(with_see_also(&valid, &format!("[{{\"target\":\"{uuid_a}\"}}]")))),
        ("see_also entry with an extra field", Reason::Json, b(with_see_also(&valid, &format!("[{{\"target\":\"{uuid_a}\",\"reason\":\"plain\",\"extra\":1}}]")))),
        ("see_also as plain strings", Reason::Json, b(with_see_also(&valid, "[\"just-a-uuid\"]"))),
        ("blank name", Reason::Named("blank node name/micro-summary"), b(valid.replace(&name_line, "name: \"  \"\n"))),
        ("blank micro_summary", Reason::Named("blank node name/micro-summary"), b(valid.replace(&summary_line, "micro_summary: \"\"\n"))),
        ("blank reason", Reason::Named("blank relationship reason"), b(with_see_also(&valid, &format!("[{{\"target\":\"{uuid_a}\",\"reason\":\"  \"}}]")))),
        ("non-UUID reference target", Reason::Named("reference target is not a UUID"), b(with_see_also(&valid, "[{\"target\":\"not-a-uuid\",\"reason\":\"plain\"}]"))),
        ("unknown metadata is not JSON", Reason::Json, b(valid.replacen("---\n", "---\njunk: {oops}\n", 1))),
        ("children metadata", Reason::Named("second authored hierarchy source"), b(valid.replacen("---\n", "---\nchildren: []\n", 1))),
        ("parent metadata", Reason::Named("second authored hierarchy source"), b(valid.replacen("---\n", "---\nparent: \"x\"\n", 1))),
        ("parents metadata", Reason::Named("second authored hierarchy source"), b(valid.replacen("---\n", "---\nparents: null\n", 1))),
        ("id naming another node", Reason::Named("duplicate/mismatched node UUID"), b(valid.replace(&format!("id: \"{uuid_a}\"\n"), &format!("id: \"{}\"\n", id(B))))),
        ("invalid UTF-8 in the body", Reason::Named("invalid utf-8"), {
            let mut bytes = valid.as_bytes().to_vec();
            bytes.push(0xff);
            bytes
        }),
    ];
    assert!(cases.len() >= 25, "the malformed-metadata battery got thin");
    let rejected_cases = cases.len();

    for (label, reason, text) in &cases {
        let dir = temp("malformed-case");
        fixture(dir.path());
        let path = node_file(dir.path(), A);
        fs::write(&path, text).unwrap();
        let got = err_at(dir.path());
        expect_reason(label, &got, *reason);
        // Rejection is non-destructive: the bad file stays exactly as written.
        assert_eq!(fs::read(&path).unwrap(), *text, "{label}: rejected file was rewritten");
        sentinels_untouched(dir.path(), label);
    }

    // Positive grammar neighbours: these spellings are equivalent and must read.
    let accepted: Vec<(&str, String)> = vec![
        ("CRLF metadata", valid.replace('\n', "\r\n")),
        (
            "extra spacing and a blank metadata line",
            valid
                .replacen("name: ", "name:   ", 1)
                .replacen("---\nid:", "---\n\nid:", 1),
        ),
        (
            "unknown metadata with nested JSON",
            valid.replacen("---\n", "---\nx_deep: {\"a\":[1,{\"b\":null}]}\n", 1),
        ),
        ("body that only looks like metadata", format!("{valid}\nname: \"injected\"\n---\n")),
    ];
    for (label, text) in accepted {
        let dir = temp("accepted-case");
        fixture(dir.path());
        fs::write(node_file(dir.path(), A), &text).unwrap();
        let store = match Store::read(dir.path()) {
            Ok(s) => s,
            Err(e) => panic!("{label}: a valid spelling was rejected: {e}"),
        };
        policy()
            .validate(&store)
            .unwrap_or_else(|e| panic!("{label}: the checked policy rejected a valid store: {e}"));
        assert_eq!(store.nodes[&id(A)].name, NAME_A, "{label}: name drifted");
        assert_eq!(store.nodes[&id(A)].see_also[0].reason, REASON_A_B, "{label}: reason drifted");
        assert_eq!(store.nodes[&id(A)].content, body_after(&text), "{label}: body drifted");
        if label == "body that only looks like metadata" {
            assert!(
                store.nodes[&id(A)].content.contains("name: \"injected\""),
                "trailing pseudo-metadata was consumed as fields"
            );
        }
    }
    println!(
        "PASS malformed: {rejected_cases} malformed/duplicate spellings rejected with the stated reason, equivalent spellings still accepted"
    );
}

fn b(s: String) -> Vec<u8> {
    s.into_bytes()
}

fn body_after(text: &str) -> String {
    text[body_start(text)..].to_owned()
}

// ------------------------------------------------------------ 3. unsafe paths
#[test]
fn unsafe_paths_and_link_shapes_reject_without_cleanup() {
    let cases: Vec<UnsafeCase> = vec![
        UnsafeCase {
            label: "absolute child link",
            reason: Reason::Named("unsafe child/root link target"),
            tamper: |root| {
                fs::remove_file(child_path(root, R, A)).unwrap();
                std::os::unix::fs::symlink(root.join(format!("nodes/{}", id(A))), child_path(root, R, A)).unwrap();
            },
        },
        UnsafeCase {
            label: "escaping child link",
            reason: Reason::Named("unsafe child/root link target"),
            tamper: |root| {
                fs::remove_file(child_path(root, R, A)).unwrap();
                std::os::unix::fs::symlink(format!("../../../../{}", id(A)), child_path(root, R, A)).unwrap();
            },
        },
        UnsafeCase {
            label: "child link naming another node",
            reason: Reason::Named("unsafe child/root link target"),
            tamper: |root| {
                fs::remove_file(child_path(root, R, A)).unwrap();
                std::os::unix::fs::symlink(format!("../../{}", id(C)), child_path(root, R, A)).unwrap();
            },
        },
        UnsafeCase {
            label: "child entry as a regular file",
            reason: Reason::Named("child/root entry must be a symlink"),
            tamper: |root| {
                fs::remove_file(child_path(root, R, A)).unwrap();
                fs::write(child_path(root, R, A), "not a link").unwrap();
            },
        },
        UnsafeCase {
            label: "child entry with a non-UUID name",
            reason: Reason::Named("child entry is not a UUID"),
            tamper: |root| {
                fs::remove_file(child_path(root, R, A)).unwrap();
                std::os::unix::fs::symlink(
                    format!("../../{}", id(A)),
                    root.join(format!("nodes/{}/children/older-name", id(R))),
                )
                .unwrap();
            },
        },
        UnsafeCase {
            label: "root shortcut naming another node",
            reason: Reason::Named("unsafe child/root link target"),
            tamper: |root| {
                fs::remove_file(root.join(format!("trees/{}", id(S)))).unwrap();
                std::os::unix::fs::symlink(
                    format!("../nodes/{}", id(D)),
                    root.join(format!("trees/{}", id(S))),
                )
                .unwrap();
            },
        },
        UnsafeCase {
            label: "root shortcut as a directory",
            reason: Reason::Named("child/root entry must be a symlink"),
            tamper: |root| {
                fs::remove_file(root.join(format!("trees/{}", id(S)))).unwrap();
                fs::create_dir(root.join(format!("trees/{}", id(S)))).unwrap();
            },
        },
        UnsafeCase {
            label: "root shortcut for an unknown UUID",
            reason: Reason::Named("unknown root UUID"),
            tamper: |root| {
                std::os::unix::fs::symlink(
                    "../nodes/x",
                    root.join("trees/ffffffff-ffff-4fff-8fff-ffffffffffff"),
                )
                .unwrap();
            },
        },
        UnsafeCase {
            label: "provisional roots/ directory",
            reason: Reason::Named("unsafe unrelated symlink"),
            tamper: |root| {
                fs::create_dir(root.join("roots")).unwrap();
                std::os::unix::fs::symlink(
                    format!("../nodes/{}", id(R)),
                    root.join(format!("roots/{}", id(R))),
                )
                .unwrap();
            },
        },
        UnsafeCase {
            label: "unrelated symlink inside the store",
            reason: Reason::Named("unsafe unrelated symlink"),
            tamper: |root| {
                std::os::unix::fs::symlink("attachments/keep.txt", root.join("hooks.txt")).unwrap();
            },
        },
        UnsafeCase {
            label: "symlinked node.md",
            reason: Reason::Named("unsafe or oversized authored file"),
            tamper: |root| {
                fs::rename(node_file(root, C), root.join(format!("nodes/{}/other.md", id(C)))).unwrap();
                std::os::unix::fs::symlink(
                    root.join(format!("nodes/{}/other.md", id(C))),
                    node_file(root, C),
                )
                .unwrap();
            },
        },
        UnsafeCase {
            label: "hard-linked node.md",
            reason: Reason::Named("hard-linked authored file"),
            tamper: |root| {
                fs::hard_link(node_file(root, C), root.join(format!("nodes/{}/copy.md", id(C)))).unwrap();
            },
        },
        UnsafeCase {
            label: "symlinked node directory",
            reason: Reason::Named("not a real directory"),
            tamper: |root| {
                fs::rename(root.join(format!("nodes/{}", id(C))), root.join("somewhere")).unwrap();
                std::os::unix::fs::symlink(root.join("somewhere"), root.join(format!("nodes/{}", id(C)))).unwrap();
            },
        },
        UnsafeCase {
            label: "node directory with a non-UUID name",
            reason: Reason::Named("node directory is not a canonical full UUID"),
            tamper: |root| {
                fs::create_dir(root.join("nodes/notes")).unwrap();
            },
        },
        UnsafeCase {
            label: "missing children.order",
            reason: Reason::Named("No such file or directory"),
            tamper: |root| {
                fs::remove_file(root.join(format!("nodes/{}/children.order", id(C)))).unwrap();
            },
        },
        UnsafeCase {
            label: "nonempty relationship marker",
            reason: Reason::Named("nonempty relationship directory marker"),
            tamper: |root| {
                fs::write(root.join(format!("nodes/{}/children/.keep", id(C))), "edge").unwrap();
            },
        },
        UnsafeCase {
            label: "nonempty root marker",
            reason: Reason::Named("nonempty root directory marker"),
            tamper: |root| {
                fs::write(root.join("trees/.keep"), "edge").unwrap();
            },
        },
        UnsafeCase {
            label: "order member naming a non-child",
            reason: Reason::Named("missing, duplicate or non-child order member"),
            tamper: |root| {
                fs::write(
                    root.join(format!("nodes/{}/children.order", id(C))),
                    format!("{}\n", id(D)),
                )
                .unwrap();
            },
        },
        UnsafeCase {
            label: "duplicated order member",
            reason: Reason::Named("missing, duplicate or non-child order member"),
            tamper: |root| {
                order(root, R, &[A, A, B, C]);
            },
        },
    ];

    let rejected = cases.len();
    for case in cases {
        let label = case.label;
        let dir = temp("unsafe");
        fixture(dir.path());
        (case.tamper)(dir.path());
        let got = err_at(dir.path());
        expect_reason(label, &got, case.reason);
        // Nothing was cleaned up: the offending entry is exactly what the writer
        // left, and the unrelated parts of the store are untouched.
        sentinels_untouched(dir.path(), label);
        assert!(
            fs::symlink_metadata(dir.path().join(format!("nodes/{}", id(C)))).is_ok(),
            "{label}: the node directory disappeared during rejection"
        );
    }

    // The same tampering through the actual CLI: nonzero exit, stated reason,
    // no stdout, and the draft left inspectable.
    let dir = temp("unsafe-cli");
    fixture(dir.path());
    let before = fs::read(node_file(dir.path(), A)).unwrap();
    fs::remove_file(child_path(dir.path(), R, A)).unwrap();
    std::os::unix::fs::symlink(format!("../../../../{}", id(A)), child_path(dir.path(), R, A)).unwrap();
    let store_s = dir.path().to_string_lossy().into_owned();
    let (ok, stdout, stderr) = run_cli(&["validate", &store_s]);
    assert!(!ok, "the CLI accepted an escaping child link");
    assert!(stderr.contains("rejected:"), "CLI did not report a rejection: {stderr}");
    assert!(stderr.contains("unsafe child/root link target"), "CLI reason: {stderr}");
    assert!(stdout.is_empty(), "the CLI printed a graph for a rejected store");
    assert_eq!(fs::read(node_file(dir.path(), A)).unwrap(), before, "the CLI rewrote the store");
    assert!(
        fs::read_link(child_path(dir.path(), R, A)).is_ok(),
        "the CLI removed the offending link"
    );
    assert_eq!(rejected, 19, "the unsafe-path battery changed size");
    println!("PASS unsafe: {rejected} unsafe path and link shapes rejected without cleanup, plus the CLI rejection path");
}

// --------------------------------------------------------- 4. interrupted drafts
#[test]
fn interrupted_private_drafts_stay_inspectable_and_are_never_valid() {
    // (a) the child entry moved, the authored sequences were not updated yet.
    let dir = temp("interrupt-a");
    fixture(dir.path());
    let root = dir.path();
    run("mv", &[
        &child_path(root, R, C).to_string_lossy(),
        &child_path(root, D, C).to_string_lossy(),
    ]);
    let got = err_at(root);
    assert!(got.contains("missing, duplicate or non-child order member"), "half-move reason: {got}");
    assert!(child_path(root, D, C).symlink_metadata().is_ok(), "the interrupted entry disappeared");
    sentinels_untouched(root, "interrupted move");

    // (b) the sequences were updated, the entry was not moved yet.
    let dir = temp("interrupt-b");
    fixture(dir.path());
    let root = dir.path();
    order(root, R, &[A, B]);
    order(root, D, &[C]);
    assert!(err_at(root).contains("missing, duplicate or non-child order member"));
    assert!(child_path(root, R, C).symlink_metadata().is_ok());

    // (c) a truncated authoring write, at three different points.
    for cut in [12usize, 40, 63] {
        let dir = temp("interrupt-c");
        fixture(dir.path());
        let path = node_file(dir.path(), A);
        let full = fs::read(&path).unwrap();
        fs::write(&path, &full[..cut]).unwrap();
        let got = err_at(dir.path());
        assert!(
            got.contains("missing Markdown closing fence")
                || got.contains("unterminated Markdown metadata")
                || got.contains("missing required metadata")
                || got.contains("malformed metadata line")
                || got.contains("at line 1 column"),
            "a truncated write at {cut} gave an unexpected reason: {got:?}"
        );
        assert_eq!(fs::read(&path).unwrap(), full[..cut], "the truncated file was repaired");
        sentinels_untouched(dir.path(), "truncated write");
    }

    // (d) a store changed after it was read is never applied.
    let dir = temp("interrupt-d");
    fixture(dir.path());
    let before = get(dir.path());
    let mut edited = fs::read_to_string(node_file(dir.path(), A)).unwrap();
    edited.push_str("an ordinary save landed after the store was read\n");
    fs::write(node_file(dir.path(), A), &edited).unwrap();
    let err = apply(
        &policy(),
        &before,
        &Operation::Move { node: id(C), parent: id(D) },
    )
    .expect_err(
    "apply accepted a store changed after reading");
    assert!(err.to_string().contains("candidate changed after checking"), "reason: {err}");
    assert_eq!(fs::read_to_string(node_file(dir.path(), A)).unwrap(), edited, "the interrupted save was overwritten");
    assert!(child_path(dir.path(), D, C).symlink_metadata().is_err(), "apply created the destination entry anyway");

    // (e) the interrupted draft is recoverable by finishing it, and until then
    //     the actual CLI refuses to call it the result of the operation.
    let base = temp("interrupt-e-before");
    fixture(base.path());
    let draft = temp("interrupt-e-after");
    fixture(draft.path());
    let root = draft.path();
    run("mv", &[
        &child_path(root, R, C).to_string_lossy(),
        &child_path(root, D, C).to_string_lossy(),
    ]);
    order(root, R, &[A, B]);
    let args = move_args(base.path(), root);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (ok, _, stderr) = run_cli(&refs);
    assert!(!ok, "the CLI accepted a half-finished draft: {stderr}");
    assert!(stderr.contains("rejected:"), "unexpected CLI output: {stderr}");
    assert!(child_path(root, D, C).symlink_metadata().is_ok(), "the draft was cleaned up");
    order(root, D, &[C]);
    let (ok, stdout, stderr) = run_cli(&refs);
    assert!(ok, "the completed draft must validate: {stderr}");
    assert!(stdout.contains("exactly matches"), "unexpected CLI output: {stdout}");
    assert!(child_path(root, R, C).symlink_metadata().is_err());
    println!("PASS interrupted: half-moved, half-ordered, truncated (3 cuts), post-read and recoverable drafts judged as prescribed");
}

fn child_path(root: &Path, parent: usize, child: usize) -> PathBuf {
    root.join(format!("nodes/{}/children/{}", id(parent), id(child)))
}

fn move_args(before: &Path, after: &Path) -> Vec<String> {
    vec![
        "validate-draft".to_owned(),
        before.to_string_lossy().into_owned(),
        after.to_string_lossy().into_owned(),
        "move".to_owned(),
        id(C),
        id(D),
    ]
}

// ----------------------------------------------- 5. native save and restoration
#[test]
fn native_save_and_ordinary_git_restoration_reproduce_the_original_bytes() {
    let dir = temp("git");
    let root = dir.path();
    fixture(root);
    git(root, &["init", "-q"]);
    git(root, &["add", "-A"]);
    git(root, &["-c", "user.email=q2@example", "-c", "user.name=q2", "commit", "-qm", "original"]);

    let original = get(root);
    let original_digest = inventory_digest(&original);
    let original_bytes: Vec<(PathBuf, Vec<u8>)> = [R, A, B, C, S, D]
        .iter()
        .map(|&n| (node_file(root, n), fs::read(node_file(root, n)).unwrap()))
        .collect();

    // Positive operation through the real CLI and the compiled checked policy.
    let root_s = root.to_string_lossy().into_owned();
    let (ic, id_d) = (id(C), id(D));
    let (ok, stdout, stderr) = run_cli(&["move", &root_s, &ic, &id_d]);
    assert!(ok, "CLI move failed: {stderr}");
    let view: serde_json::Value = serde_json::from_str(&stdout).expect("CLI prints the inspected graph");
    let d_row = view["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == serde_json::json!(id(D)))
        .unwrap();
    assert_eq!(d_row["children"], serde_json::json!([id(C)]));
    let moved = get(root);
    assert_eq!(moved.children[&id(R)], vec![id(A), id(B)]);
    assert_eq!(moved.nodes, original.nodes, "a move changed authored content");

    let status = git(root, &["status", "--porcelain"]);
    let mut touched: Vec<String> = status.lines().map(|l| l[3..].trim().to_owned()).collect();
    touched.sort();
    let mut expected = vec![
        format!("nodes/{}/children.order", id(D)),
        format!("nodes/{}/children.order", id(R)),
        format!("nodes/{}/children/{}", id(D), id(C)),
        format!("nodes/{}/children/{}", id(R), id(C)),
    ];
    expected.sort();
    assert_eq!(touched, expected, "Git reported an unexpected change set");

    // Every authored payload is byte-identical; only relationships moved.
    for (path, bytes) in &original_bytes {
        assert_eq!(&fs::read(path).unwrap(), bytes, "{} changed", path.display());
    }

    // Ordinary Git restoration reproduces the original bytes exactly.
    git(root, &["checkout", "--", "."]);
    git(root, &["clean", "-fdq"]);
    let restored = get(root);
    assert_eq!(inventory_digest(&restored), original_digest, "Git restore did not reproduce the store");
    assert_eq!(restored.children, original.children);
    assert_eq!(restored.nodes, original.nodes);
    assert_eq!(git(root, &["status", "--porcelain"]), "");

    // An ordinary editor save is readable, byte-faithful, and the only file Git
    // reports as changed.
    let mut edited = fs::read_to_string(node_file(root, B)).unwrap();
    let split = body_start(&edited);
    edited.push_str("\nAn ordinary appended paragraph — é 中 🦀\n");
    fs::write(node_file(root, B), &edited).unwrap();
    let after_save = get(root);
    assert_eq!(after_save.nodes[&id(B)].content, edited[split..]);
    policy()
        .validate(&after_save)
        .expect("an ordinary body edit must still validate");
    assert_eq!(
        git(root, &["diff", "--name-only"]).trim(),
        format!("nodes/{}/node.md", id(B))
    );
    let (ok, stdout, stderr) = run_cli(&["inspect", &root_s]);
    assert!(ok, "CLI inspect failed: {stderr}");
    assert!(stdout.contains("ordinary appended paragraph"), "inspect lost the body");
    git(root, &["checkout", "--", "."]);
    assert_eq!(inventory_digest(&get(root)), original_digest, "the body edit was not restorable");

    // A completed native rename draft validates through the CLI.
    let original_name_file = fs::read(node_file(root, A)).unwrap();
    let new_name = "Native rename 中 🦀";
    let mut renamed = String::from_utf8(original_name_file.clone()).unwrap();
    let span = name_span(&renamed);
    renamed.replace_range(span, &serde_json::to_string(new_name).unwrap());
    fs::write(node_file(root, A), renamed.as_bytes()).unwrap();
    let id_a = id(A);
    let draft = vec!["validate-draft", root_s.as_str(), root_s.as_str(), "rename", id_a.as_str(), new_name];
    let (ok, stdout, stderr) = run_cli(&draft);
    assert!(ok, "the exact native rename draft must validate: {stderr}");
    assert!(stdout.contains("exactly matches"));

    // The same graph in a different byte spelling must not validate.
    let json_name = serde_json::to_string(new_name).unwrap();
    let decoded = serde_json::from_str::<String>(&json_name).unwrap();
    assert_eq!(decoded, new_name);
    let escaped = json_name
        .replace('中', "\\u4e2d")
        .replace("\u{1f980}", "\\ud83e\\udd80");
    let mut reserialised = String::from_utf8(original_name_file.clone()).unwrap();
    let span = name_span(&reserialised);
    reserialised.replace_range(span, &escaped);
    fs::write(node_file(root, A), reserialised.as_bytes()).unwrap();
    let readback = get(root);
    assert_eq!(readback.nodes[&id(A)].name, new_name, "an escaped name must decode identically");
    let (ok, _, stderr) = run_cli(&draft);
    assert!(!ok, "a same-graph different-bytes rename draft must not validate");
    assert!(stderr.contains("native draft differs"), "unexpected reason: {stderr}");

    fs::write(node_file(root, A), &original_name_file).unwrap();
    assert_eq!(inventory_digest(&get(root)), original_digest);
    println!(
        "PASS native-save: CLI move changed exactly {} relationship paths, all payloads byte-identical, Git restore reproduced the inventory, escaped rename draft refused",
        expected.len()
    );
}

// -------------------------------------------- 6. hostile checked subprocess
/// Keep the standalone Rust mutation fixture alive and compile it only once,
/// even when the two hostile-policy consumers run concurrently.
fn hostile_transformer() -> &'static Path {
    static FIXTURE: std::sync::OnceLock<(TempDir, PathBuf)> = std::sync::OnceLock::new();
    &FIXTURE.get_or_init(|| {
        let dir = temp("hostile-transformer");
        let exe = dir.path().join("hostile-policy");
        let output = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .args(["--edition=2024", "--crate-name", "hostile_policy"])
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/common/hostile_policy.rs"))
            .arg("-o").arg(&exe).output().unwrap();
        assert!(output.status.success(), "Rust fixture compilation: {}", String::from_utf8_lossy(&output.stderr));
        (dir, exe)
    }).1
}

/// Build a project whose checked executable is a controlled wrapper around the
/// genuine compiled policy, deliberately defecting its verdict.
fn hostile_project(mode: &str, pos: usize, dump: Option<&Path>) -> (TempDir, PathBuf) {
    let dir = temp("hostile");
    fs::create_dir(dir.path().join("build")).unwrap();
    let exe = dir.path().join("build/graph-policy");
    let real = Path::new(env!("CARGO_MANIFEST_DIR")).join("build/graph-policy");
    let quote = |path: &Path| format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"));
    let script = format!(
        "#!/bin/sh\nOUT=\"$({} \"$@\")\" || exit 7\nprintf '%s\\n' \"$OUT\" | {} '{mode}' {pos} {}\n",
        quote(&real), quote(hostile_transformer()),
        quote(dump.unwrap_or_else(|| Path::new("")))
    );
    fs::write(&exe, &script).unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
    let mut stamp = String::new();
    for file in ["Cargo.toml", "Cargo.lock", "README.md", ".gitignore"] {
        fs::write(dir.path().join(file), "q2 hostile fixture\n").unwrap();
        stamp.push_str(&format!("{}  {file}\n", sha256(b"q2 hostile fixture\n")));
    }
    fs::write(dir.path().join("build/candidate.sha256"), stamp).unwrap();
    fs::write(
        dir.path().join("build/graph-policy.sha256"),
        format!("{}  build/graph-policy\n", sha256(&fs::read(&exe).unwrap())),
    )
    .unwrap();
    (dir, exe)
}

/// Restore the shared fixture store after a hostile attempt possibly moved the
/// C entry, so each iteration judges the same independent baseline.
fn repair(root: &Path, digest: &str) {
    if Store::read(root).is_ok_and(|s| inventory_digest(&s) == digest) {
        return;
    }
    let entry_d = child_path(root, D, C);
    let entry_r = child_path(root, R, C);
    if entry_d.symlink_metadata().is_ok() {
        fs::remove_file(&entry_d).unwrap();
    }
    if entry_r.symlink_metadata().is_err() {
        link(root, R, C);
    }
    order(root, R, &[A, B, C]);
    order(root, A, &[D]);
    order(root, D, &[]);
    assert_eq!(inventory_digest(&get(root)), digest, "repair did not restore the baseline");
}

#[test]
fn a_changed_checked_result_can_never_be_applied() {
    let store_dir = temp("hostile-store");
    fixture(store_dir.path());
    let store = get(store_dir.path());
    let clean = inventory_digest(&store);
    let op = || Operation::Move { node: id(C), parent: id(D) };

    // Control: a wrapper that passes the genuine verdict through is accepted, so
    // every rejection below is about the change, not about the harness.
    let dump = temp("hostile-dump");
    let dump_path = dump.path().join("verdict.txt");
    let (pass_dir, _) = hostile_project("passthrough", 0, Some(&dump_path));
    let pass_policy = Policy::checked(pass_dir.path()).unwrap();
    let applied = apply(&pass_policy, &store, &op()).expect("the genuine checked verdict must apply");
    assert_eq!(applied.children[&id(D)], vec![id(C)]);
    repair(store_dir.path(), &clean);

    let words = fs::read_to_string(&dump_path).unwrap().split_whitespace().count();
    assert!(words >= 20, "unexpected verdict shape: {words} words");

    // Every single-word mutation of the genuine verdict is refused.
    let mut refused = 0;
    for pos in 1..=words {
        let (dir, _) = hostile_project("plus1", pos, None);
        let p = Policy::checked(dir.path()).unwrap();
        let store = get(store_dir.path());
        if apply(&p, &store, &op()).is_ok() {
            panic!("a hostile verdict with word {pos} incremented was accepted");
        }
        refused += 1;
        // A refused operation may leave at most an invalid private draft: it can
        // never leave a store that reads as a valid result of that operation.
        match Store::read(store_dir.path()) {
            Ok(after) => assert_eq!(
                inventory_digest(&after),
                clean,
                "word {pos} mutation was refused but still changed the store"
            ),
            Err(_) => {
                assert_eq!(
                    fs::read(node_file(store_dir.path(), A)).unwrap(),
                    node_md(A, &UNKNOWN_A, BODY_A),
                    "word {pos} mutation destroyed an authored payload"
                );
                sentinels_untouched(store_dir.path(), &format!("hostile word {pos}"));
            }
        }
        repair(store_dir.path(), &clean);
    }
    assert_eq!(refused, words, "not every verdict word was load-bearing");
    let mutations = refused;

    // Named defects and their prescribed reasons, on the same wrapper.
    let store = get(store_dir.path());
    for (mode, pos, want) in [
        ("empty", 0, "checked operation rejected or malformed verdict"),
        ("stderr", 0, "unexpected policy stderr"),
        ("fail", 0, "policy process exited nonzero"),
        ("extra", 0, "trailing process output"),
        ("drop", 1, "checked operation rejected or malformed verdict"),
        ("huge", 3, "out-of-envelope process integer"),
        ("plus1", 3, "process changed assets, roots or node inventory"),
    ] {
        let (dir, _) = hostile_project(mode, pos, None);
        let p = Policy::checked(dir.path()).unwrap();
        let err = apply(&p, &store, &op())
            .err()
            .unwrap_or_else(|| panic!("{mode}@{pos}: the hostile verdict was accepted"))
            .to_string();
        assert!(
            err.contains(want) || (want.contains("malformed verdict") && (err.contains("truncated process graph") || err.contains("trailing process output"))),
            "{mode}@{pos}: reason was {err:?}, want {want:?}"
        );
        assert_eq!(inventory_digest(&get(store_dir.path())), clean, "{mode}@{pos}: the store was mutated");
    }

    // A process that cannot be started, and an executable changed after stamping.
    assert!(invoke(Path::new("/definitely/not/here"), b"1 1 1 1").is_err());
    let (dir, exe) = hostile_project("passthrough", 0, None);
    let checked = Policy::checked(dir.path()).unwrap();
    append_bytes(&exe, b"# tampered\n");
    let err = checked.validate(&store).expect_err("a tampered executable still validated");
    assert!(err.to_string().contains("checked candidate changed"), "reason: {err}");

    // A hung process is refused by the bounded deadline and cannot complete a
    // native effect.
    let (dir, exe) = hostile_project("passthrough", 0, None);
    fs::write(&exe, "#!/bin/sh\nsleep 30\n").unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        dir.path().join("build/graph-policy.sha256"),
        format!("{}  build/graph-policy\n", sha256(&fs::read(&exe).unwrap())),
    )
    .unwrap();
    let p = Policy::checked(dir.path()).unwrap();
    let started = std::time::Instant::now();
    let err = apply(&p, &store, &op())
        .expect_err(
        "a hung policy process must not succeed");
    assert!(err.to_string().contains("policy process timed out"), "reason: {err}");
    assert!(started.elapsed().as_secs() < 20, "the deadline did not bound the call");
    assert_eq!(inventory_digest(&get(store_dir.path())), clean, "a timed-out call mutated the store");
    println!(
        "PASS hostile: {mutations} single-word verdict mutations refused, 7 named subprocess defects refused with their reasons, a hung process timed out without a native effect"
    );
}

// --------------------------------------- 7. candidate integrity after review
#[test]
fn candidate_integrity_is_rechecked_after_validation() {
    let dir = temp("candidate");
    fixture(dir.path());
    let p = policy();
    let store = get(dir.path());
    p.validate(&store).unwrap();

    // The reviewed candidate changes between validation and application.
    let mut edited = fs::read_to_string(node_file(dir.path(), B)).unwrap();
    edited.push_str("written after the store was reviewed\n");
    fs::write(node_file(dir.path(), B), &edited).unwrap();
    let err = apply(
        &p,
        &store,
        &Operation::Move { node: id(C), parent: id(D) },
    )
    .expect_err(
    "a candidate changed after review was applied");
    assert!(err.to_string().contains("candidate changed after checking"), "reason: {err}");
    assert_eq!(fs::read_to_string(node_file(dir.path(), B)).unwrap(), edited, "the change was reverted");
    assert!(child_path(dir.path(), D, C).symlink_metadata().is_err(), "the native effect ran anyway");

    // Build-stamp hygiene: tampering with the stamped candidate is caught when
    // the policy is next used, not only when it is first built.
    let store_dir = temp("candidate-store");
    fixture(store_dir.path());
    let store = get(store_dir.path());
    let (proj, exe) = hostile_project("passthrough", 0, None);
    let checked = Policy::checked(proj.path()).unwrap();
    checked.validate(&store).unwrap();
    append_bytes(&exe, b"# tampered\n");
    let err = checked.validate(&store).expect_err("a tampered executable still validated");
    assert!(err.to_string().contains("checked candidate changed"), "reason: {err}");
    assert!(Policy::checked(proj.path()).is_err(), "a tampered executable passed the stamp");

    let (proj2, _) = hostile_project("passthrough", 0, None);
    let checked2 = Policy::checked(proj2.path()).unwrap();
    checked2.validate(&store).unwrap();
    fs::create_dir(proj2.path().join("src")).unwrap();
    fs::write(proj2.path().join("src/later.rs"), "added after review").unwrap();
    let err = checked2.validate(&store).expect_err("an added candidate file passed the stamp");
    assert!(err.to_string().contains("candidate file set changed"), "reason: {err}");

    // An unsafe path, a duplicate entry, a malformed line and an empty stamp all
    // fail closed before any process is called.
    let stamp_sha = sha256(b"q2 hostile fixture\n");
    for (label, body, want) in [
        ("empty", String::new(), "empty build stamp"),
        ("malformed", "deadbeef build/graph-policy\n".to_owned(), "malformed build stamp"),
        ("absolute", format!("{stamp_sha}  {}\n", proj2.path().join("Cargo.toml").display()), "unsafe build stamp path"),
        ("escaping", format!("{stamp_sha}  ../escape\n"), "unsafe build stamp path"),
        ("duplicate", format!("{stamp_sha}  Cargo.toml\n{stamp_sha}  Cargo.toml\n"), "duplicate build stamp entry"),
    ] {
        let (dir3, _) = hostile_project("passthrough", 0, None);
        fs::write(dir3.path().join("build/candidate.sha256"), &body).unwrap();
        let err = Policy::checked(dir3.path()).err().unwrap_or_else(|| panic!("{label}: stamp accepted"));
        assert!(err.to_string().contains(want), "{label}: reason {err}, want {want}");
    }

    // A valid draft judged twice: the second judgement must see the change.
    let before = temp("draft-before");
    fixture(before.path());
    let after = temp("draft-after");
    fixture(after.path());
    let root = after.path();
    run("mv", &[
        &child_path(root, R, C).to_string_lossy(),
        &child_path(root, D, C).to_string_lossy(),
    ]);
    order(root, R, &[A, B]);
    order(root, D, &[C]);
    let p = policy();
    let from = get(before.path());
    let to = get(root);
    let op = Operation::Move { node: id(C), parent: id(D) };
    validate_draft(&p, &from, &to, &op).expect("the completed native draft must validate");

    // Somebody edits the candidate after it was validated.
    let mut late = fs::read_to_string(node_file(root, A)).unwrap();
    late.push_str("edited after validation\n");
    fs::write(node_file(root, A), &late).unwrap();
    let changed = get(root);
    validate_draft(&p, &from, &changed, &op)
        .expect_err(
        "a candidate changed after validation still matched");
    assert_eq!(
        inventory_digest(&get(before.path())),
        inventory_digest(&from),
        "the before store was touched"
    );
    println!("PASS candidate-integrity: post-review change, tampered executable, added candidate file, 5 stamp defects and a post-validation draft change all refused");
}
