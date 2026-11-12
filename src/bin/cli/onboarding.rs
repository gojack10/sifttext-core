//! Agent-facing onboarding, projected from retained state without changing it.
use super::{commands, sync};
use serde_json::{Value, json};
use sifttext_file_graph::{Result, lean_render, updates};
use std::{
    collections::BTreeSet,
    env, fs,
    io::Read,
    mem::MaybeUninit,
    path::{Component, Path, PathBuf},
    process::Command,
};

fn field(lines: &mut Vec<String>, key: &str, value: Value) -> Result<()> {
    lines.push(lean_render::render(&json!({key:value}))?);
    Ok(())
}

fn timestamp(seconds: libc::time_t, utc: bool) -> Option<String> {
    let mut time = MaybeUninit::uninit();
    // Both functions initialize the supplied tm on success.
    let time = unsafe {
        let result = if utc {
            libc::gmtime_r(&seconds, time.as_mut_ptr())
        } else {
            libc::localtime_r(&seconds, time.as_mut_ptr())
        };
        if result.is_null() {
            return None;
        }
        time.assume_init()
    };
    let offset = if utc {
        "Z".to_owned()
    } else {
        let sign = if time.tm_gmtoff < 0 { '-' } else { '+' };
        let minutes = time.tm_gmtoff.unsigned_abs() / 60;
        format!("{sign}{:02}:{:02}", minutes / 60, minutes % 60)
    };
    Some(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{offset}",
        i64::from(time.tm_year) + 1900,
        time.tm_mon + 1,
        time.tm_mday,
        time.tm_hour,
        time.tm_min,
        time.tm_sec,
    ))
}

fn system_time_zone() -> Option<String> {
    let tz = env::var_os("TZ");
    let setting = match &tz {
        Some(value) => {
            let value = value.to_str()?;
            value.strip_prefix(':').unwrap_or(value)
        }
        None => "/etc/localtime",
    };
    let path = Path::new(setting);
    let (file, name) = if path.is_absolute() {
        let file = fs::canonicalize(path).ok()?;
        let (_, name) = file.to_str()?.rsplit_once("/zoneinfo/")?;
        let name = name.to_owned();
        (file, name)
    } else {
        if !path.components().all(|part| matches!(part, Component::Normal(_))) {
            return None;
        }
        let directory = env::var_os("TZDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/usr/share/zoneinfo"));
        (directory.join(path), setting.to_owned())
    };
    // A named setting must point to timezone data. Minimal images can lack it.
    if !file.is_file() {
        return None;
    }
    let mut magic = [0; 4];
    fs::File::open(file).ok()?.read_exact(&mut magic).ok()?;
    if &magic != b"TZif" {
        return None;
    }
    Some(
        name.strip_prefix("posix/")
            .or_else(|| name.strip_prefix("right/"))
            .unwrap_or(&name)
            .to_owned(),
    )
}

fn clock_fields(lines: &mut Vec<String>) -> Result<()> {
    // A null argument asks time() to return the reading without writing through a pointer.
    let seconds = unsafe { libc::time(std::ptr::null_mut()) };
    let (utc, local) = if seconds == -1 {
        (None, None)
    } else {
        (timestamp(seconds, true), timestamp(seconds, false))
    };
    field(lines, "utc-time", json!(utc.as_deref().unwrap_or("unavailable")))?;
    field(
        lines,
        "system-local-time",
        json!(local.as_deref().unwrap_or("unavailable")),
    )?;
    field(
        lines,
        "system-time-zone",
        json!(system_time_zone().as_deref().unwrap_or("unknown")),
    )
}

fn retrieval_tool(names: &[&str]) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    env::var_os("PATH").and_then(|paths| {
        names.iter().find_map(|name| {
            env::split_paths(&paths)
                .map(|directory| directory.join(name))
                .find(|path| fs::metadata(path)
                    .is_ok_and(|file| file.is_file() && file.permissions().mode() & 0o111 != 0))
        })
    })
}

/// Where each retrieval tool resolves on PATH, so an agent need not probe before using it.
fn retrieval_tool_fields(lines: &mut Vec<String>) -> Result<()> {
    // Debian and Ubuntu package fd as fdfind.
    for (key, names) in [("rg", &["rg"][..]), ("fd", &["fd", "fdfind"][..])] {
        let found = retrieval_tool(names);
        field(
            lines,
            key,
            json!(found.map_or("missing".into(), |path| path.to_string_lossy().into_owned())),
        )?;
    }
    Ok(())
}

fn command_output(lines: &mut Vec<String>, key: &str, command: &mut Command) -> Result<()> {
    match command.output() {
        Ok(output) if output.status.success() => {
            lines.push(format!("{key}:"));
            lines.extend(String::from_utf8_lossy(&output.stdout).lines()
                .map(|line| format!("  {line}")));
            Ok(())
        }
        Ok(output) => {
            eprintln!("{key} unavailable: {}", String::from_utf8_lossy(&output.stderr).trim());
            field(lines, key, json!("unavailable"))
        }
        Err(error) => {
            eprintln!("{key} unavailable: {error}");
            field(lines, key, json!("unavailable"))
        }
    }
}

