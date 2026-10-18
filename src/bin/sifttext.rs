mod account;

use serde_json::json;
use sifttext_file_graph::{Policy, Result, local};
use std::{
    collections::BTreeSet,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const SOURCE_REPOSITORY: &str = "https://github.com/gojack10/sifttext-core.git";
const MAINTENANCE_NODE: &str = "6da0b35a-955a-4ad2-bdce-2994ba26119c";
const LARGE_RETURN_LIMIT: usize = 15_000;

#[derive(Debug)]
struct LargeReturnSuppressed(usize);

impl std::fmt::Display for LargeReturnSuppressed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{} tokens suppressed] Re-run with -y to print them, read sql schema if needed.",
            self.0
        )
    }
}

impl std::error::Error for LargeReturnSuppressed {}

const HELP: &str = "SiftText — local Markdown knowledge, Git history, derived retrieval

sifttext login                            Open browser sign-in and verify paid Platform activation
sifttext account                          Show server-confirmed account/subscription (refreshes login)
sifttext logout                           Revoke remote login; always remove local credentials offline
                                          Credentials: ~/.config/sifttext/account (outside knowledge)
sifttext source-code                      Check out matching Core source and maintenance guidance
sifttext runtime-check                     Verify executable assets without reading a knowledge store
sifttext update                            Check and apply compatible official product updates
sifttext update [-y]                       Allow a large update receipt
sifttext update --status                    Show installed identities and pending update state
sifttext update --resume                    Submit an agent-resolved KB merge and activate its release
sifttext update --rollback                  Restore the previous release through checked KB reversal
sifttext update --rebuild                   Rebuild the selected local source and retain its pin
sifttext update --config                    Read the user's update mode and standing grant
sifttext update --grant standing|ask        Record or revoke the user's routine-update grant
sifttext update --mode notify|disabled      Choose whether ordinary commands check releases
sifttext update --attach-node=UUID          Register the repair undertaking for a pending update
sifttext update --pin IDENTITY              Keep a chosen build through later official updates
sifttext update --unpin                     Let official updates activate again
sifttext init HOME                         Copy the embedded first-party KB (from native files) to a NEW Git home
sifttext init HOME --bundle BUNDLE         Copy a supplied native KB to a NEW independent Git home
sifttext init HOME --empty                 Explicitly initialize an empty knowledge home
                                          Existing recognized home/edits always unchanged
sifttext instructions [STORE]              Print checked file-first version and entry path
sifttext read STORE UUID [-y]              Check source; -y prints exact node.md above 15000 tokens
sifttext submit REPO DRAFT COMMIT BASE [EXPLICIT_UUID ...]
                                          Check/save in draft, refresh HOME, remove completed draft
sifttext accepted REPO [-y]                Recover accepted view; -y allows >15000 receipt tokens
sifttext advance HOME                      Explicit ff-only clean home reader + derived refresh
sifttext worktree HOME                     Allocate a private branch/worktree; prints its path
sifttext create WORKTREE NAME SUMMARY [PARENT_UUID]
                                          Create a tree root or child; never commits
sifttext session-capture SOURCE DESCRIPTOR.json ARCHIVE_ROOT
                                          Copy and Git-checkpoint a validated Pi session
sifttext session-attach HOME NODE DESCRIPTOR.json TOPIC ARCHIVE_ROOT [-y]
                                          Attach a retained session; -y allows >15000 receipt tokens
sifttext session-backup HOME                 Push the opted-in archive to its configured Git remote
sifttext session-restore HOME ARCHIVE_ROOT Rebuild links; clone configured backup if archive is absent
sifttext session-retention-config HOME  Read local session-retention/backup settings (default off)
sifttext check STORE [BEFORE [EXPLICIT_UUID ...]] [-y]
                                          Complete candidate check; -y allows >15000 output tokens
sifttext normalize BEFORE WORKTREE [EXPLICIT_UUID ...] [-y]
                                          Apply native status activation/reopening; -y allows >15000 receipt tokens
sifttext index STORE                       Refresh derived SQLite and LEAN outlines
sifttext outline STORE UUID [full] [-y]  Refresh, verify, then print shallow/full LEAN
sifttext search STORE QUERY [UNDER_UUID] [-y]
                                          Ranked lexical search; -y allows >15000 output tokens
sifttext sql STORE 'SELECT ...' [-y]       Read-only same-version query; -y allows >15000 tokens
sifttext -h                               This reference

