use sifttext_file_graph::account;
mod sync;
#[path = "cli/commands.rs"]
mod commands;
#[path = "cli/onboarding.rs"]
mod onboarding;

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
struct LargeReturnSuppressed {
    tokens: usize,
    command: String,
}

impl std::fmt::Display for LargeReturnSuppressed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{} tokens suppressed] Re-run this sifttext {} command with -y to print them.",
            self.tokens, self.command
        )
    }
}

impl std::error::Error for LargeReturnSuppressed {}

#[derive(Debug)]
struct AcceptedSubmissionIncomplete;

impl std::fmt::Display for AcceptedSubmissionIncomplete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("knowledge accepted, but the save could not finish. Keep the draft and original submit arguments for recovery. See the receipt for the cause.")
    }
}

impl std::error::Error for AcceptedSubmissionIncomplete {}

#[derive(Debug)]
struct ReportedRenderFailure;
impl std::fmt::Display for ReportedRenderFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("tree rendering rejected")
    }
}
impl std::error::Error for ReportedRenderFailure {}

fn run(hosted_summary: &mut Option<String>) -> Result<()> {
    macro_rules! println {
        ($($arg:tt)*) => {
            writeln!(std::io::stdout().lock(), $($arg)*)?;
        };
    }
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // Subcommand help must never be mistaken for a new knowledge-home path.
    if args.len() == 2 && matches!(args[1].as_str(), "-h" | "--help") {
        args.remove(0);
    }
    if args.is_empty() || args == ["help"] || args == ["-h"] || args == ["--help"] {
        println!("{}", commands::help()?);
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
    if args.first().is_some_and(|cmd| sifttext_file_graph::hosted_checks::is_command(cmd)) {
        return sifttext_file_graph::hosted_checks::run(&args);
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
            render_cli_receipt(&local::session_retention_config(Path::new(home))?)?
        );
        return Ok(());
    }
    if args.first().is_some_and(|arg| arg == "session-backup") {
        let [_, home] = args.as_slice() else {
            return Err("usage: sifttext session-backup HOME".into());
        };
        let receipt = local::backup_session_archive(Path::new(home))?;
        println!(
            "{}",
            render_cli_receipt(&json!({"status":receipt["status"], "commit":receipt["commit"]}))?
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
                    "--local-status",
                    "--resume",
                    "--rollback",
                    "--config",
                    "--unpin",
                    "--rebuild",
                    "--opportunistic", // Detached release check; not a public update operation.
                ]
                .contains(&mode.as_str()) =>
            {
                if mode == "--unpin" {
                    "--pin=current".to_owned()
                } else {
                    mode.clone()
                }
            }
            [_, flag] if flag.starts_with("--attach-node=") => flag.clone(),
            [_, flag, value] if flag == "--attach-node" => format!("--attach-node={value}"),
            [_, flag, value] if flag == "--pin" => format!("--pin={value}"),
            [_, flag, value]
                if flag == "--grant" && ["standing", "ask"].contains(&value.as_str()) =>
            {
                format!("{flag}={value}")
            }
            _ => return Err("unknown update arguments".into()),
        };
        let root = std::env::var_os("SIFTTEXT_INSTALL_ROOT")
            .ok_or("use the managed installation launcher for updates")?;
        let mut submission = None;
        let result = sifttext_file_graph::updates::update_with_submission_receipt(
            Path::new(&root),
            &mode,
            |receipt| preflight_update_receipt(&mode, receipt, force),
            &mut submission,
        );
        if std::env::var("SIFTTEXT_RECEIPT_FORMAT").as_deref() != Ok("json")
            && let Some(receipt) = submission
        {
            if let Some(check) = receipt.get("hosted_check") {
                // Main emits errors/notices before this summary and its final credits.
                *hosted_summary = Some(sifttext_file_graph::hosted_checks::render_publication(check));
            }
            print_large_return(&format!("{}\n", render_publication_receipt(&receipt)?), force)?;
        }
        let output = result?;
        let rendered = render_update_output(&mode, &output)?;
        print_large_return(&format!("{rendered}\n"), force)?;
        return Ok(());
    }
    // Maintenance never runs for update --local-status or other early-return
    // account/help commands. It is independent of the requested operation.
    sifttext_file_graph::scratch::cleanup();
    let runtime = local::runtime()?;
    if args.first().is_some_and(|arg| arg == "sync") {
        let output = sync::run(&runtime, &args)?;
        if !output.is_null() {
            println!("{}", render_cli_receipt(&output)?);
        }
        return Ok(());
    }
    let policy = checked_policy(&runtime)?;
    if args.first().is_some_and(|arg| arg == "instructions") {
        let path = match args.as_slice() {
            [_, path] => PathBuf::from(path),
            [_] => {
                PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?).join("sifttext")
            }
            _ => return Err("usage: sifttext instructions [STORE]".into()),
        };
        let instructions = local::instructions(&policy, &path)?;
        println!("{}", onboarding::render(&path, &instructions)?);
        return Ok(());
    }
    let output = match args.as_slice() {
        [cmd, draft, flag] if cmd == "migrate-order" && flag == "--legacy" => {
            sifttext_file_graph::ordering::restore_legacy_draft(&absolute(draft)?)?
        }
        [cmd, draft] if cmd == "migrate-order" => {
            let draft = absolute(draft)?;
            sifttext_file_graph::ordering::migrate_draft(&draft)?
        }
        [cmd] if cmd == "runtime-check" => json!({"status":"checked", "runtime":runtime}),
        [cmd] if cmd == "hosted-account-protocol" => json!({"hosted_account":1}),
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
            let submit = if explicit == ["--received-statuses"] {
                explicit.clear();
                local::submit_received
            } else {
                local::submit
            };
            let receipt = submit(
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
                    render_publication_receipt(&receipt)?
                ),
                force || receipt["status"] == "current",
            )?;
            let failure: Option<Box<dyn std::error::Error>> = if receipt["status"] == "current" {
                None
            } else if receipt["published"] == true || receipt["publication"] == "accepted_incomplete" {
                Some(AcceptedSubmissionIncomplete.into())
            } else {
                Some("submission unavailable; retain draft and inspect receipt".into())
            };
            if std::env::var("SIFTTEXT_RECEIPT_FORMAT").as_deref() != Ok("json") {
                if let Some(check) = receipt.get("hosted_check") {
                    // Main prints diagnostics and existing updater notices first,
                    // after releasing the operation lock, then flushes credits last.
                    *hosted_summary = Some(sifttext_file_graph::hosted_checks::render_publication(check));
                }
            }
            if let Some(error) = failure {
                return Err(error);
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
                    render_publication_receipt(&receipt)?
                ),
                force || receipt.status == "current",
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
                        &format!("{}\n", render_cli_receipt(receipt)?),
                        force,
                    )
                },
            )?
        }
        [cmd, home, archive] if cmd == "session-restore" => {
            local::restore_session_links(Path::new(home), Path::new(archive))?
        }
        [cmd, path, name, summary, rest @ ..] if cmd == "create" && rest.len() <= 1 => {
            let created = local::create(
                &policy,
                Path::new(path),
                name,
                summary,
                rest.first().map(String::as_str),
            )?;
            json!({"id":created["id"],"path":created["path"]})
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
                    check_large_return(
                        &format!("{}\n", render_cli_receipt(&receipt.cli_json())?),
                        force,
                    )
                },
            )?;
            normalized.cli_json()
        }
        [cmd, path, rest @ ..] if cmd == "index" => {
            let output = match rest {
                [] => local::refresh(&policy, Path::new(path))?,
                [flag] if flag == "--repair" => sifttext_file_graph::index::repair(&policy, Path::new(path))?,
                _ => return Err("usage: sifttext index STORE [--repair]".into()),
            };
            println!(
                "{}",
                sifttext_file_graph::lean_render::render(&output).map_err(std::io::Error::other)?
            );
            return Ok(());
        }
        [cmd, path, selection] if cmd == "tree-render" => {
            let input = std::fs::read(selection)?;
            let request = serde_json::from_slice(&input)?;
            let output = sifttext_file_graph::tree_render::run(&policy, Path::new(path), &request)?;
            println!("{}", render_cli_receipt(&output)?);
            return if output["status"] == "render-success" { Ok(()) } else { Err(ReportedRenderFailure.into()) };
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
            let output = result.cli_value();
            let rendered = sifttext_file_graph::lean_render::render(&output)
                .map_err(std::io::Error::other)?
                + "\n";
            print_large_return(&rendered, force)?;
            return Ok(());
        }
        [cmd, path, sql, rest @ ..] if cmd == "sql" && (rest.is_empty() || rest == ["-y"]) => {
            let rendered = local::sql_lean(&policy, Path::new(path), sql)?;
            print_large_return(&rendered, !rest.is_empty())?;
            return Ok(());
        }
        _ => return Err("unknown command or arguments; run sifttext -h".into()),
    };
    println!("{}", render_cli_receipt(&output)?);
    Ok(())
}

