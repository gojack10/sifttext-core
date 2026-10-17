use serde_json::json;
use sifttext_file_graph::{Policy, Result, local};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

const HELP: &str = "SiftText — local Markdown knowledge, Git history, derived retrieval

sifttext runtime-check                     Verify executable assets without reading a knowledge store
sifttext update                            Check and apply compatible official product updates
sifttext update --status                    Show installed identities and pending update state
sifttext update --resume                    Submit an agent-resolved KB merge and activate its release
sifttext update --rollback                  Restore the previous release through checked KB reversal
sifttext update --config                    Read the user's update mode and standing grant
sifttext update --grant standing|ask        Record or revoke the user's routine-update grant
sifttext update --mode notify|disabled      Choose whether ordinary commands check releases
sifttext init HOME                         Copy the embedded first-party KB (from native files) to a NEW Git home
sifttext init HOME --bundle BUNDLE         Copy a supplied native KB to a NEW independent Git home
sifttext init HOME --empty                 Explicitly initialize an empty knowledge home
                                          Existing recognized home/edits always unchanged
sifttext instructions STORE                Print file-first entry and ordered child paths
sifttext read STORE UUID                   Check current source, then print exact node.md
sifttext submit REPO DRAFT COMMIT BASE [EXPLICIT_UUID ...]
                                          Check/save in draft, refresh HOME, remove completed draft
sifttext accepted REPO                     Recover a current detached accepted view
sifttext advance HOME                      Explicit ff-only clean home reader + derived refresh
sifttext worktree HOME                     Allocate a private branch/worktree; prints its path
sifttext create WORKTREE NAME SUMMARY [PARENT_UUID]
                                          Create a tree root or child; never commits
sifttext session-capture SOURCE DESCRIPTOR.json ARCHIVE_ROOT
                                          Copy and Git-checkpoint a validated Pi session
sifttext session-attach HOME NODE DESCRIPTOR.json TOPIC ARCHIVE_ROOT
                                          Attach a retained session and create a local chat link
sifttext session-restore HOME ARCHIVE_ROOT Rebuild local links after archive relocation
sifttext session-retention-config HOME  Read local Pi session-retention opt-in (default off)
sifttext check STORE [BEFORE [EXPLICIT_UUID ...]]
                                          Complete candidate check; does not modify files
sifttext normalize BEFORE WORKTREE [EXPLICIT_UUID ...]
                                          Apply native status activation/reopening only
sifttext index STORE                       Refresh derived SQLite and LEAN outlines
sifttext outline STORE UUID [full]         Refresh, verify, then print shallow/full LEAN
sifttext search STORE QUERY [UNDER_UUID]    Ranked lexical search, at most 20 hits
sifttext sql STORE 'SELECT ...'            Read-only same-version SQLite query
sifttext -h                               This reference

