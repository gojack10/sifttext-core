// Generate the executable's seed from the canonical, readable native files.
use std::{env, fs, path::Path};

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

fn main() {
    let root = env::var_os("CARGO_MANIFEST_DIR").unwrap();
    let root = Path::new(&root);
    let (mut dirs, mut files, mut links) = (String::new(), String::new(), String::new());
    for name in ["nodes", "trees"] {
        walk(root, &root.join(name), &mut dirs, &mut files, &mut links);
    }
    let out = fs::canonicalize(env::var_os("OUT_DIR").unwrap()).unwrap();
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
