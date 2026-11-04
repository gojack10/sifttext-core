//! Portable, mechanically verified format-migration allowances.
//! The evidence is node-local so selected sync need not disclose private ancestry.
use crate::{Result, require};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

const FIELD: &str = "migration_size";
// A legal source is at most 1 MiB. Link conversion is smaller than twice its
// source, base64 evidence adds at most 4/3, and position/record framing is tiny.
// This is an input bound, never an authored publication budget.
pub const INPUT_LIMIT: usize = 4 * crate::LIMIT;

#[derive(Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Evidence {
    version: u8,
    body: String,
    position: Option<String>,
    links: bool,
    status: String,
}

fn evidence(text: &str) -> Result<Option<Evidence>> {
    crate::markdown(text)?.fields.get(FIELD)
        .map(|raw| serde_json::from_str(raw).map_err(Into::into)).transpose()
}

pub fn has_record(text: &str) -> Result<bool> {
    Ok(evidence(text)?.is_some())
}

pub(crate) fn without_record(text: &str) -> String {
    let mut fence = 0;
    text.split_inclusive('\n').filter(|line| {
        if line.trim_end_matches(['\r', '\n']) == "---" { fence += 1; }
        !(fence == 1 && line.split_once(':').is_some_and(|(key, _)| key.trim() == FIELD))
    }).collect()
}

fn with_body(text: &str, body: &str) -> Result<String> {
    let parsed = crate::parse_markdown(text)?;
    Ok(format!("{}{body}", &text[..text.len() - parsed.content.len()]))
}

fn position_line(text: &str) -> Option<(usize, &str)> {
    let mut offset = 0;
    let mut fence = 0;
    for line in text.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" { fence += 1; }
        if fence == 2 { break }
        if fence == 1 && line.split_once(':').is_some_and(|(key, _)| key.trim() == "position") {
            return Some((offset, line));
        }
        offset += line.len();
    }
    None
}

/// Verify byte derivation independently of token counting. Readers can retain a
/// draft needing repair, but acceptance always invokes `permitted` as well.
fn source(text: &str, record: &Evidence) -> Result<String> {
    require(record.version == 1 && record.body.len() <= crate::LIMIT.div_ceil(3) * 4,
        "invalid migration size evidence")?;
    let body = String::from_utf8(STANDARD.decode(&record.body)?)?;
    let current = without_record(text);
    serde_json::from_str::<crate::status::Status>(&record.status)?;
    let current_status = crate::markdown(&current)?.fields["status"].to_owned();
    let mut original = with_body(&current, &body)?;
    original = crate::status::patch(&original, "status", Some(&record.status))?;
    let (offset, line) = position_line(&original).ok_or("missing migrated position")?;
    let end = offset + line.len();
    if let Some(prior) = &record.position {
        let line = prior.trim_end_matches(['\r', '\n']);
        require(!line.contains(['\r', '\n']) && prior.ends_with('\n')
            && line.split_once(':').is_some_and(|(key, _)| key.trim() == "position"),
            "invalid migration position evidence")?;
    }
    original.replace_range(offset..end, record.position.as_deref().unwrap_or(""));
    crate::ordering::position(&original)?;
    require(original.len() <= crate::LIMIT, "oversized migration source")?;
    let mut converted = if record.links { crate::home_format::markdown_links(&original)? } else { original.clone() };
    if position_line(&converted).map(|(_, line)| line) != position_line(&current).map(|(_, line)| line) {
        let position = crate::ordering::position(&current)?.ok_or("missing migrated position")?;
        converted = crate::ordering::with_position(&converted, position)?;
    }
    require(converted != original, "migration did not change node")?;
    converted = crate::status::patch(&converted, "status", Some(&current_status))?;
    require(converted == current, "migration evidence does not derive node")?;
    Ok(original)
}

pub(crate) fn permitted(text: &str) -> Result<bool> {
    let Some(record) = evidence(text)? else { return Ok(false) };
    require(!crate::native_kb::documentation(&crate::parse_markdown(text)?.id),
        "bundled documentation must fit ordinary size limits")?;
    let original = source(text, &record)?;
    require(crate::token_count::ordinary(&crate::parse_markdown(&original)?.content)
        <= crate::candidate_validation::CEILING, "oversized migration source body")?;
    Ok(true)
}

/// Mint only for excess produced by the importer from an admissible source.
pub(crate) fn attach(original: &str, converted: &str, links: bool) -> Result<String> {
    if converted.len() <= crate::LIMIT
        && (!links || crate::token_count::ordinary(&crate::parse_markdown(converted)?.content)
            <= crate::candidate_validation::CEILING) { return Ok(converted.to_owned()) }
    require(evidence(original)?.is_none(), "migration allowance already present")?;
    let record = Evidence { version: 1,
        body: STANDARD.encode(crate::parse_markdown(original)?.content.as_bytes()),
        position: position_line(original).map(|(_, line)| line.to_owned()),
        links, status: crate::markdown(original)?.fields["status"].to_owned() };
    let original_check = source(converted, &record)?;
    require(original.len() <= crate::LIMIT && original_check == original,
        "migration source changed")?;
    require(crate::token_count::ordinary(&crate::parse_markdown(original)?.content)
        <= crate::candidate_validation::CEILING, "oversized migration source body")?;
    let parsed = crate::parse_markdown(converted)?;
    let boundary = converted.len() - parsed.content.len();
    let prefix = &converted[..boundary];
    let fence = prefix.rfind("---").ok_or("missing metadata fence")?;
    let text = format!("{}{}: {}\n{}", &converted[..fence], FIELD,
        serde_json::to_string(&record)?, &converted[fence..]);
    require(text.len() <= INPUT_LIMIT, "migration evidence exceeds input bound")?;
    Ok(text)
}

