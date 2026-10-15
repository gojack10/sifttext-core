use crate::{
    Policy, Store,
    index::{self, Mode},
    outlines::{self, ResidueProof},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

const NAMES: [&str; 4] = [
    "outline.lean",
    "outline.full.lean",
    "outline.lean.meta.json",
    "outline.full.lean.meta.json",
];
fn id(n: usize) -> String {
    format!("84000000-0000-4000-8000-{n:012}")
}
fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}
fn fixture() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir(d.path().join("trees")).unwrap();
    for n in 1..=3 {
        let dir = d.path().join("nodes").join(id(n));
        fs::create_dir_all(dir.join("children")).unwrap();
        let fields = json!({"id":id(n),"name":format!("Residue {n} Ω"),"micro_summary":"Exact nonblank summary\nnext line.","warnings":"Preserve \"quotes\", slash \\, tabs\t and Unicode Ω.","status":"in_progress","see_also":[]});
        let text = fields
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| format!("{k}: {v}\n"))
            .collect::<String>();
        fs::write(
            dir.join("node.md"),
            format!("---\n{text}---\nAuthored protected body.\n"),
        )
        .unwrap();
        let children = if n < 3 {
            id(n + 1) + "\n"
        } else {
            String::new()
        };
        fs::write(dir.join("children.order"), children).unwrap();
        if n < 3 {
            std::os::unix::fs::symlink(
                format!("../../{}", id(n + 1)),
                dir.join("children").join(id(n + 1)),
            )
            .unwrap();
        }
    }
    std::os::unix::fs::symlink(
        format!("../nodes/{}", id(1)),
        d.path().join("trees").join(id(1)),
    )
    .unwrap();
    d
}
fn refresh(path: &Path, base: Option<&Path>) {
    assert_eq!(
        index::refresh(&policy(), path, base, Mode::Incremental, None)
            .unwrap()
            .status,
        "current"
    );
}
fn relative(name: &str) -> PathBuf {
    PathBuf::from("nodes").join(id(1)).join(name)
}
fn residue(source: &Path) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir_all(d.path().join("nodes").join(id(1))).unwrap();
    for name in NAMES {
        fs::copy(source.join(relative(name)), d.path().join(relative(name))).unwrap();
    }
    d
}
fn setup() -> (tempfile::TempDir, tempfile::TempDir, ResidueProof) {
    let source = fixture();
    refresh(source.path(), None);
    let proof = ResidueProof::capture(&policy(), source.path(), None, &id(1)).unwrap();
    let residue = residue(source.path());
    (source, residue, proof)
}
#[test]
fn source_grounded_shallow_full_and_historical_not_current() {
    let (source, residue, proof) = setup();
    let before = Store::read(source.path()).unwrap().inventory;
    let version = outlines::inspect(&policy(), source.path(), None)
        .unwrap()
        .version
        .unwrap();
    assert_eq!(proof.source_version(), version);
    for name in NAMES {
        proof
            .recognize(residue.path(), &relative(name))
            .unwrap()
            .unchanged()
            .unwrap();
    }
    assert_eq!(Store::read(source.path()).unwrap().inventory, before);
    // Source-only body changes advance the accepted global identity, not the
    // old proof's historical claim. The residue never becomes a current view.
    let node = source.path().join("nodes").join(id(3)).join("node.md");
    let mut text = fs::read_to_string(&node).unwrap();
    text.push_str("Later authored bytes.\n");
    fs::write(&node, text).unwrap();
    assert_eq!(
        outlines::inspect(&policy(), source.path(), None)
            .unwrap()
            .status,
        "stale"
    );
    assert!(ResidueProof::capture(&policy(), source.path(), None, &id(1)).is_err());
    refresh(source.path(), None);
    assert_ne!(
        proof.source_version(),
        outlines::inspect(&policy(), source.path(), None)
            .unwrap()
            .version
            .unwrap()
    );
    for name in NAMES {
        proof.recognize(residue.path(), &relative(name)).unwrap();
    }
    // Removed authored owners cannot manufacture a proof from residue alone.
    assert!(ResidueProof::capture(&policy(), residue.path(), None, &id(1)).is_err());
}
#[test]
fn forged_truncated_costs_versions_coverage_and_protected_bytes_rejected() {
    let (source, residue, proof) = setup();
    for name in NAMES {
        let path = residue.path().join(relative(name));
        let original = fs::read(&path).unwrap();
        for bytes in [
            b"protected authored bytes".to_vec(),
            original[..original.len() / 2].to_vec(),
            b"{}\n".to_vec(),
        ] {
            fs::write(&path, &bytes).unwrap();
            assert!(proof.recognize(residue.path(), &relative(name)).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        fs::write(&path, &original).unwrap();
    }
    let path = residue.path().join(relative("outline.full.lean"));
    let original = fs::read_to_string(&path).unwrap();
    for bytes in [
        original.replace("coverage:full", "coverage:shallow"),
        original.replace("source_version:", "fake_version:"),
        original.replace(&id(1), &id(3)),
    ] {
        assert_ne!(bytes, original);
        fs::write(&path, &bytes).unwrap();
        assert!(
            proof
                .recognize(residue.path(), &relative("outline.full.lean"))
                .is_err()
        );
        // Self-consistent hashes/costs are not evidence of canonical provenance.
        let (hash, tokens) = crate::outline_artifacts::cost(&bytes);
        fs::write(
            residue.path().join(relative("outline.full.lean.meta.json")),
            serde_json::to_vec(&json!({"hash":hash,"tokens":tokens,"encoding":"o200k_base"}))
                .unwrap(),
        )
        .unwrap();
        assert!(
            proof
                .recognize(residue.path(), &relative("outline.full.lean.meta.json"))
                .is_err()
        );
    }
    let meta = residue.path().join(relative("outline.lean.meta.json"));
    let mut value: Value = serde_json::from_slice(&fs::read(&meta).unwrap()).unwrap();
    value["tokens"] = json!(0);
    fs::write(&meta, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(
        proof
            .recognize(residue.path(), &relative("outline.lean.meta.json"))
            .is_err()
    );
    // Oversized inputs are bounded by proof length, not an attacker-sized read.
    fs::write(&path, vec![b'x'; original.len() * 100]).unwrap();
    assert!(
        proof
            .recognize(residue.path(), &relative("outline.full.lean"))
            .is_err()
    );
    assert_eq!(
        outlines::inspect(&policy(), source.path(), None)
            .unwrap()
            .status,
        "current"
    );
}
#[test]
fn no_unknown_nested_directory_owner_or_path_exemptions() {
    let (_, residue, proof) = setup();
    let protected = residue.path().join(relative("unknown.txt"));
    fs::write(&protected, "Keep me").unwrap();
    for relative in [
        relative("unknown.txt"),
        relative("node.md"),
        PathBuf::from("nodes").join(id(2)).join("outline.lean"),
        relative("outline.lean").join("nested"),
        relative("../outline.lean"),
        PathBuf::from("/outline.lean"),
    ] {
        assert!(proof.recognize(residue.path(), &relative).is_err());
    }
    let file = residue.path().join(relative("outline.lean"));
    fs::remove_file(&file).unwrap();
    fs::create_dir(&file).unwrap();
    fs::write(file.join("unknown"), "Keep nested").unwrap();
    assert!(
        proof
            .recognize(residue.path(), &relative("outline.lean"))
            .is_err()
    );
    assert_eq!(fs::read_to_string(&protected).unwrap(), "Keep me");
    assert_eq!(
        fs::read_to_string(file.join("unknown")).unwrap(),
        "Keep nested"
    );
}
#[test]
fn links_aliases_missing_and_replacement_receipts_reject() {
    for hard in [false, true] {
        let (_, residue, proof) = setup();
        let file = residue.path().join(relative("outline.lean"));
        let outside = tempfile::NamedTempFile::new().unwrap();
        let original = fs::read(&file).unwrap();
        fs::write(outside.path(), &original).unwrap();
        fs::remove_file(&file).unwrap();
        if hard {
            fs::hard_link(outside.path(), &file).unwrap();
        } else {
            std::os::unix::fs::symlink(outside.path(), &file).unwrap();
        }
        assert!(
            proof
                .recognize(residue.path(), &relative("outline.lean"))
                .is_err()
        );
        assert_eq!(fs::read(outside.path()).unwrap(), original);
    }
    let (_, residue, proof) = setup();
    let receipt = proof
        .recognize(residue.path(), &relative("outline.lean"))
        .unwrap();
    let path = residue.path().join(relative("outline.lean"));
    let bytes = fs::read(&path).unwrap();
    let replacement = tempfile::NamedTempFile::new_in(path.parent().unwrap()).unwrap();
    fs::write(replacement.path(), &bytes).unwrap();
    replacement.persist(&path).unwrap();
    assert!(receipt.unchanged().is_err());
    // New independent recognition may accept the exact historical bytes again.
    let receipt = proof
        .recognize(residue.path(), &relative("outline.lean"))
        .unwrap();
    fs::write(path.parent().unwrap().join("unknown"), "new resource").unwrap();
    assert!(receipt.unchanged().is_err());
    let alias = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(residue.path(), alias.path().join("alias")).unwrap();
    assert!(
        proof
            .recognize(&alias.path().join("alias"), &relative("outline.lean"))
            .is_err()
    );
    fs::remove_file(&path).unwrap();
    assert!(
        proof
            .recognize(residue.path(), &relative("outline.lean"))
            .is_err()
    );
    let nodes = residue.path().join("nodes");
    fs::rename(&nodes, residue.path().join("old-nodes")).unwrap();
    std::os::unix::fs::symlink(residue.path().join("old-nodes"), &nodes).unwrap();
    assert!(
        proof
            .recognize(residue.path(), &relative("outline.full.lean"))
            .is_err()
    );
}
#[test]
fn source_proof_rejects_forgery_invalid_store_and_capture_replacement() {
    let source = fixture();
    assert!(ResidueProof::capture(&policy(), source.path(), None, &id(1)).is_err());
    refresh(source.path(), None);
    for name in NAMES {
        let file = source.path().join(relative(name));
        let original = fs::read(&file).unwrap();
        fs::write(&file, "forged").unwrap();
        assert!(ResidueProof::capture(&policy(), source.path(), None, &id(1)).is_err());
        fs::write(file, original).unwrap();
    }
    assert!(ResidueProof::capture(&policy(), source.path(), None, &id(9)).is_err());
    assert!(ResidueProof::capture(&policy(), source.path(), None, "../bad").is_err());
    let path = source.path().join("nodes").join(id(1)).join("node.md");
    let original = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        original.replace("Exact nonblank summary\\nnext line.", " "),
    )
    .unwrap();
    assert!(ResidueProof::capture(&policy(), source.path(), None, &id(1)).is_err());
    fs::write(&path, &original).unwrap();
    // Actual shared-capture hook: same bytes, different inode during capture.
    crate::coherent_capture::on_inventory(move || {
        let replacement = tempfile::NamedTempFile::new_in(path.parent().unwrap()).unwrap();
        fs::write(replacement.path(), original).unwrap();
        replacement.persist(path).unwrap();
    });
    assert!(ResidueProof::capture(&policy(), source.path(), None, &id(1)).is_err());
    assert_eq!(
        outlines::inspect(&policy(), source.path(), None)
            .unwrap()
            .status,
        "current"
    );
}
#[test]
fn sparse_historical_owner_proof_and_parent_replacement() {
    let base = fixture();
    let sparse = fixture();
    fs::remove_dir_all(sparse.path().join("nodes").join(id(1))).unwrap();
    refresh(sparse.path(), Some(base.path()));
    let proof = ResidueProof::capture(&policy(), sparse.path(), Some(base.path()), &id(1)).unwrap();
    let residue = tempfile::tempdir().unwrap();
    let dir = residue.path().join("nodes").join(id(1));
    fs::create_dir_all(&dir).unwrap();
    for name in NAMES {
        fs::copy(
            sparse
                .path()
                .join(".sifttext/outlines")
                .join(id(1))
                .join(name),
            dir.join(name),
        )
        .unwrap();
    }
    let receipt = proof
        .recognize(residue.path(), &relative("outline.lean"))
        .unwrap();
    fs::rename(&dir, residue.path().join("old-owner")).unwrap();
    fs::create_dir(&dir).unwrap();
    for name in NAMES {
        fs::copy(residue.path().join("old-owner").join(name), dir.join(name)).unwrap();
    }
    assert!(receipt.unchanged().is_err());
    assert!(!base.path().join(".sifttext").exists());
}
