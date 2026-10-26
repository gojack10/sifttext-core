// Generate the executable's seed from the canonical, readable native files.
use std::{
    env, fs,
    io::{BufWriter, Write},
    path::Path,
    process::Command,
};

fn ordinary_vocabulary(out: &Path) {
    // tiktoken-rs =0.6.0 o200k_base has 199_998 ordinary ranks, 0..199_998
    // (exclusive). Rank 199_998 is absent; 199_999 and 200_018 are special.
    const COUNT: u32 = 199_998;
    let bpe = tiktoken_rs::o200k_base().expect("pinned o200k_base vocabulary");
    let mut packed = BufWriter::new(fs::File::create(out.join("o200k-ordinary.bin")).unwrap());
    packed.write_all(&COUNT.to_le_bytes()).unwrap();
    let mut single_bytes = [false; 256];
    let mut count = 0u32;
    // Missing/incompatible ranks panic in the public decoder, never truncate.
    // CoreBPE construction checks rank uniqueness; encoder keys are unique bytes.
    for token in bpe._decode_native_and_split((0..COUNT).collect()) {
        assert!(!token.is_empty(), "empty ordinary token");
        if token.len() == 1 {
            assert!(!single_bytes[token[0] as usize], "duplicate byte token");
            single_bytes[token[0] as usize] = true;
        }
        let len = u32::try_from(token.len()).expect("ordinary token length fits u32");
        packed.write_all(&len.to_le_bytes()).unwrap();
        packed.write_all(&token).unwrap();
        count = count
            .checked_add(1)
            .expect("ordinary vocabulary size overflow");
    }
    assert_eq!(count, COUNT, "incompatible ordinary vocabulary count");
    assert!(
        single_bytes.into_iter().all(|present| present),
        "missing byte token"
    );
    packed.flush().unwrap();
}

fn walk(root: &Path, path: &Path, dirs: &mut String, files: &mut String, links: &mut String) {
    println!("cargo:rerun-if-changed={}", path.display());
    let name = path.strip_prefix(root).unwrap().to_str().unwrap();
    let meta = fs::symlink_metadata(path).unwrap();
    if meta.is_symlink() {
        let target = fs::read_link(path).unwrap();
        links.push_str(&format!("({name:?}, {:?}),\n", target.to_str().unwrap()));
    } else if meta.is_dir() {
        dirs.push_str(&format!("{name:?},\n"));
        let mut children: Vec<_> = fs::read_dir(path)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        children.sort();
        for child in children {
            walk(root, &child, dirs, files, links);
        }
    } else {
        assert!(meta.is_file(), "unsupported KB entry: {name}");
        files.push_str(&format!(
            "({name:?}, include_str!({:?})),\n",
            path.to_str().unwrap()
        ));
    }
}

fn watch_tree(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
    if path.is_dir() {
        for entry in fs::read_dir(path).unwrap() {
            watch_tree(&entry.unwrap().path());
        }
    }
}

fn git_output(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())
        .flatten()
}

fn source_revision(root: &Path) -> Option<String> {
    let revision = git_output(root, &["rev-parse", "--verify", "HEAD"])?
        .trim()
        .to_owned();
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    if !git_output(root, &["status", "--porcelain", "--untracked-files=all"])?.is_empty() {
        return None;
    }
    Some(revision)
}

fn main() {
    let root = env::var_os("CARGO_MANIFEST_DIR").unwrap();
    let root = Path::new(&root);
    let (mut dirs, mut files, mut links) = (String::new(), String::new(), String::new());
    for name in ["src", "tests", "laws", "scripts", "docs", "fixtures"] {
        watch_tree(&root.join(name));
    }
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "README.md",
        "AGENTS.md",
        "LICENSE",
        ".gitignore",
        "rust-toolchain.toml",
        "install-local.sh",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(name).display());
    }
    if let Some(head) = git_output(&root, &["rev-parse", "--git-path", "HEAD"]) {
        println!(
            "cargo:rerun-if-changed={}",
            root.join(head.trim()).display()
        );
    }
    if let Some(reference) = git_output(&root, &["symbolic-ref", "--quiet", "HEAD"]) {
        if let Some(path) = git_output(&root, &["rev-parse", "--git-path", reference.trim()]) {
            println!(
                "cargo:rerun-if-changed={}",
                root.join(path.trim()).display()
            );
        }
    }
    let revision = source_revision(&root).unwrap_or_default();
    println!("cargo:rustc-env=SIFTTEXT_SOURCE_REVISION={revision}");
    for name in ["nodes", "trees"] {
        walk(root, &root.join(name), &mut dirs, &mut files, &mut links);
    }
    let out = fs::canonicalize(env::var_os("OUT_DIR").unwrap()).unwrap();
    ordinary_vocabulary(&out);
    // Pin the physical build location; runtime must not follow later target symlink edits.
    println!(
        "cargo:rustc-env=SIFTTEXT_BUILD_BIN_DIR={}",
        out.ancestors().nth(3).unwrap().display()
    );
    fs::write(
        out.join("native_kb.rs"),
        format!("const DIRS: &[&str] = &[{dirs}];\nconst FILES: &[(&str, &str)] = &[{files}];\nconst LINKS: &[(&str, &str)] = &[{links}];\n"),
    ).unwrap();
}