fn knowledge_fields(lines: &mut Vec<String>, home: &Path) -> Result<()> {
    field(lines, "knowledge-home", json!(home))?;
    command_output(lines, "knowledge-home-entries", Command::new("ls").arg("-1").arg(home))?;
    let quote = |path: &Path| format!("'{}'", path.display().to_string().replace('\'', "'\\''"));
    let fd = retrieval_tool(&["fd", "fdfind"]).unwrap_or_else(|| PathBuf::from("fd"));
    let command = format!(
        "{} -HI -t l --max-depth 1 . {} | sort | {} outline {} - | cut -f1,3",
        quote(&fd), quote(&home.join("trees")), quote(&env::current_exe()?), quote(home),
    );
    field(lines, "trees-command", json!(command))?;
    command_output(lines, "trees-output", Command::new("bash").args(["-o", "pipefail", "-c", &command]))
}

fn references(lines: &mut Vec<String>, key: &str, values: &BTreeSet<String>) -> Result<()> {
    if values.len() == 1 {
        field(lines, key, json!(values.first().unwrap()))?;
    } else if !values.is_empty() {
        let rows = values.iter().map(|value| vec![json!(value)]).collect::<Vec<_>>();
        lines.push(lean_render::render_table(&format!("{key}s"), &[key], &rows)?);
    }
    Ok(())
}

pub(super) fn render(home: &Path, entry: &Value) -> Result<String> {
    let home = std::fs::canonicalize(home)?;
    let mut lines = Vec::new();
    let mut actions = Vec::new();
    let mut guides = BTreeSet::new();
    let mut stopped = false;
    let mut transport = "unknown".to_owned();
    field(&mut lines, "onboarding-entry", entry["path"].clone())?;
    knowledge_fields(&mut lines, &home)?;
    match sync::instructions(&home) {
        Ok(receipts) => for receipt in receipts {
            if let Some(state) = receipt["transport"].as_str() { transport = state.to_owned(); }
            if receipt["status"] == "sync_failed" { stopped = true; }
            if let Some(guide) = receipt["repair_guide"].as_str() { guides.insert(guide.to_owned()); }
        },
        Err(error) => {
            stopped = true;
            eprintln!("Retained sync state unavailable: {error}");
        }
    }
    field(&mut lines, "hosted-sync", json!(if !guides.is_empty() {
        "needs-repair"
    } else if stopped {
        "stopped"
    } else {
        &transport
    }))?;
    clock_fields(&mut lines)?;
    retrieval_tool_fields(&mut lines)?;
    references(&mut lines, "repair-guide", &guides)?;
    if !guides.is_empty() {
        actions.push("Read repair-guide (each row if several). Resolve it for the user, and keep other work moving while you do. Only something genuinely significant goes to the user.".to_owned());
    }
    if stopped {
        actions.push(format!("Inspect `sifttext sync status '{}'`. Resolve the stopped sync for the user, and keep other work moving while you do. Only something genuinely significant goes to the user.",
            home.display().to_string().replace('\'', "'\\''")));
    }
    match updates::instruction_notice() {
        Ok(notice) if notice["status"] == "update_available" => {
            field(&mut lines, "update-available", notice["application"].clone())?;
            for key in ["release_name", "release_body", "release_url"] {
                if let Some(value) = notice.get(key) {
                    field(&mut lines, &format!("update-{}", key.replace('_', "-")), value.clone())?;
                }
            }
            field(&mut lines, "update-guide", json!(home.join("nodes").join(commands::UPDATE_GUIDE).join("node.md")))?;
            actions.push(if notice["permission"] == "ask" {
                "Read update-guide. When the user is next present, ask for permission to update SiftText."
            } else {
                "Read update-guide. Automatic updates are enabled. At a convenient pause, update SiftText and keep the user informed."
            }.to_owned());
        }
        Ok(notice) if notice["status"] == "needs_agent" => {
            field(&mut lines, "update-guide", json!(home.join("nodes").join(commands::UPDATE_GUIDE).join("node.md")))?;
            actions.push("Read update-guide. Inspect `sifttext update --local-status` for the retained update repair. Resolve it for the user, and keep other work moving while you do. Only something genuinely significant goes to the user.".to_owned());
        }
        Err(error) => eprintln!("SiftText update deferred: {error}"),
        _ => (),
    }
    match updates::harness_instruction_notice(&home) {
        Ok(notice) if notice["required"] == true => {
            field(&mut lines, "harness-instructions-refresh", json!("required"))?;
            field(&mut lines, "harness-instructions-guide", notice["guide"].clone())?;
            actions.push("Read harness-instructions-guide. Ensure its current SiftText block is first in your persistent instructions, following the guide. Keep the user informed and continue their task.".to_owned());
        }
        Err(error) => eprintln!("SiftText harness instructions unavailable: {error}"),
        _ => (),
    }
    if !actions.is_empty() { field(&mut lines, "agent-instructions", json!(actions.join(" ")))?; }
    lines.push("sifttext-help-output:".into());
    lines.extend(commands::index()?.lines().map(|line| format!("  {line}")));
    Ok(lines.join("\n"))
}
