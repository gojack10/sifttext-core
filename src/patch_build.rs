//! Deterministic source-build inputs and local repair patches for managed updates.
use crate::{Result, require, sha256};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const PATCH_DIR: &str = ".sifttext/patches";
const INPUT_FILES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "build.rs",
    "rust-toolchain.toml",
    "AGENTS.md",
    "install-local.sh",
];
const INPUT_DIRS: &[&str] = &[".cargo", "src", "laws", "scripts", "nodes", "trees"];
const BUILD_ENV: &[&str] = &[
    "RUSTFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_BUILD_TARGET",
    "RUSTC",
    "RUSTUP_TOOLCHAIN",
    "RUSTC_WRAPPER",
    "CC",
    "CFLAGS",
    "AR",
    "LDFLAGS",
    "SOURCE_DATE_EPOCH",
    "CARGO_HOME",
];

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BuildReceipt {
    pub version: String,
    pub source: String,
    pub format: u32,
    pub inputs: String,
    pub executable: String,
}

fn visit(root: &Path, path: &Path, hash: &mut Sha256) -> Result<()> {
    let relative = path.strip_prefix(root)?;
    let name = relative.to_str().ok_or("non-UTF-8 build input path")?;
    let metadata = fs::symlink_metadata(path)?;
    hash.update(name.as_bytes());
    hash.update([0]);
    if metadata.file_type().is_symlink() {
        hash.update(b"link\0");
        hash.update(fs::read_link(path)?.as_os_str().as_encoded_bytes());
    } else if metadata.is_dir() {
        hash.update(b"dir\0");
        let mut children = fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        children.sort();
        for child in children {
            visit(root, &child, hash)?;
        }
    } else {
        require(metadata.is_file(), "unsupported source-build input")?;
        hash.update(b"file\0");
        hash.update(fs::read(path)?);
    }
    hash.update([0xff]);
    Ok(())
}

fn output(command: &mut Command, label: &str) -> Result<Vec<u8>> {
    let result = command
        .output()
        .map_err(|error| format!("{label}: {error}"))?;
    require(result.status.success(), label)?;
    Ok(result.stdout)
}

fn resolve_tool(name: &std::ffi::OsStr) -> Result<PathBuf> {
    let name = Path::new(name);
    let candidates = if name.components().count() > 1 {
        vec![name.to_owned()]
    } else {
        env::split_paths(&env::var_os("PATH").ok_or("PATH unavailable")?)
            .map(|dir| dir.join(name))
            .collect()
    };
    for candidate in candidates {
        if candidate.is_file() {
            return Ok(fs::canonicalize(candidate)?);
        }
    }
    Err(format!("build tool not found: {}", name.display()).into())
}

fn hash_tool(hash: &mut Sha256, name: &std::ffi::OsStr) -> Result<PathBuf> {
    let path = resolve_tool(name)?;
    hash.update(name.as_encoded_bytes());
    hash.update([0]);
    hash.update(path.as_os_str().as_encoded_bytes());
    hash.update([0]);
    // ponytail: hash executable bytes rather than relying on version strings, which can stay constant across replacement.
    hash.update(fs::read(&path)?);
    hash.update([0]);
    Ok(path)
}