fn render_cli_receipt(output: &serde_json::Value) -> Result<String> {
    if std::env::var("SIFTTEXT_RECEIPT_FORMAT").as_deref() == Ok("json") {
        return Ok(serde_json::to_string_pretty(output)?);
    }
    sifttext_file_graph::lean_render::render_receipt(output)
        .map_err(|error| std::io::Error::other(error).into())
}

fn render_publication_receipt(receipt: &impl serde::Serialize) -> Result<String> {
    // The trusted parent parses this single JSON transaction receipt. Ordinary
    // local receipts retain the same projection through the LEAN renderer.
    let json = sifttext_file_graph::checked_publication::cli_json(receipt)?;
    if std::env::var("SIFTTEXT_RECEIPT_FORMAT").as_deref() == Ok("json") {
        Ok(json)
    } else {
        render_cli_receipt(&serde_json::from_str(&json)?)
    }
}

fn checked_policy(runtime: &Path) -> Result<Policy> {
    let policy = Policy::checked(runtime)?;
    sifttext_file_graph::native_kb::verify_assets(runtime)?;
    Ok(policy)
}

fn print_read(output: &str, force: bool) -> Result<()> {
    // Every token consumes at least one byte. Small ordinary reads need no tokenizer.
    if output.len() <= LARGE_RETURN_LIMIT {
        std::io::stdout().lock().write_all(output.as_bytes())?;
        Ok(())
    } else {
        print_large_return(output, force)
    }
}

