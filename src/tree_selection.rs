//! Shared node-selection precedence for rendering and publication.
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
