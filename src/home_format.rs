//! One-time import of old homes. Normal readers and writers use positions only.
//! The temporary native draft and submission journal retain interrupted work.
use crate::{Policy, Result, local, ordering, require};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, fs, path::{Path, PathBuf}};

#[derive(Serialize, Deserialize)]
struct Pending {
    draft: PathBuf,
    base: String,
    baseline: Option<String>,
    commit: Option<String>,
    explicit: BTreeSet<String>,
}

fn persist(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().ok_or("missing migration state directory")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

fn config(home: &Path) -> Result<Value> {
    let path = home.join(".sifttext/config.json");
    let value = match fs::symlink_metadata(&path) {
        Ok(_) => serde_json::from_slice(&crate::read_regular(&path)?)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(e) => return Err(e.into()),
    };
    require(value.is_object(), "home configuration must be an object")?;
    for key in ["order_format_version", "link_format_version"] {
        require(value.get(key).is_none_or(Value::is_u64), "format version must be an integer")?;
    }
    Ok(value)
}

fn complete(value: &Value) -> bool {
    value["order_format_version"].as_u64().unwrap_or(0) >= 1
        && value["link_format_version"].as_u64().unwrap_or(0) >= 1
}

/// Parse only importer inputs, without calling a normal home/index reader first.
/// Existing position order is preserved; a legacy list supplies the old sequence.
fn import_positions(draft: &Path) -> Result<BTreeSet<String>> {
    let mut texts = BTreeMap::new();
    for entry in fs::read_dir(draft.join("nodes"))? {
        let entry = entry?;
        if entry.file_name() == ".keep" { continue }
        let id = entry.file_name().into_string().map_err(|_| "non-UTF8 node identity")?;
        require(crate::uuid(&id), "invalid migration node identity")?;
        crate::real_dir(&entry.path())?;
        let text = String::from_utf8(crate::read_regular(&entry.path().join("node.md"))?)?;
        require(crate::parse_markdown(&text)?.id == id, "migration node identity mismatch")?;
        texts.insert(id, text);
    }
    let mut ranks = BTreeMap::new();
    let mut lists = Vec::new();
    for id in texts.keys() {
        let dir = draft.join("nodes").join(id);
        let list = dir.join("children.order");
        if !list.try_exists()? { continue }
        let bytes = crate::read_regular(&list)?;
        let order: Vec<_> = std::str::from_utf8(&bytes)?.lines().collect();
        let mut members = BTreeSet::new();
        crate::real_dir(&dir.join("children"))?;
        for entry in fs::read_dir(dir.join("children"))? {
            let entry = entry?;
            if entry.file_name() == ".keep" { continue }
            let child = entry.file_name().into_string().map_err(|_| "non-UTF8 child")?;
            require(crate::uuid(&child) && texts.contains_key(&child), "invalid migration child")?;
            require(fs::read_link(entry.path())? == Path::new(&format!("../../{child}")), "unsafe migration child link")?;
            members.insert(child);
        }
        require(order.len() == members.len() && order.iter().copied().collect::<BTreeSet<_>>() == members.iter().map(String::as_str).collect(),
            "legacy child list does not match relationships; retain the migration draft")?;
        for (i, child) in order.iter().enumerate() {
            require(ranks.insert((*child).to_owned(), (i as u64 + 1) * 1024).is_none(), "multiple migration parents")?;
        }
        lists.push(list);
    }
    for (id, text) in &texts {
        let old = ordering::position(text)?;
        let rank = ranks.get(id).copied().or(old).unwrap_or(0);
        if old != Some(rank) {
            fs::write(draft.join("nodes").join(id).join("node.md"), ordering::with_position(text, rank)?)?;
        }
    }
    // Remove lists only after every node has a position. No relationship is rewritten.
    for list in lists { fs::remove_file(list)?; }
    Ok(texts.into_keys().collect())
}

/// Rewrite body references only. Metadata (including see_also) remains byte-identical.
/// The UUID is the target; relative Markdown paths start at the containing node.md.
pub fn markdown_links(text: &str) -> Result<String> {
    crate::parse_markdown(text)?;
    let mut fences = 0;
    let mut boundary = 0;
    for line in text.split_inclusive('\n') {
        boundary += line.len();
        if line.trim_end_matches(['\r', '\n']) == "---" {
            fences += 1;
            if fences == 2 { break }
        }
    }
    let pattern = regex::Regex::new(r"\[\[([^\]\n|]+)\|([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\]\]")?;
    let body = pattern.replace_all(&text[boundary..], |capture: &regex::Captures<'_>| {
        let label = capture[1].replace('\\', "\\\\").replace('[', "\\[").replace(']', "\\]");
        format!("[{label}](../{}/node.md)", &capture[2])
    });
    Ok(format!("{}{}", &text[..boundary], body))
}

fn commit(draft: &Path, message: &str) -> Result<String> {
    local::reader_git(draft, &["add", "--all", "--", "nodes", "trees"])?;
    if !local::reader_git(draft, &["diff", "--cached", "--name-only"])?.is_empty() {
        local::reader_git(draft, &["commit", "-m", message])?;
    }
    local::reader_git(draft, &["rev-parse", "HEAD"])
}

