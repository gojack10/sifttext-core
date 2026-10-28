//! Native user-session scheduling; no shell wrapper, resident daemon or growing log.
use super::*;
use sha2::{Digest, Sha256};

fn command(program: &str, args: &[&str]) -> Result<()> {
    let output = bounded({
        let mut c = Command::new(program);
        c.args(args);
        c
    })?;
    if !output.status.success() {
        return Err(format!("{program} could not update the user sync schedule; inspect sync status, setup remains incomplete").into());
    }
    Ok(())
}
fn name(home: &Path) -> String {
    format!(
        "com.sifttext.sync.{}",
        &format!("{:x}", Sha256::digest(home.as_os_str().as_encoded_bytes()))[..20]
    )
}
fn paths(home: &Path) -> Result<(String, PathBuf, Option<PathBuf>)> {
    let user = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?);
    let name = name(home);
    if cfg!(target_os = "macos") {
        Ok((
            name.clone(),
            user.join("Library/LaunchAgents")
                .join(format!("{name}.plist")),
            None,
        ))
    } else if cfg!(target_os = "linux") {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| user.join(".config"));
        if !config.is_absolute() {
            return Err("XDG_CONFIG_HOME must be absolute".into());
        }
        let dir = config.join("systemd/user");
        Ok((
            name.clone(),
            dir.join(format!("{name}.service")),
            Some(dir.join(format!("{name}.timer"))),
        ))
    } else {
        Err("native sync scheduling supports macOS launchd and Linux systemd-user only".into())
    }
}
fn text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| "runner path must be UTF-8".into())
}
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn unit(value: &str) -> Result<String> {
    if value.chars().any(char::is_control) {
        return Err("control character in runner path".into());
    }
    Ok(format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
            .replace('$', "$$")
    ))
}
fn launcher() -> Result<PathBuf> {
    let executable = if let Some(root) = std::env::var_os("SIFTTEXT_INSTALL_ROOT") {
        let path = PathBuf::from(root).join("sifttext");
        // Managed installations keep a stable launcher across release activation.
        if path.is_file() {
            path
        } else {
            std::env::current_exe()?
        }
    } else {
        std::env::current_exe()?
    };
    Ok(executable)
}
fn write_changed(path: &Path, bytes: &[u8]) -> Result<bool> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        if !meta.is_file() || meta.nlink() != 1 || meta.uid() != unsafe { libc::geteuid() } {
            return Err("unsafe runner definition".into());
        }
        if fs::read(path)? == bytes {
            return Ok(false);
        }
    }
    fs::create_dir_all(path.parent().ok_or("missing runner directory")?)?;
    atomic(path, bytes)?;
    Ok(true)
}
fn launchd_target(label: &str) -> String {
    format!("gui/{}/{label}", unsafe { libc::geteuid() })
}
fn active(label: &str, timer: Option<&Path>) -> bool {
    let mut c = if cfg!(target_os = "macos") {
        let mut c = Command::new("launchctl");
        c.args(["print", &launchd_target(label)]);
        c
    } else {
        let mut c = Command::new("systemctl");
        c.args([
            "--user",
            "is-active",
            "--quiet",
            timer
                .and_then(|p| p.file_name())
                .and_then(|p| p.to_str())
                .unwrap_or(""),
        ]);
        c
    };
    c.stdin(Stdio::null());
    bounded(c).is_ok_and(|o| o.status.success())
}
pub(super) fn status(home: &Path) -> Value {
    match paths(home) {
        Ok((label, service, timer)) => {
            json!({"kind":if timer.is_some() {"systemd-user"} else {"launchd"},"definition":service,"timer":timer,"installed":service.is_file(),"scheduled":active(&label, timer.as_deref()),"interval_seconds":15,"user_session_only":true})
        }
        Err(error) => json!({"scheduled":false,"error":error.to_string()}),
    }
}
pub(super) fn install(home: &Path) -> Result<()> {
    let (label, service, timer) = paths(home)?;
    let launcher = launcher()?;
    let home_text = text(home)?;
    let launcher_text = text(&launcher)?;
    if launcher_text
        .chars()
        .chain(home_text.chars())
        .any(char::is_control)
    {
        return Err("control character in runner path".into());
    }
    if let Some(timer) = &timer {
        let definition = format!(
            "[Unit]\nDescription=SiftText knowledge sync\n[Service]\nType=oneshot\nExecStart={} sync tick {}\nEnvironment=SIFTTEXT_NO_UPDATE=1\nStandardOutput=null\nStandardError=null\n",
            unit(launcher_text)?,
            unit(home_text)?
        );
        let clock = "[Unit]\nDescription=SiftText knowledge sync schedule\n[Timer]\nOnStartupSec=15s\nOnUnitInactiveSec=15s\nAccuracySec=1s\n[Install]\nWantedBy=timers.target\n";
        let changed = write_changed(&service, definition.as_bytes())?
            | write_changed(timer, clock.as_bytes())?;
        if changed {
            command("systemctl", &["--user", "daemon-reload"])?;
        }
        if changed || !active(&label, Some(timer)) {
            command(
                "systemctl",
                &[
                    "--user",
                    "enable",
                    "--now",
                    text(
                        timer
                            .file_name()
                            .map(Path::new)
                            .ok_or("missing timer name")?,
                    )?,
                ],
            )?;
        }
    } else {
        let definition = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>{label}</string><key>ProgramArguments</key><array><string>{}</string><string>sync</string><string>tick</string><string>{}</string></array><key>StartInterval</key><integer>15</integer><key>EnvironmentVariables</key><dict><key>SIFTTEXT_NO_UPDATE</key><string>1</string></dict><key>StandardOutPath</key><string>/dev/null</string><key>StandardErrorPath</key><string>/dev/null</string></dict></plist>\n",
            xml(launcher_text),
            xml(home_text)
        );
        let changed = write_changed(&service, definition.as_bytes())?;
        let running = active(&label, None);
        if changed && running {
            command("launchctl", &["bootout", &launchd_target(&label)])?;
        }
        if changed || !running {
            command(
                "launchctl",
                &[
                    "bootstrap",
                    &format!("gui/{}", unsafe { libc::geteuid() }),
                    text(&service)?,
                ],
            )?;
        }
    }
    Ok(())
}
pub(super) fn disable(home: &Path) -> Result<()> {
    let (label, service, timer) = paths(home)?;
    if let Some(timer) = &timer {
        if timer.try_exists()? {
            command(
                "systemctl",
                &[
                    "--user",
                    "disable",
                    "--now",
                    text(
                        timer
                            .file_name()
                            .map(Path::new)
                            .ok_or("missing timer name")?,
                    )?,
                ],
            )?;
        }
    } else if active(&label, None) {
        command("launchctl", &["bootout", &launchd_target(&label)])?;
    }
    for path in [Some(service), timer].into_iter().flatten() {
        if let Ok(meta) = fs::symlink_metadata(&path) {
            if !meta.is_file() || meta.nlink() != 1 || meta.uid() != unsafe { libc::geteuid() } {
                return Err("unsafe runner definition retained".into());
            }
            fs::remove_file(path)?;
        }
    }
    if cfg!(target_os = "linux") {
        command("systemctl", &["--user", "daemon-reload"])?;
    }
    Ok(())
}
pub(super) fn record(store: &Store, error: Option<&str>) -> Result<()> {
    let path = store.root.join("runner-error.json");
    if let Some(error) = error {
        // Retain one bounded failure, without a per-tick timestamp/write on repeats.
        let summary: String = error.chars().take(2048).collect();
        let value = serde_json::to_vec(
            &json!({"status":"failed","error":summary,"retry":"next enabled scheduled tick; inspect sync status for scheduling and cause before relying on hosted freshness"}),
        )?;
        if fs::read(&path).ok().as_deref() != Some(value.as_slice()) {
            atomic(&path, &value)?;
        }
    } else if path.try_exists()? {
        fs::remove_file(path)?;
        fs::File::open(&store.root)?.sync_all()?;
    }
    Ok(())
}
pub(super) fn error(store: &Store) -> Result<Value> {
    let path = store.root.join("runner-error.json");
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Null),
        Err(e) => Err(e.into()),
        Ok(m) if m.is_file() && m.nlink() == 1 && m.len() < 16 * 1024 => {
            Ok(serde_json::from_slice(&fs::read(path)?)?)
        }
        Ok(_) => Err("unsafe runner diagnostic".into()),
    }
}
