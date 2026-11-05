//! Missing user-level skill installation during authorized home/update preparation.
use crate::Result;
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

const NAME: &str = "sifttext-commit";
const COMMIT: &str = "c72d552d-3499-445b-a8d5-05d0ff7824f2";
const TEMPLATE: &str = include_str!("../integrations/skills/sifttext-commit/SKILL.md");

/// Only configured harnesses receive a skill. An occupied skill directory belongs
/// to the user, including a pointer to another knowledge home.
pub(crate) fn install_missing(home: &Path) {
    let home = match fs::canonicalize(home) {
        Ok(home) => home,
        Err(error) => {
            eprintln!("SiftText Commit skill could not resolve its knowledge home: {error}");
            return;
        }
    };
    let node = home.join("nodes").join(COMMIT).join("node.md");
    if !node.is_file() {
        return;
    }
    let text = TEMPLATE.replace("{{COMMIT_NODE}}", &node.to_string_lossy());
    let user = env::var_os("HOME").map(PathBuf::from);
    for (harness, variable, default) in [
        ("Codex", "CODEX_HOME", ".codex"),
        ("Claude Code", "CLAUDE_CONFIG_DIR", ".claude"),
        ("Pi", "PI_CODING_AGENT_DIR", ".pi/agent"),
    ] {
        let configured = env::var_os(variable)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let Some(mut directory) =
            configured.or_else(|| user.as_ref().map(|user| user.join(default)))
        else {
            continue;
        };
        if let (Some(user), Ok(relative)) = (&user, directory.strip_prefix("~")) {
            directory = user.join(relative);
        }
        if !directory.is_dir() {
            continue;
        }
        if let Err(error) = install(&directory.join("skills"), &text) {
            eprintln!("SiftText Commit skill for {harness} was not installed: {error}");
        }
    }
}

fn install(skills: &Path, text: &str) -> Result<()> {
    fs::create_dir_all(skills)?;
    let directory = skills.join(NAME);
    match fs::create_dir(&directory) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    let path = directory.join("SKILL.md");
    let result = (|| -> Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(&directory)?;
        file.write_all(text.as_bytes())?;
        file.persist_noclobber(&path).map_err(|error| error.error)?;
        Ok(())
    })();
    if result.is_err() {
        // Remove only our empty directory after its temporary file has retired.
        let _ = fs::remove_dir(&directory);
    } else {
        eprintln!("SiftText Commit skill installed: {}", path.display());
    }
    result
}
