//! Shared node-selection precedence for rendering, publication and hosted review.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Include,
    Exclude,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub node: String,
    pub action: Action,
    pub recurse: bool,
}

/// An explicit node rule wins, then its nearest recursive ancestor, then default.
/// Ancestors must be supplied nearest first. This does not alter parentage.
pub fn includes<'a>(
    rules: &[Rule],
    default: Action,
    node: &str,
    ancestors: impl IntoIterator<Item = &'a str>,
) -> bool {
    rules.iter().find(|rule| rule.node == node)
        .or_else(|| ancestors.into_iter().find_map(|parent| {
            rules.iter().find(|rule| rule.node == parent && rule.recurse)
        }))
        .map_or(default, |rule| rule.action) == Action::Include
}

/// Replace structured references without changing the remaining authored text.
pub fn references(text: &str, refs: &[crate::Reference]) -> crate::Result<String> {
    if crate::parse_markdown(text)?.see_also == refs {
        return Ok(text.into());
    }
    let mut result = String::new();
    let mut fences = 0;
    let mut replaced = false;
    for line in text.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            fences += 1;
        }
        if fences == 1
            && line
                .split_once(':')
                .is_some_and(|(key, _)| key.trim() == "see_also")
        {
            result.push_str(&format!("see_also: {}\n", serde_json::to_string(refs)?));
            replaced = true;
        } else {
            result.push_str(line);
        }
    }
    if !replaced {
        return Err("missing structured references".into());
    }
    Ok(result)
}