/// Called on update preparation and first home use. Linked drafts are not silently
/// rewritten: their owners retain their edits and can reconcile against migrated main.
pub fn ensure(policy: &Policy, home: &Path) -> Result<()> {
    if !home.join(".git").is_dir() || !home.join("nodes").is_dir() { return Ok(()) }
    // Source checkouts are not managed knowledge homes.
    if local::reader_git(home, &["symbolic-ref", "--quiet", "HEAD"]).is_ok() { return Ok(()) }
    if complete(&config(home)?) { return Ok(()) }
    let home = fs::canonicalize(home)?;
    let state_dir = home.join(".sifttext");
    fs::create_dir_all(&state_dir)?;
    let lock = fs::OpenOptions::new().read(true).write(true).create(true).truncate(false)
        .open(state_dir.join("format-migration.lock"))?;
    lock.lock()?;
    if complete(&config(&home)?) { return Ok(()) }
    let journal = state_dir.join("format-migration.json");
    local::configure_git_identity(&home)?;
    let mut pending: Pending = if journal.try_exists()? {
        serde_json::from_slice(&crate::read_regular(&journal)?)?
    } else {
        let allocated = local::migration_worktree(policy, &home)?;
        let pending = Pending {
            draft: PathBuf::from(allocated["worktree"].as_str().ok_or("missing migration draft")?),
            base: allocated["base"].as_str().ok_or("missing migration base")?.into(),
            baseline: None, commit: None, explicit: BTreeSet::new(),
        };
        persist(&journal, &pending)?;
        pending
    };
    eprintln!("Preparing home format: {} (draft {})", home.display(), pending.draft.display());
    if pending.baseline.is_none() {
        if local::sparse_checkout(&pending.draft)? {
            local::reader_git(&pending.draft, &["sparse-checkout", "disable"])?;
        }
        pending.explicit = import_positions(&pending.draft)?;
        pending.baseline = Some(commit(&pending.draft, "home-format: replace shared child lists with positions")?);
        persist(&journal, &pending)?;
    }
    if pending.commit.is_none() {
        for id in &pending.explicit {
            let path = pending.draft.join("nodes").join(id).join("node.md");
            let text = String::from_utf8(crate::read_regular(&path)?)?;
            let converted = markdown_links(&text)?;
            if text != converted { fs::write(&path, converted)?; }
        }
        pending.commit = Some(commit(&pending.draft, "home-format: use relative Markdown body references")?);
        persist(&journal, &pending)?;
    }
    if pending.draft.join(".git").is_file() {
        let original = pending.commit.as_deref().ok_or("missing migration candidate")?;
        // Finish accepted requests unchanged. For a rejected candidate, use the
        // same committed-descendant repair route as the managed updater.
        if local::reader_git(&home, &["merge-base", "--is-ancestor", original, "refs/heads/main"]).is_err() {
            let head = local::reader_git(&pending.draft, &["rev-parse", "HEAD"])?;
            if head != original {
                local::reader_git(&home, &["merge-base", "--is-ancestor", original, &head])?;
                crate::checked_publication::clean(&pending.draft, &head)?;
                pending.commit = Some(head);
                persist(&journal, &pending)?;
            }
        }
        crate::index::prepare(policy, &pending.draft, None)?;
    }
    let request = crate::checked_publication::Request {
        repo: home.clone(), accepted_ref: "refs/heads/main".into(),
        draft: Some(pending.draft.clone()), commit: pending.commit.clone(), base: Some(pending.base.clone()),
        explicit: pending.explicit.clone(), pause_before_publish: None, pause_after_publish: None,
    };
    let baseline = pending.baseline.as_deref().ok_or("missing imported baseline")?;
    let mut receipt = local::submit_migration(policy, &request, baseline)?;
    // An old derived cache can refuse finishing after acceptance. Preparation is
    // authorized here, never hidden in ordinary reads after migration completes.
    if receipt["status"] != "current"
        && local::reader_git(&home, &["merge-base", "--is-ancestor", pending.commit.as_deref().unwrap(), "HEAD"]).is_ok()
    {
        crate::index::prepare(policy, &home, None)?;
        receipt = local::submit_migration(policy, &request, baseline)?;
    }
    eprintln!("home-format-publication:{}", crate::checked_publication::cli_json(&receipt)?);
    require(receipt["status"] == "current" && receipt["worktree_removed"] == true,
        &format!("home format publication remains unfinished; preserve {} and retry the command", journal.display()))?;
    // Reload instead of replacing unrelated consent, sharing or update settings.
    let mut settings = config(&home)?;
    settings["order_format_version"] = json!(1);
    settings["link_format_version"] = json!(1);
    persist(&state_dir.join("config.json"), &settings)?;
    fs::remove_file(&journal)?;
    fs::File::open(&state_dir)?.sync_all()?;
    Ok(())
}