Read node.md with ordinary file tools. Edit only your private worktree.
Git publication is explicit: inspect, validate the exact candidate, commit,
then submit to combine accepted main, refresh the home and remove the saved draft.
After interruption, preserve the original submit arguments and retry that request.
No fixed node-count ceiling; checked resource bounds, 1 MiB authored files,
5000-token crystallization ceiling. Default KB is separate from editable homes.
No hosted account, MCP, model provider, or Dark Factory required.";

fn run() -> Result<()> {
    macro_rules! println {
        ($($arg:tt)*) => {
            writeln!(std::io::stdout().lock(), $($arg)*)?;
        };
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args == ["-h"] || args == ["--help"] {
        println!("{HELP}");
        return Ok(());
    }
    if let Some(command) = args.first()
        && account::is_command(command)
    {
        if args.len() != 1 {
            return Err(format!("usage: sifttext {command}").into());
        }
        return account::run(command);
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
    if args.first().is_some_and(|arg| arg == "session-backup") {
        let [_, home] = args.as_slice() else {
            return Err("usage: sifttext session-backup HOME".into());
        };
        println!(
            "{}",
            serde_json::to_string(&local::backup_session_archive(Path::new(home))?)?
        );
        return Ok(());
    }
    if args.first().is_some_and(|arg| arg == "source-code") {
        if args.len() != 1 {
            return Err("usage: sifttext source-code".into());
        }
        let output = source_code()?;
        println!(
            "{}",
            sifttext_file_graph::lean_render::render(&output).map_err(std::io::Error::other)?
        );
        return Ok(());
    }
    if args.first().is_some_and(|arg| arg == "update") {
        let mut update_args = args.as_slice();
        let force = update_args.last().is_some_and(|arg| arg == "-y");
        if force {
            update_args = &update_args[..update_args.len() - 1];
        }
        let mode = match update_args {
            [_] => String::new(),
            [_, mode]
                if [
                    "--status",
                    "--resume",
                    "--rollback",
                    "--config",
                    "--unpin",
                    "--rebuild",
                ]
                .contains(&mode.as_str()) =>
            {
                if mode == "--unpin" {
                    "--pin=current".to_owned()
                } else {
                    mode.clone()
                }
            }
            [_, flag, value] if flag == "--pin" => format!("--pin={value}"),
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
        let output = sifttext_file_graph::updates::update_with_receipt_preflight(
            Path::new(&root),
            &mode,
            |receipt| preflight_update_receipt(&mode, receipt, force),
        )?;
        let rendered = render_update_output(&mode, &output)?;
        print_large_return(&format!("{rendered}\n"), force)?;
        return Ok(());
    }
    let runtime = local::runtime()?;
    let policy = Policy::checked(&runtime)?;
    // Bundle/bootstrap assets belong to this packaged onramp, not the generic
    // native Policy contract (which also supports minimal policy-only fixtures).
    sifttext_file_graph::native_kb::verify_assets(&runtime)?;
    sifttext_file_graph::graph_transition::Transition::checked(&policy)?;
    if args.first().is_some_and(|arg| arg == "instructions") {
        let path = match args.as_slice() {
            [_, path] => PathBuf::from(path),
            [_] => {
                PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?).join("sifttext")
            }
            _ => return Err("usage: sifttext instructions [STORE]".into()),
        };
        let instructions = local::instructions(&policy, &path)?;
        let output = json!({"version":instructions["version"],"path":instructions["path"]});
        println!(
            "{}",
            sifttext_file_graph::lean_render::render(&output).map_err(std::io::Error::other)?
        );
        return Ok(());
    }
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
        [cmd, path, id] if cmd == "read" => {
            print_read(&local::read(&policy, Path::new(path), id)?, false)?;
            return Ok(());
        }
        [cmd, path, id, flag] if cmd == "read" && flag == "-y" => {
            print_read(&local::read(&policy, Path::new(path), id)?, true)?;
            return Ok(());
        }
        [cmd, repo, draft, commit, base, explicit @ ..] if cmd == "submit" => {
            let mut explicit = explicit.to_vec();
            let force = explicit.last().is_some_and(|arg| arg == "-y");
            if force {
                explicit.pop();
            }
            let receipt = local::submit(
                &policy,
                &sifttext_file_graph::checked_publication::Request {
                    repo: repo.into(),
                    accepted_ref: "refs/heads/main".into(),
                    draft: Some(draft.into()),
                    commit: Some(commit.clone()),
                    base: Some(base.clone()),
                    explicit: explicit.into_iter().collect(),
                    pause_before_publish: None,
                    pause_after_publish: None,
                },
            )?;
            print_large_return(
                &format!(
                    "{}\n",
                    sifttext_file_graph::checked_publication::cli_json(&receipt)?
                ),
                force,
            )?;
            if receipt["status"] != "current" {
                return Err("submission unavailable; retain draft and inspect receipt".into());
            }
            return Ok(());
        }
        [cmd, repo, rest @ ..] if cmd == "accepted" && (rest.is_empty() || rest == ["-y"]) => {
            let force = !rest.is_empty();
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
            print_large_return(
                &format!(
                    "{}\n",
                    sifttext_file_graph::checked_publication::cli_json(&receipt)?
                ),
                force,
            )?;
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
        [cmd, home, node, descriptor, topic, archive, rest @ ..]
            if cmd == "session-attach" && (rest.is_empty() || rest == ["-y"]) =>
        {
            let force = !rest.is_empty();
            let descriptor_path = absolute(descriptor)?;
            let metadata = std::fs::symlink_metadata(&descriptor_path)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err("descriptor must be a regular file".into());
            }
            let value = serde_json::from_slice(&std::fs::read(descriptor_path)?)?;
            local::attach_session_with_gate(
                Path::new(home),
                node,
                &value,
                topic,
                Path::new(archive),
                |receipt| {
                    check_large_return(
                        &format!("{}\n", serde_json::to_string_pretty(receipt)?),
                        force,
                    )
                },
            )?
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
            let force = rest.last().is_some_and(|arg| arg == "-y");
            let rest = if force { &rest[..rest.len() - 1] } else { rest };
            let report = local::check(
                &policy,
                Path::new(path),
                rest.first().map(Path::new),
                rest.iter().skip(1).cloned().collect(),
            )?;
            print_large_return(&format!("{}\n", report.cli_lean()?), force)?;
            return Ok(());
        }
        [cmd, before, after, rest @ ..] if cmd == "normalize" => {
            let force = rest.last().is_some_and(|arg| arg == "-y");
            let explicit = if force { &rest[..rest.len() - 1] } else { rest };
            let normalized = sifttext_file_graph::status::normalize_with_preflight(
                &policy,
                Path::new(before),
                Path::new(after),
                &explicit.iter().cloned().collect(),
                None,
                |receipt| {
                    let output = serde_json::to_value(receipt)?;
                    check_large_return(
                        &format!("{}\n", serde_json::to_string_pretty(&output)?),
                        force,
                    )
                },
            )?;
            normalized.report_reopenings();
            serde_json::to_value(normalized)?
        }
        [cmd, path] if cmd == "index" => {
            let v = local::refresh(&policy, Path::new(path))?;
            let output = json!({"status":v.status,"version":v.version,"version_scope":"authored_store","nodes":v.nodes.len(),"links":v.links.len()});
            println!(
                "{}",
                sifttext_file_graph::lean_render::render(&output).map_err(std::io::Error::other)?
            );
            return Ok(());
        }
        [cmd, path, id, rest @ ..] if cmd == "outline" => {
            let (full, force) = match rest {
                [] => (false, false),
                [flag] if flag == "full" => (true, false),
                [flag] if flag == "-y" => (false, true),
                [full, force] if full == "full" && force == "-y" => (true, true),
                _ => return Err("usage: sifttext outline STORE UUID [full] [-y]".into()),
            };
            let output = sifttext_file_graph::index::outline(&policy, Path::new(path), id, full)?;
            print_large_return(&output, force)?;
            return Ok(());
        }
        [cmd, path, query, rest @ ..] if cmd == "search" => {
            let force = rest.last().is_some_and(|arg| arg == "-y");
            let rest = if force { &rest[..rest.len() - 1] } else { rest };
            if rest.len() > 1 {
                return Err("usage: sifttext search STORE QUERY [UNDER_UUID] [-y]".into());
            }
            let result = sifttext_file_graph::search::local_search(
                &policy,
                Path::new(path),
                query,
                rest.first().map(String::as_str),
            )
            .map_err(|e| {
                if let Some(refusal) = e.downcast_ref::<sifttext_file_graph::index::Refusal>() {
                    return refusal.clone();
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
                        problem: None,
                    })
                    .into());
            }
            let output = json!({"status":result.status,"version":result.version,"version_scope":"authored_store","total_matches":result.total_matches,"hits":result.hits});
            print_large_return(
                &format!("{}\n", serde_json::to_string_pretty(&output)?),
                force,
            )?;
            return Ok(());
        }
        [cmd, path, sql, rest @ ..] if cmd == "sql" && (rest.is_empty() || rest == ["-y"]) => {
            let output = local::sql(&policy, Path::new(path), sql)?;
            let rendered = format!("{}\n", serde_json::to_string_pretty(&output)?);
            print_large_return(&rendered, !rest.is_empty())?;
            return Ok(());
        }
        _ => return Err("unknown command or arguments; run sifttext -h".into()),
    };
    if args.first().is_some_and(|cmd| {
        matches!(
            cmd.as_str(),
            "normalize"
                | "session-capture"
                | "session-attach"
                | "session-backup"
                | "session-restore"
        )
    }) {
        // These receipts have nested schemas beyond LEAN's scalar-row dialect.
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "{}",
            sifttext_file_graph::lean_render::render(&output).map_err(std::io::Error::other)?
        );
    }
    Ok(())
}