Read node.md with ordinary file tools. Edit only your private worktree.
Git publication is explicit: inspect, validate the exact candidate, commit,
then submit to combine accepted main, refresh the home and remove the saved draft.
After interruption, preserve the original submit arguments and retry that request.
No fixed node-count ceiling; checked resource bounds, 1 MiB authored files,
5000-token crystallization ceiling. Default KB is separate from editable homes.
No hosted account, MCP, model provider, or Dark Factory required.";

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args == ["-h"] || args == ["--help"] {
        println!("{HELP}");
        return Ok(());
    }
    if args
        .as_slice()
        .first()
        .is_some_and(|arg| arg == "session-retention-config")
    {
        let [_, home] = args.as_slice() else {
            return Err("usage: sifttext session-retention-config HOME".into());
        };
        println!(
            "{}",
            serde_json::to_string(&local::session_retention_config(Path::new(home))?)?
        );
        return Ok(());
    }
    if args.first().is_some_and(|arg| arg == "update") {
        let mode = match args.as_slice() {
            [_] => String::new(),
            [_, mode]
                if ["--status", "--resume", "--rollback", "--config"].contains(&mode.as_str()) =>
            {
                mode.clone()
            }
            [_, flag, value]
                if (flag == "--grant" && ["standing", "ask"].contains(&value.as_str()))
                    || (flag == "--mode" && ["notify", "disabled"].contains(&value.as_str())) =>
            {
                format!("{flag}={value}")
            }
            _ => return Err("unknown update arguments".into()),
        };
        let root = std::env::var_os("SIFTTEXT_INSTALL_ROOT")
            .ok_or("use the managed installation launcher for updates")?;
        println!(
            "{}",
            serde_json::to_string_pretty(&sifttext_file_graph::updates::update(
                Path::new(&root),
                &mode
            )?)?
        );
        return Ok(());
    }
    let runtime = local::runtime()?;
    let policy = Policy::checked(&runtime)?;
    // Bundle/bootstrap assets belong to this packaged onramp, not the generic
    // native Policy contract (which also supports minimal policy-only fixtures).
    sifttext_file_graph::native_kb::verify_assets(&runtime)?;
    sifttext_file_graph::graph_transition::Transition::checked(&policy)?;
    let output = match args.as_slice() {
        [cmd] if cmd == "runtime-check" => json!({"status":"checked", "runtime":runtime}),
        [cmd, root, home] if cmd == "install-home" => {
            sifttext_file_graph::updates::initialize(&policy, &absolute(root)?, &absolute(home)?)?
        }
        [cmd, home] if cmd == "init" => {
            sifttext_file_graph::native_kb::init(&policy, &absolute(home)?)?
        }
        [cmd, home, flag] if cmd == "init" && flag == "--empty" => {
            local::init(&policy, &absolute(home)?, None)?
        }
        [cmd, home, flag, source] if cmd == "init" && flag == "--bundle" => {
            local::init(&policy, &absolute(home)?, Some(Path::new(source)))?
        }
        [cmd, path] if cmd == "instructions" => {
            let instructions = local::instructions(&policy, Path::new(path))?;
            let output = json!({"version":instructions["version"],"rows":instructions["files"]});
            println!(
                "{}",
                sifttext_file_graph::lean_render::render(&output).map_err(std::io::Error::other)?
            );
            return Ok(());
        }
        [cmd, path, id] if cmd == "read" => {
            print!("{}", local::read(&policy, Path::new(path), id)?);
            return Ok(());
        }
        [cmd, repo, draft, commit, base, explicit @ ..] if cmd == "submit" => {
            let receipt = local::submit(
                &policy,
                &sifttext_file_graph::checked_publication::Request {
                    repo: repo.into(),
                    accepted_ref: "refs/heads/main".into(),
                    draft: Some(draft.into()),
                    commit: Some(commit.clone()),
                    base: Some(base.clone()),
                    explicit: explicit.iter().cloned().collect(),
                    pause_before_publish: None,
                    pause_after_publish: None,
                },
            )?;
            println!("{}", serde_json::to_string_pretty(&receipt)?);
            if receipt["status"] != "current" {
                return Err("submission unavailable; retain draft and inspect receipt".into());
            }
            return Ok(());
        }
        [cmd, repo] if cmd == "accepted" => {
            let receipt = sifttext_file_graph::checked_publication::submit(
                &policy,
                &sifttext_file_graph::checked_publication::Request {
                    repo: repo.into(),
                    accepted_ref: "refs/heads/main".into(),
                    draft: None,
                    commit: None,
                    base: None,
                    explicit: BTreeSet::new(),
                    pause_before_publish: None,
                    pause_after_publish: None,
                },
            );
            println!("{}", serde_json::to_string_pretty(&receipt)?);
            if receipt.status != "current" {
                return Err("accepted view unavailable; retain existing files".into());
            }
            return Ok(());
        }
        [cmd, home] if cmd == "advance" => local::advance(&policy, Path::new(home))?,
        [cmd, home] if cmd == "worktree" => local::worktree(&policy, Path::new(home))?,
        [cmd, source, descriptor, archive] if cmd == "session-capture" => {
            let descriptor_path = absolute(descriptor)?;
            let metadata = std::fs::symlink_metadata(&descriptor_path)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err("descriptor must be a regular file".into());
            }
            let value = serde_json::from_slice(&std::fs::read(descriptor_path)?)?;
            local::capture_session(Path::new(source), &value, Path::new(archive))?
        }
        [cmd, home, node, descriptor, topic, archive] if cmd == "session-attach" => {
            let descriptor_path = absolute(descriptor)?;
            let metadata = std::fs::symlink_metadata(&descriptor_path)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err("descriptor must be a regular file".into());
            }
            let value = serde_json::from_slice(&std::fs::read(descriptor_path)?)?;
            local::attach_session(Path::new(home), node, &value, topic, Path::new(archive))?
        }
        [cmd, home, archive] if cmd == "session-restore" => {
            local::restore_session_links(Path::new(home), Path::new(archive))?
        }
        [cmd, path, name, summary, rest @ ..] if cmd == "create" && rest.len() <= 1 => {
            local::create(
                &policy,
                Path::new(path),
                name,
                summary,
                rest.first().map(String::as_str),
            )?
        }
        [cmd, path, rest @ ..] if cmd == "check" => {
            let report = local::check(
                &policy,
                Path::new(path),
                rest.first().map(Path::new),
                rest.iter().skip(1).cloned().collect(),
            )?;
            println!("{}", report.lean()?);
            return Ok(());
        }
        [cmd, before, after, explicit @ ..] if cmd == "normalize" => {
            serde_json::to_value(sifttext_file_graph::status::normalize(
                &policy,
                Path::new(before),
                Path::new(after),
                &explicit.iter().cloned().collect(),
                None,
            )?)?
        }
        [cmd, path] if cmd == "index" => {
            let v = local::refresh(&policy, Path::new(path))?;
            let output = json!({"status":v.status,"version":v.version,"nodes":v.nodes.len(),"links":v.links.len()});
            println!(
                "{}",
                sifttext_file_graph::lean_render::render(&output).map_err(std::io::Error::other)?
            );
            return Ok(());
        }
        [cmd, path, id, rest @ ..] if cmd == "outline" && (rest.is_empty() || rest == ["full"]) => {
            print!(
                "{}",
                sifttext_file_graph::index::outline(
                    &policy,
                    Path::new(path),
                    id,
                    !rest.is_empty(),
                )?
            );
            return Ok(());
        }
        [cmd, path, query, rest @ ..] if cmd == "search" && rest.len() <= 1 => {
            let result = sifttext_file_graph::search::local_search(
                &policy,
                Path::new(path),
                query,
                rest.first().map(String::as_str),
            )
            .map_err(|e| {
                if let Some(refusal) = e.downcast_ref::<sifttext_file_graph::index::Refusal>() {
                    return *refusal;
                }
                sifttext_file_graph::index::Refusal::from_error(
                    if e.is::<rusqlite::Error>() {
                        "index_cache"
                    } else {
                        "search"
                    },
                    e.as_ref(),
                )
            })?;
            if result.status != "current" {
                return Err(result
                    .refusal
                    .unwrap_or(sifttext_file_graph::index::Refusal {
                        stage: "search",
                        kind: "unknown",
                    })
                    .into());
            }
            json!({"status":result.status,"version":result.version,"total_matches":result.total_matches,"hits":result.hits})
        }
        [cmd, path, sql] if cmd == "sql" => local::sql(&policy, Path::new(path), sql)?,
        _ => return Err("unknown command or arguments; run sifttext -h".into()),
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn absolute(path: &str) -> Result<PathBuf> {
    let path = PathBuf::from(path);
    Ok(if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    })
}

fn main() {
    let updating = std::env::args().nth(1).is_some_and(|arg| arg == "update");
    let result = (|| -> Result<()> {
        let _operation = if updating {
            None
        } else {
            sifttext_file_graph::updates::operation_lock()?
        };
        run()
    })();
    if let Err(error) = result {
        eprintln!("rejected: {error}");
        std::process::exit(1);
    }
    if !updating {
        sifttext_file_graph::updates::opportunistic();
    }
}
