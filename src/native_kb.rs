//! First-party seed compiled from the readable, Git-tracked `nodes/` and `trees/`.
//! The installed executable never needs the build checkout or a private KB path.
use crate::{Policy, Result, local};
use serde_json::Value;
use std::{fs, os::unix::fs::symlink, path::Path};

/// Existing homes are recognized before even materializing the default seed.
pub fn init(policy: &Policy, home: &Path) -> Result<Value> {
    if fs::symlink_metadata(home).is_ok() {
        return local::init(policy, home, None);
    }
    let seed = tempfile::tempdir()?;
    materialize(seed.path())?;
    local::init(policy, home, Some(seed.path()))
}

fn materialize(path: &Path) -> Result<()> {
    for dir in DIRS {
        fs::create_dir(path.join(dir))?;
    }
    for (name, text) in FILES {
        fs::write(path.join(name), text.as_bytes())?;
    }
    for (name, target) in LINKS {
        symlink(target, path.join(name))?;
    }
    Ok(())
}

/// Check new readable assets without changing the frozen stamp file set/recipe.
/// Source and installed onramp runtimes must carry exactly the compiled KB.
/// Generic policy-only callers keep their existing stamp contract.
pub fn verify_assets(root: &Path) -> Result<()> {
    use crate::Entry;
    use std::{collections::BTreeMap, path::PathBuf};
    let mut expected = BTreeMap::new();
    for dir in DIRS {
        expected.insert(PathBuf::from(dir), Entry::Dir);
    }
    for (name, text) in FILES {
        expected.insert(PathBuf::from(name), Entry::File(text.as_bytes().to_vec()));
    }
    for (name, target) in LINKS {
        expected.insert(PathBuf::from(name), Entry::Link(PathBuf::from(target)));
    }
    fn walk(root: &Path, rel: &Path, expected: &BTreeMap<PathBuf, Entry>) -> Result<()> {
        let path = root.join(rel);
        match expected.get(rel) {
            Some(Entry::Dir) => {
                crate::real_dir(&path)?;
                for entry in fs::read_dir(&path)? {
                    walk(root, &rel.join(entry?.file_name()), expected)?;
                }
            }
            Some(Entry::File(bytes)) => crate::require(
                crate::read_regular(&path)? == *bytes,
                "bundled KB changed; rebuild and recheck",
            )?,
            Some(Entry::Link(target)) => crate::require(
                fs::read_link(path)? == *target,
                "bundled KB link changed; rebuild and recheck",
            )?,
            None => return Err("bundled KB inventory changed; rebuild and recheck".into()),
        }
        Ok(())
    }
    for dir in ["nodes", "trees"] {
        walk(root, Path::new(dir), &expected)?;
    }
    // Walking refuses extras; iterating also detects missing files/empty dirs.
    for rel in expected.keys() {
        crate::require(
            root.join(rel).symlink_metadata().is_ok(),
            "bundled KB entry missing",
        )?;
    }
    for (name, text) in [
        ("build.rs", include_str!("../build.rs")),
        ("AGENTS.md", include_str!("../AGENTS.md")),
        ("install-local.sh", include_str!("../install-local.sh")),
    ] {
        crate::require(
            crate::read_regular(&root.join(name))? == text.as_bytes(),
            "bundled bootstrap changed; rebuild and recheck",
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/successor/default_native_kb.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/successor/local_authoring_seed.rs"]
mod local_authoring_tests;

#[cfg(test)]
#[path = "../tests/successor/local_capabilities_seed.rs"]
mod local_capabilities_tests;

#[cfg(test)]
#[path = "../tests/successor/local_onboarding_seed.rs"]
mod local_onboarding_tests;

// Generated at build time; authored text exists only in the native files.
include!(concat!(env!("OUT_DIR"), "/native_kb.rs"));