fn print_read(output: &str, force: bool) -> Result<()> {
    print_large_return(output, force)
}

fn print_large_return(output: &str, force: bool) -> Result<()> {
    check_large_return(output, force)?;
    std::io::stdout().lock().write_all(output.as_bytes())?;
    Ok(())
}

fn check_large_return(output: &str, force: bool) -> Result<()> {
    if !force {
        let tokens = tiktoken_rs::o200k_base_singleton()
            .lock()
            .encode_ordinary(output)
            .len();
        if tokens > LARGE_RETURN_LIMIT {
            return Err(LargeReturnSuppressed(tokens).into());
        }
    }
    Ok(())
}

fn valid_revision(revision: &str) -> bool {
    revision.len() == 40
        && revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn source_code() -> Result<serde_json::Value> {
    let revision = option_env!("SIFTTEXT_SOURCE_REVISION")
        .filter(|revision| valid_revision(revision))
        .ok_or("matching source revision is unavailable for this build")?;
    let checkout = clone_source_at(SOURCE_REPOSITORY, revision)?;
    let guide = checkout
        .join("nodes")
        .join(MAINTENANCE_NODE)
        .join("node.md");
    if !guide.is_file() {
        let _ = std::fs::remove_dir_all(&checkout);
        return Err("matching source checkout lacks its maintenance guide".into());
    }
    Ok(json!({
        "path": checkout.display().to_string(),
        "revision": revision,
        "maintenance_instructions": guide.display().to_string()
    }))
}

fn clone_source_at(repository: &str, revision: &str) -> Result<PathBuf> {
    if !valid_revision(revision) {
        return Err("invalid source revision".into());
    }
    let checkout = tempfile::Builder::new()
        .prefix("sifttext-source-")
        .tempdir()?;
    let path = checkout.path();
    let clone = Command::new("git")
        .args([
            "clone",
            "--quiet",
            "--filter=blob:none",
            "--no-checkout",
            repository,
        ])
        .arg(path)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()?;
    if !clone.status.success() {
        return Err(format!(
            "source clone failed: {}",
            String::from_utf8_lossy(&clone.stderr)
        )
        .into());
    }
    let path_arg = path.to_str().ok_or("source path is not UTF-8")?;
    let result = Command::new("git")
        .args(["-C", path_arg, "checkout", "--quiet", "--detach", revision])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()?;
    if !result.status.success() {
        return Err(format!(
            "source revision unavailable: {}",
            String::from_utf8_lossy(&result.stderr)
        )
        .into());
    }
    let actual = Command::new("git")
        .args(["-C", path_arg, "rev-parse", "HEAD"])
        .output()?;
    if !actual.status.success() || String::from_utf8_lossy(&actual.stdout).trim() != revision {
        return Err("source checkout revision mismatch".into());
    }
    Ok(checkout.keep())
}

fn render_update_output(mode: &str, output: &serde_json::Value) -> Result<String> {
    if mode == "--config"
        || ["--grant=", "--mode=", "--pin="]
            .iter()
            .any(|prefix| mode.starts_with(prefix))
    {
        Ok(sifttext_file_graph::lean_render::render(output).map_err(std::io::Error::other)?)
    } else {
        Ok(serde_json::to_string_pretty(output)?)
    }
}

fn preflight_update_receipt(mode: &str, receipt: &serde_json::Value, force: bool) -> Result<()> {
    check_large_return(
        &format!("{}\n", render_update_output(mode, receipt)?),
        force,
    )
}

fn absolute(path: &str) -> Result<PathBuf> {
    let path = PathBuf::from(path);
    Ok(if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_revision_validation_requires_a_full_git_id() {
        assert!(valid_revision("0123456789abcdef0123456789abcdef01234567"));
        assert!(!valid_revision("main"));
        assert!(!valid_revision("0123456789ABCDEF0123456789ABCDEF01234567"));
        assert!(!valid_revision("0123456789abcdef0123456789abcdef0123456z"));
    }

    #[test]
    fn source_checkout_is_detached_at_the_requested_revision() {
        let source = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            let output = Command::new("git")
                .args(args)
                .current_dir(source.path())
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap().trim().to_owned()
        };
        run(&["init", "--quiet"]);
        run(&["config", "user.name", "Test"]);
        run(&["config", "user.email", "test@example.invalid"]);
        std::fs::write(source.path().join("README.md"), "matching source\\n").unwrap();
        run(&["add", "README.md"]);
        run(&["commit", "--quiet", "-m", "test source"]);
        let revision = run(&["rev-parse", "HEAD"]);
        let checkout = clone_source_at(source.path().to_str().unwrap(), &revision).unwrap();
        assert_eq!(
            std::fs::read_to_string(checkout.join("README.md")).unwrap(),
            "matching source\\n"
        );
        let actual = Command::new("git")
            .args(["-C", checkout.to_str().unwrap(), "rev-parse", "HEAD"])
            .output()
            .unwrap();
        assert!(actual.status.success());
        assert_eq!(String::from_utf8(actual.stdout).unwrap().trim(), revision);
        assert!(checkout.join(".git").exists());
        let origin = Command::new("git")
            .args([
                "-C",
                checkout.to_str().unwrap(),
                "remote",
                "get-url",
                "origin",
            ])
            .output()
            .unwrap();
        assert!(origin.status.success());
        assert_eq!(
            String::from_utf8(origin.stdout).unwrap().trim(),
            source.path().to_str().unwrap()
        );
        assert!(
            Command::new("git")
                .args([
                    "-C",
                    checkout.to_str().unwrap(),
                    "symbolic-ref",
                    "--quiet",
                    "HEAD"
                ])
                .output()
                .unwrap()
                .status
                .code()
                .is_some_and(|code| code != 0)
        );
        std::fs::remove_dir_all(checkout).unwrap();
    }

    #[test]
    fn large_update_receipt_preflight_obeys_override() {
        let receipt =
            json!({"status":"updated", "refresh_instruction_paths":["path".repeat(20_000)]});
        assert!(preflight_update_receipt("", &receipt, false).is_err());
        assert!(preflight_update_receipt("", &receipt, true).is_ok());
    }

    #[test]
    fn update_settings_use_lean_and_transaction_receipts_stay_json() {
        let settings = json!({
            "mode": "notify",
            "permission": "ask",
            "pinned": null,
            "source": "file",
            "path": "/home/example/.sifttext/config.json"
        });
        for mode in [
            "--config",
            "--grant=standing",
            "--mode=disabled",
            "--pin=release-id",
        ] {
            let rendered = render_update_output(mode, &settings).unwrap();
            assert!(!rendered.starts_with('{'), "{mode}: {rendered}");
            assert!(rendered.contains("mode:notify"), "{mode}: {rendered}");
            assert!(rendered.contains("permission:ask"), "{mode}: {rendered}");
        }

        let receipt = json!({"status":"current", "application":"v1", "knowledge":"rev"});
        assert_eq!(
            render_update_output("--status", &receipt).unwrap(),
            serde_json::to_string_pretty(&receipt).unwrap()
        );
    }
}

fn main() {
    let authentication = std::env::args()
        .nth(1)
        .is_some_and(|arg| account::is_command(&arg));
    let updating = std::env::args().nth(1).is_some_and(|arg| arg == "update");
    let checking = std::env::args().nth(1).is_some_and(|arg| arg == "check");
    let force = std::env::args().last().is_some_and(|arg| arg == "-y");
    let result = (|| -> Result<()> {
        let _operation = if updating || authentication {
            None
        } else {
            sifttext_file_graph::updates::operation_lock()?
        };
        run()
    })();
    if let Err(error) = result {
        if let Some(suppressed) = error.downcast_ref::<LargeReturnSuppressed>() {
            let _ = writeln!(std::io::stdout().lock(), "{suppressed}");
        } else {
            let rendered = format!("rejected: {error}\n");
            if checking && let Err(suppressed) = check_large_return(&rendered, force) {
                let _ = writeln!(std::io::stdout().lock(), "{suppressed}");
            } else {
                let _ = std::io::stderr().lock().write_all(rendered.as_bytes());
            }
        }
        std::process::exit(1);
    }
    if !updating && !authentication {
        sifttext_file_graph::updates::opportunistic();
    }
}