fn print_large_return(output: &str, force: bool) -> Result<()> {
    check_large_return(output, force)?;
    std::io::stdout().lock().write_all(output.as_bytes())?;
    Ok(())
}

fn check_large_return(output: &str, force: bool) -> Result<()> {
    // Ordinary tokens consume at least one UTF-8 byte; small output needs no tokenizer.
    if !force && output.len() > LARGE_RETURN_LIMIT {
        let tokens = sifttext_file_graph::token_count::ordinary(output);
        if tokens > LARGE_RETURN_LIMIT {
            return Err(LargeReturnSuppressed {
                tokens,
                command: std::env::args().nth(1).unwrap_or_default(),
            }
            .into());
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
    let mut output = output.clone();
    if mode == "--status" {
        // The persisted journal is recovery input, not a CLI receipt. Keep the
        // usable identities and unfinished work, not manifests, clocks or hashes
        // already available in the installation's state.json.
        let mut status = json!({
            "home":output["home"],
            "application":output["application"],
            "knowledge":output["knowledge"],
            "activation_pending":output["activation_pending"],
            "update_check":output["update_check"],
        });
        if let Some(previous) = output.get("previous").filter(|v| v.is_object()) {
            status["rollback_application"] = previous["application"].clone();
            status["rollback_knowledge"] = previous["knowledge"].clone();
        }
        if let Some(pending) = output.get("pending").filter(|v| v.is_object()) {
            status["worktree"] = pending["draft"].clone();
            if pending["plan"].get("Rollback").is_some() {
                status["pending_operation"] = json!("rollback");
            } else if pending["plan"].get("Release").is_some() {
                status["pending_operation"] = json!("update");
            }
            for field in ["commit", "repair_node"] {
                if let Some(value) = pending.get(field).filter(|v| !v.is_null()) {
                    status[format!("pending_{field}")] = value.clone();
                }
            }
            status["resume"] = json!("sifttext update --resume");
        } else if output["activation_pending"] == true {
            status["resume"] = json!("sifttext update --resume");
        }
        if output["update_check"] == "update_available" {
            for field in ["application", "next_action", "command", "permission", "grant_command", "reason", "guide"] {
                if let Some(value) = output.get(format!("available_{field}")) {
                    status[format!("available_{field}")] = value.clone();
                }
            }
        }
        if let Some(error) = output.get("update_error") {
            status["update_error"] = error.clone();
        }
        output = status;
    } else if mode == "--config"
        || mode.starts_with("--grant=")
        || mode.starts_with("--pin=")
    {
        let settings = output.as_object_mut().ok_or("update settings must be an object")?;
        settings.remove("source");
        if settings.get("pinned").is_some_and(|v| v.is_null()) {
            settings.remove("pinned");
        }
    }
    if output["refresh_instruction_paths"] == json!([]) {
        output.as_object_mut().unwrap().remove("refresh_instruction_paths");
    }
    render_cli_receipt(&output)
}

fn render_update_error(error: &dyn std::fmt::Display) -> Result<String> {
    Ok(sifttext_file_graph::lean_render::render(&json!({
        "status":"error",
        "message":error.to_string(),
    }))
    .map_err(std::io::Error::other)? + "\n")
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
    fn large_return_warning_names_the_command() {
        for command in [
            "check",
            "read",
            "outline",
            "search",
            "submit",
            "accepted",
            "normalize",
            "session-attach",
            "update",
            "sql",
        ] {
            let notice = LargeReturnSuppressed {
                tokens: 15_001,
                command: command.into(),
            }
            .to_string();
            assert_eq!(
                notice,
                format!(
                    "[15001 tokens suppressed] Re-run this sifttext {command} command with -y to print them."
                )
            );
            assert!(!notice.contains("schema"));
        }
    }

    #[test]
    fn large_update_receipt_preflight_obeys_override() {
        let receipt =
            json!({"status":"updated", "refresh_instruction_paths":["path".repeat(20_000)]});
        assert!(preflight_update_receipt("", &receipt, false).is_err());
        assert!(preflight_update_receipt("", &receipt, true).is_ok());
    }

    #[test]
    fn update_settings_and_transactions_use_lean() {
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
            assert!(rendered.contains("path:/home/example/.sifttext/config.json"));
            assert!(!rendered.contains("source:"));
            assert!(!rendered.contains("pinned:"));
        }

        let receipt = json!({
            "status":"update_available",
            "application":"v0.3.3",
            "pending":{"worktree":"/tmp/draft"},
            "paths":["one", "two"]
        });
        let rendered = render_update_output("", &receipt).unwrap();
        assert!(!rendered.starts_with('{'), "{rendered}");
        assert!(rendered.contains("status:update_available"), "{rendered}");
        assert!(rendered.contains("application:v0.3.3"), "{rendered}");
        assert!(rendered.contains("pending:"), "{rendered}");
        assert!(rendered.contains("paths:"), "{rendered}");
    }

    #[test]
    fn update_status_keeps_recovery_actions_without_dumping_the_journal() {
        let output = json!({
            "home":"/home/example/knowledge",
            "application":"releases/v2",
            "application_sha256":"a".repeat(64),
            "knowledge":"b".repeat(64),
            "baseline":"c".repeat(40),
            "success":{"wall":1,"uptime":2,"boot":"fixture"},
            "attempt":{"wall":3,"uptime":4,"boot":"fixture"},
            "previous":{"application":"releases/v1","knowledge":"d".repeat(64),"baseline":"e".repeat(40),"application_sha256":"f".repeat(64)},
            "pending":{
                "draft":"/tmp/retained-draft",
                "base":"c".repeat(40),
                "commit":"1".repeat(40),
                "repair_node":"00000000-0000-4000-8000-000000000001",
                "plan":{"Release":{"manifest":"private recovery input"}},
                "changed":(0..1000).map(|i| format!("nodes/{i:040}/node.md")).collect::<Vec<_>>()
            },
            "activation_pending":true,
            "update_check":"update_available",
            "available_application":"v3",
            "available_next_action":"Ask the user to update SiftText as soon as possible.",
            "available_command":"sifttext update",
            "available_permission":"ask",
            "available_grant_command":"sifttext update --grant standing",
            "available_reason":"The latest release fixes speed and stability.",
            "available_config":"/home/example/knowledge/.sifttext/config.json",
            "available_guide":"/home/example/knowledge/nodes/guide/node.md"
        });
        let rendered = render_update_output("--status", &output).unwrap();
        let shown = sifttext_file_graph::outlines::decode_lean(&format!("{rendered}\n")).unwrap();
        for field in ["home", "application", "knowledge", "activation_pending", "update_check",
            "available_application", "available_next_action", "available_command", "available_permission", "available_grant_command", "available_reason", "available_guide"] {
            assert_eq!(shown[field], output[field], "{field}");
        }
        assert_eq!(shown["worktree"], output["pending"]["draft"]);
        assert_eq!(shown["pending_operation"], "update");
        let mut rollback = output.clone();
        rollback["pending"]["plan"] = json!({"Rollback":{}});
        let rollback = render_update_output("--status", &rollback).unwrap();
        assert!(rollback.contains("pending_operation:rollback"));
        assert_eq!(shown["pending_commit"], output["pending"]["commit"]);
        assert_eq!(shown["pending_repair_node"], output["pending"]["repair_node"]);
        assert_eq!(shown["resume"], "sifttext update --resume");
        assert_eq!(shown["rollback_application"], output["previous"]["application"]);
        assert_eq!(shown["rollback_knowledge"], output["previous"]["knowledge"]);
        for field in ["application_sha256", "baseline", "success", "attempt", "previous", "pending", "available_config"] {
            assert!(shown.get(field).is_none(), "{field}");
        }
        // The old renderer exposed every state field as a scalar, including the
        // serialized pending journal. Keep that measurement reproducible.
        let old: serde_json::Map<String, serde_json::Value> = output.as_object().unwrap().iter().map(|(key, value)| {
            (key.clone(), if value.is_array() || value.is_object() { json!(value.to_string()) } else { value.clone() })
        }).collect();
        let old = sifttext_file_graph::lean_render::render(&serde_json::Value::Object(old)).unwrap();
        let count = |s: &str| tiktoken_rs::o200k_base_singleton().lock().encode_ordinary(s).len();
        eprintln!("update --status pending journal (1000 changed paths): {} -> {} tokens", count(&old), count(&rendered));
        assert!(count(&rendered) < count(&old) / 10);
        assert!(preflight_update_receipt("--status", &output, false).is_ok());

        let current = json!({"home":"/home/example/knowledge","application":"releases/v2","knowledge":"b".repeat(64),"activation_pending":false,"update_check":"current","available_application":"v2","pending":null,"previous":null});
        let shown = sifttext_file_graph::outlines::decode_lean(&format!("{}\n", render_update_output("--status", &current).unwrap())).unwrap();
        assert_eq!(shown.as_object().unwrap().len(), 5);
        assert!(shown.get("available_application").is_none());
        let mut failed = current;
        failed["update_check"] = json!("error");
        failed["update_error"] = json!("release check failed; active installation retained");
        let shown = sifttext_file_graph::outlines::decode_lean(&format!("{}\n", render_update_output("--status", &failed).unwrap())).unwrap();
        assert_eq!(shown["update_error"], failed["update_error"]);
    }

    #[test]
    fn update_receipts_keep_changed_paths_and_omit_only_an_empty_list() {
        let output = json!({"status":"updated","application":"v2","refresh_instruction_paths":[]});
        assert_eq!(render_update_output("", &output).unwrap(), "application:v2\nstatus:updated");
        let output = json!({"status":"updated","application":"v2","refresh_instruction_paths":["nodes/a/node.md","trees/b"]});
        let rendered = render_update_output("--resume", &output).unwrap();
        let shown = sifttext_file_graph::outlines::decode_lean(&format!("{rendered}\n")).unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(shown["refresh_instruction_paths"].as_str().unwrap()).unwrap(), output["refresh_instruction_paths"]);
        let output = json!({"mode":"notify","permission":"standing","pinned":"local-build","source":"file","path":"/home/example/knowledge/.sifttext/config.json"});
        let shown = sifttext_file_graph::outlines::decode_lean(&format!("{}\n", render_update_output("--pin=local-build", &output).unwrap())).unwrap();
        assert_eq!(shown["pinned"], "local-build");
    }

    #[test]
    fn update_errors_use_friendly_lean() {
        let error = render_update_error(&"Could not download the official release. Check your internet connection, then retry `sifttext update`. Your current installation and local work remain available.").unwrap();
        assert!(error.contains("status:error"), "{error}");
        assert!(
            error.contains("Could not download the official release"),
            "{error}"
        );
        assert!(error.contains("Check your internet connection"), "{error}");
        assert!(!error.starts_with("rejected:"), "{error}");
    }
}