fn hash_cargo_config_file(hash: &mut Sha256, path: &Path) -> Result<()> {
    match fs::read(path) {
        Ok(contents) => {
            hash.update([1]);
            hash.update(path.as_os_str().as_encoded_bytes());
            hash.update([0]);
            hash.update((contents.len() as u64).to_be_bytes());
            hash.update(contents);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn hash_cargo_config(root: &Path, hash: &mut Sha256) -> Result<()> {
    for ancestor in fs::canonicalize(root)?.ancestors().skip(1) {
        for name in ["config.toml", "config"] {
            hash_cargo_config_file(hash, &ancestor.join(".cargo").join(name))?;
        }
    }
    let home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
        .ok_or("Cargo home unavailable")?;
    hash.update(home.as_os_str().as_encoded_bytes());
    hash.update([0]);
    for name in ["config.toml", "config"] {
        hash_cargo_config_file(hash, &home.join(name))?;
    }
    Ok(())
}

/// Hash every checked source/build input plus the actual compiler/toolchain settings.
pub(crate) fn fingerprint(root: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    hash_cargo_config(root, &mut hash)?;
    for name in INPUT_FILES {
        let path = root.join(name);
        if path.exists() {
            visit(root, &path, &mut hash)?;
        }
    }
    for name in INPUT_DIRS {
        let path = root.join(name);
        if path.exists() {
            visit(root, &path, &mut hash)?;
        }
    }
    let rustc_name = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let rustc = hash_tool(&mut hash, &rustc_name)?;
    let cargo = hash_tool(&mut hash, std::ffi::OsStr::new("cargo"))?;
    hash_tool(&mut hash, std::ffi::OsStr::new("clang"))?;
    for key in ["RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER"] {
        if let Some(wrapper) = env::var_os(key) {
            hash.update(key.as_bytes());
            hash.update([0]);
            hash_tool(&mut hash, &wrapper)?;
        }
    }
    let linker_name = env::var_os("CC").unwrap_or_else(|| "cc".into());
    let linker_command = linker_name.to_string_lossy();
    let linker_name = linker_command.split_whitespace().next().ok_or("empty CC")?;
    let linker = hash_tool(&mut hash, linker_name.as_ref())?;
    let archiver = env::var_os("AR").unwrap_or_else(|| "ar".into());
    hash_tool(&mut hash, &archiver)?;
    for command in [
        output(
            Command::new(rustc).current_dir(root).arg("-Vv"),
            "rustc version unavailable",
        )?,
        output(
            Command::new(cargo).current_dir(root).arg("--version"),
            "cargo version unavailable",
        )?,
        output(
            Command::new(linker).current_dir(root).arg("--version"),
            "linker version unavailable",
        )?,
        {
            let bend = output(
                Command::new("sh")
                    .current_dir(root)
                    .arg("scripts/bend-path.sh"),
                "pinned Bend unavailable",
            )?;
            let path = PathBuf::from(String::from_utf8(bend)?.trim());
            let version = output(
                Command::new(path).current_dir(root).arg("version"),
                "pinned Bend version unavailable",
            )?;
            version
        },
    ] {
        hash.update(command);
        hash.update([0]);
    }
    for key in BUILD_ENV {
        hash.update(key.as_bytes());
        hash.update([0]);
        if let Some(value) = env::var_os(key) {
            hash.update(value.as_encoded_bytes());
        }
        hash.update([0xff]);
    }
    let mut target_linkers = env::vars_os()
        .filter(|(key, _)| {
            key.as_encoded_bytes().starts_with(b"CARGO_TARGET_")
                && key.as_encoded_bytes().ends_with(b"_LINKER")
        })
        .collect::<Vec<_>>();
    target_linkers.sort_by(|left, right| left.0.cmp(&right.0));
    for (key, value) in target_linkers {
        hash.update(key.as_encoded_bytes());
        hash.update([0]);
        hash_tool(&mut hash, &value)?;
        hash.update([0xff]);
    }
    let mut profile = env::vars_os()
        .filter(|(key, _)| {
            key.as_encoded_bytes()
                .starts_with(b"CARGO_PROFILE_RELEASE_")
        })
        .collect::<Vec<_>>();
    profile.sort_by(|left, right| left.0.cmp(&right.0));
    for (key, value) in profile {
        hash.update(key.as_encoded_bytes());
        hash.update([0]);
        hash.update(value.as_encoded_bytes());
        hash.update([0xff]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Read and apply user-recorded unified patches in stable filename order.
pub(crate) fn apply_recorded(source: &Path, home: &Path) -> Result<Vec<(String, String)>> {
    let parent = home.join(".sifttext");
    match fs::symlink_metadata(&parent) {
        Ok(metadata) => require(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "unsafe local patch parent",
        )?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    }
    let dir = home.join(PATCH_DIR);
    let metadata = match fs::symlink_metadata(&dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    require(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "unsafe patch directory",
    )?;
    let mut patches = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "patch")
        {
            let meta = fs::symlink_metadata(&path)?;
            require(
                meta.is_file() && !meta.file_type().is_symlink(),
                "unsafe patch file",
            )?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "non-UTF-8 patch name")?;
            patches.push((name, path));
        }
    }
    patches.sort_by(|left, right| left.0.cmp(&right.0));
    let mut receipts = Vec::with_capacity(patches.len());
    for (name, path) in patches {
        let bytes = fs::read(&path)?;
        require(!bytes.is_empty(), "empty recorded patch")?;
        let check = Command::new("git")
            .arg("-C")
            .arg(source)
            .args(["apply", "--check"])
            .arg(&path)
            .output()?;
        if check.status.success() {
            let apply = Command::new("git")
                .arg("-C")
                .arg(source)
                .arg("apply")
                .arg(&path)
                .output()?;
            require(
                apply.status.success(),
                &format!(
                    "recorded patch {name} could not be applied: {}",
                    String::from_utf8_lossy(&apply.stderr)
                ),
            )?;
        } else {
            let reversed = Command::new("git")
                .arg("-C")
                .arg(source)
                .args(["apply", "--reverse", "--check"])
                .arg(&path)
                .output()?;
            require(
                reversed.status.success(),
                &format!(
                    "recorded patch {name} conflicts with this release: {}",
                    String::from_utf8_lossy(&check.stderr)
                ),
            )?;
        }
        receipts.push((name, sha256(&bytes)));
    }
    Ok(receipts)
}

pub(crate) fn write_receipt(
    path: &Path,
    version: &str,
    source: &str,
    format: u32,
    inputs: &str,
) -> Result<BuildReceipt> {
    let executable = sha256(&fs::read(path.join("sifttext"))?);
    Ok(BuildReceipt {
        version: version.to_owned(),
        source: source.to_owned(),
        format,
        inputs: inputs.to_owned(),
        executable,
    })
}

pub(crate) fn verify_receipt(
    path: &Path,
    receipt: &BuildReceipt,
    version: &str,
    source: &str,
    format: u32,
    inputs: &str,
) -> Result<()> {
    require(
        receipt.version == version
            && receipt.source == source
            && receipt.format == format
            && receipt.inputs == inputs
            && receipt.executable == sha256(&fs::read(path.join("sifttext"))?),
        "cached patched build identity changed; preserve it and rebuild",
    )
}