pub(crate) fn convert_links(text: &str) -> Result<String> {
    let clean = without_record(text);
    let converted = crate::home_format::markdown_links(&clean)?;
    let original = match evidence(text)? {
        Some(record) => source(text, &record)?,
        None => clean,
    };
    attach(&original, &converted, true)
}

pub fn input_limit(path: &Path) -> usize {
    if path.file_name().is_some_and(|name| name == "node.md") { INPUT_LIMIT } else { crate::LIMIT }
}

fn repair(home: &Path, draft: &Path, id: &str, text: &str) -> Result<()> {
    // Bookkeeping is retired by acceptance, so it must not manufacture byte debt.
    let authored = without_record(text);
    let tokens = crate::token_count::ordinary(&crate::parse_markdown(&authored)?.content);
    let (measure, count, limit, cause) = if tokens > crate::candidate_validation::CEILING {
        ("tokens", tokens, crate::candidate_validation::CEILING,
            "This node exceeded the token limit after link conversion.")
    } else if authored.len() > crate::LIMIT {
        ("bytes", text.len(), crate::LIMIT, "This node exceeded the file byte limit during migration.")
    } else { return Ok(()) };
    Err(format!("node:{}\n{measure}:{count}\nlimit:{limit}\nsifttext-commit:{}\nagent-instructions:{cause} Cut if easy, but prefer decomposition following SiftText Commit. Resubmit within the stated limit. Draft preserved.",
        draft.join(format!("nodes/{id}/node.md")).display(),
        home.join("nodes/c72d552d-3499-445b-a8d5-05d0ff7824f2/node.md").display()).into())
}

pub(crate) fn submitted_nodes(home: &Path, base: &str, submitted: &str) -> Result<BTreeSet<String>> {
    let paths = crate::local::reader_git(home,
        &["diff", "--no-renames", "--name-only", base, submitted, "--", "nodes", "trees"])?;
    Ok(paths.lines().filter_map(|path| {
        let mut parts = path.split('/');
        match (parts.next(), parts.next()) {
            (Some("nodes" | "trees"), Some(id)) if crate::uuid(id) => Some(id.to_owned()),
            _ => None,
        }
    }).collect())
}

/// Inspect the immutable original contribution before merge/status preparation.
/// Returns only nodes whose evidence must be retired in the acceptance commit.
pub(crate) fn submission(
    reader: &mut crate::git_snapshot::GitReader, home: &Path, draft: &Path,
    base: &str, submitted: &str, accepted: &str, received: bool,
) -> Result<BTreeSet<String>> {
    let before = reader.view(base)?;
    let after = reader.view(submitted)?;
    let current = reader.view(accepted)?;
    let migration = reader.imported_baseline.is_some();
    let authored = match &reader.submitted_nodes {
        Some(ids) => ids.clone(),
        None => submitted_nodes(home, base, submitted)?,
    };
    let mut retire = BTreeSet::new();
    for (id, node) in &after.nodes {
        if !authored.contains(id) { continue }
        if !node.migration_size && current.nodes.get(id).is_none_or(|old| !old.migration_size)
            && before.nodes.get(id).is_none_or(|old| !old.migration_size) { continue }
        let bytes = reader.bytes(&after, id)?;
        let text = std::str::from_utf8(&bytes)?;
        let new = evidence(text)?;
        let prior = if current.nodes.contains_key(id) {
            Some(reader.bytes(&current, id)?)
        } else {
            // Accepted first-parent history retains retirement across deletion
            // and selected exclusion. Never consult an untrusted candidate's past.
            let path = format!("nodes/{id}/node.md");
            let revision = crate::local::reader_git(home,
                &["log", "--first-parent", "-1", "--diff-filter=AM", "--format=%H", accepted, "--", &path])?;
            if revision.is_empty() { None } else {
                let view = reader.view(&revision)?;
                Some(reader.bytes(&view, id)?)
            }
        };
        let old = prior.as_ref().map(|bytes| evidence(std::str::from_utf8(bytes)?)).transpose()?.flatten();
        if received || migration {
            if let Some(record) = &new {
                permitted(text)?;
                if migration && old.as_ref() != Some(record) {
                    let original = source(text, record)?;
                    let previous = reader.bytes(&before, id)?;
                    require(crate::parse_markdown(std::str::from_utf8(&previous)?)?.content
                        == crate::parse_markdown(&original)?.content, "migration source body changed")?;
                } else if !migration {
                    require(prior.is_none() || old.as_ref() == Some(record),
                        "retired migration allowance cannot be renewed")?;
                }
            }
        } else {
            let base_record = if before.nodes.get(id).is_some_and(|old| old.migration_size) {
                let bytes = reader.bytes(&before, id)?;
                evidence(std::str::from_utf8(&bytes)?)?
            } else { None };
            require(new.is_none() || old.as_ref() == new.as_ref() || base_record.as_ref() == new.as_ref(),
                "authored submission cannot create or replace a migration allowance")?;
            if old.is_some() || base_record.is_some() || new.is_some() {
                repair(home, draft, id, text)?;
                retire.insert(id.clone());
            }
        }
    }
    Ok(retire)
}
