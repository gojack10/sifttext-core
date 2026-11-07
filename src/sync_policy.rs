//! Shared upload selection and consent settings for sync and hosted review.
use crate::{Result, uuid, tree_selection::{Action, Rule, includes}};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub mode: String,
    default: Action,
    rules: Vec<Rule>,
}
impl Settings {
    pub fn includes<'a>(&self, node: &str, ancestors: impl IntoIterator<Item = &'a str>) -> bool {
        includes(&self.rules, self.default, node, ancestors)
    }
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: "unanswered".into(),
            default: Action::Include,
            rules: Vec::new(),
        }
    }
}
pub fn config(home: &Path) -> Result<Value> {
    if let Ok(meta) = fs::symlink_metadata(home.join(".sifttext")) {
        if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o022 != 0 {
            return Err("unsafe local configuration directory".into());
        }
    }
    let path = home.join(".sifttext/config.json");
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.into()),
        Ok(meta) => {
            if !meta.is_file() || meta.nlink() != 1 || meta.len() > 1024 * 1024 {
                return Err("unsafe local configuration".into());
            }
            let mut file = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(path)?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            let value: Value = serde_json::from_slice(&bytes)?;
            if !value.is_object() {
                return Err("local configuration must be an object".into());
            }
            Ok(value)
        }
    }
}
pub fn settings(home: &Path) -> Result<Settings> {
    let file = config(home)?;
    let settings: Settings = file
        .get("sync")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_default();
    validate(&settings)?;
    Ok(settings)
}
pub fn validate(settings: &Settings) -> Result<()> {
    if !["unanswered", "declined", "disabled", "enabled"].contains(&settings.mode.as_str()) {
        return Err("sync.mode must be unanswered, declined, disabled or enabled".into());
    }
    let mut rules: BTreeMap<&str, Action> = BTreeMap::new();
    for rule in &settings.rules {
        if !uuid(&rule.node) {
            return Err("sync rule requires a canonical node UUID".into());
        }
        if rules
            .insert(&rule.node, rule.action)
            .is_some_and(|old| old != rule.action)
        {
            return Err("opposing sync rules for one node; no transfer permitted".into());
        }
    }
    Ok(())
}
