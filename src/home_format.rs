//! One-time import of old homes. Normal readers and writers use positions only.
//! The temporary native draft and submission journal retain interrupted work.
use crate::{Policy, Result, local, require};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::{Path, PathBuf}};

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
        pending.explicit = crate::order_migration::convert(&home, &pending.draft, &pending.base)?;
        let baseline = commit(&pending.draft, "home-format: replace shared child lists with positions")?;
        crate::order_migration::check(policy, &home, &pending.base, &baseline)?;
        pending.baseline = Some(baseline);
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