fn main() {
    let authentication = std::env::args()
        .nth(1)
        .is_some_and(|arg| account::is_command(&arg) || sifttext_file_graph::hosted_checks::is_command(&arg));
    let updating = std::env::args().nth(1).is_some_and(|arg| arg == "update");
    let mut hosted_summary = None;
    let result = (|| -> Result<()> {
        let _operation = if updating || authentication {
            None
        } else {
            sifttext_file_graph::updates::operation_lock()?
        };
        run(&mut hosted_summary)
    })();
    let failed = result.is_err();
    if let Err(error) = result {
        if let Some(suppressed) = error.downcast_ref::<LargeReturnSuppressed>() {
            let _ = writeln!(std::io::stdout().lock(), "{suppressed}");
        } else if error.is::<sifttext_file_graph::hosted_checks::ReportedCheckFailure>() || error.is::<ReportedRenderFailure>() {
            // The command already printed its receipt.
        } else if std::env::args().nth(1).as_deref() == Some("tree-render") {
            let receipt = json!({"status":"rejected", "errors":[{
                "code":"render_error", "path":"request", "message":error.to_string()
            }]});
            if let Ok(text) = render_cli_receipt(&receipt) {
                let _ = writeln!(std::io::stdout().lock(), "{text}");
            }
        } else {
            let rendered = if error.is::<AcceptedSubmissionIncomplete>() {
                format!("{error}\n")
            } else if updating {
                render_update_error(error.as_ref()).unwrap_or_else(|_| format!("rejected: {error}\n"))
            } else {
                format!("rejected: {error}\n")
            };
            let _ = std::io::stderr().lock().write_all(rendered.as_bytes());
        }
    } else if !updating && !authentication {
        let args: Vec<_> = std::env::args().skip(1).collect();
        let onboarding_or_help = args.is_empty()
            || args.first().is_some_and(|arg| matches!(arg.as_str(), "instructions" | "help" | "-h" | "--help"))
            || (args.len() == 2 && matches!(args[1].as_str(), "-h" | "--help"));
        sifttext_file_graph::updates::opportunistic_with_notice(!onboarding_or_help);
    }
    if let Some(summary) = hosted_summary {
        let mut out = std::io::stdout().lock();
        if let Err(error) = writeln!(out, "{summary}").and_then(|_| out.flush()) {
            let _ = writeln!(std::io::stderr().lock(), "rejected: {error}");
            std::process::exit(1);
        }
    }
    if failed {
        std::process::exit(1);
    }
}
