//! Standalone guest terminal/drain verifier; compile before launching the admitted unit.
use std::{env, fs, path::Path, process::Command};
type Result<T> = std::result::Result<T, String>;
const CGROUP_ROOT: &str = "/sys/fs/cgroup";
const MOUNTINFO: &str = "/proc/self/mountinfo";

fn read(path: impl AsRef<Path>) -> Result<String> {
    fs::read_to_string(path.as_ref()).map_err(|e| format!("{}: {e}", path.as_ref().display()))
}
fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> Result<()> {
    fs::write(path.as_ref(), bytes).map_err(|e| format!("{}: {e}", path.as_ref().display()))
}
fn admit(
    mode: &str,
    control: &str,
    unit: &str,
    control_prefix: &str,
    unit_prefix: &str,
    saved: &str,
) -> Result<()> {
    let suffix = unit
        .strip_prefix(unit_prefix)
        .ok_or("Invalid unit identity")?;
    if !matches!(mode, "admit" | "stop" | "verify")
        || suffix.is_empty()
        || !suffix
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        || !control_prefix.starts_with('/')
        || unit_prefix.is_empty()
        || !unit
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        || control != format!("{control_prefix}{suffix}")
        || saved.trim_end_matches('\n') != unit
    {
        return Err("Control directory and saved unit must match the admitted identity".into());
    }
    Ok(())
}
fn property<'a>(text: &'a str, key: &str) -> Result<&'a str> {
    let prefix = format!("{key}=");
    let mut values = text.lines().filter_map(|line| line.strip_prefix(&prefix));
    let value = values.next().ok_or_else(|| format!("Missing {key}"))?;
    if values.next().is_some() {
        return Err(format!("Duplicate {key}"));
    }
    Ok(value)
}
fn inactive(text: &str, status: &str) -> Result<bool> {
    let status: u32 = status
        .trim_end_matches('\n')
        .parse()
        .map_err(|_| "Invalid systemctl status")?;
    if status != 0 && property(text, "LoadState")? != "not-found" {
        return Err("Failed systemctl show without explicit not-found state".into());
    }
    Ok(matches!(
        property(text, "ActiveState")?,
        "inactive" | "failed"
    ))
}
fn cgroup(text: &str, unit: &str) -> Result<String> {
    let mut groups = text.lines().filter_map(|line| line.strip_prefix("0::"));
    let group = groups.next().ok_or("Missing admitted unified cgroup")?;
    if groups.next().is_some() || group != format!("/system.slice/{unit}.service") {
        return Err("Cgroup differs from admitted unit".into());
    }
    Ok(group.to_owned())
}
fn drained(events: Option<&str>) -> Result<()> {
    if let Some(events) = events {
        let mut populated = events
            .lines()
            .filter_map(|line| line.strip_prefix("populated "));
        if populated.next() != Some("0") || populated.next().is_some() {
            return Err("Admitted cgroup is populated or has malformed drain evidence".into());
        }
    }
    Ok(())
}
fn observe_drain(root: &Path, group: &str, mounts: &str) -> Result<()> {
    // Missing groups prove absence only inside an observable cgroup-v2 mount.
    let mut matching = mounts.lines().filter_map(|line| {
        let (fields, filesystem) = line.split_once(" - ")?;
        (fields.split_whitespace().nth(4) == root.to_str()).then_some(filesystem)
    });
    if !matching.next().is_some_and(|fs| fs.starts_with("cgroup2 ")) || matching.next().is_some() {
        return Err("Cannot observe admitted cgroup-v2 mount".into());
    }
    read(root.join("cgroup.controllers"))?;
    let path = root.join(group.trim_start_matches('/'));
    let events = match fs::metadata(&path) {
        Ok(m) if m.is_dir() => Some(read(path.join("cgroup.events"))?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        _ => return Err("Cannot observe admitted cgroup directory".into()),
    };
    drained(events.as_deref())
}
fn evidence_path<'a>(script: &str, path: &'a str) -> Result<&'a Path> {
    let mut roots = script.lines().filter_map(|line| line.strip_prefix("Q="));
    let root = roots.next().ok_or("Missing producer evidence root")?;
    let path = path.trim_end_matches('\n');
    let p = Path::new(path);
    if roots.next().is_some()
        || !root.starts_with('/')
        || !path
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"/._-".contains(&c))
        || p.parent() != Some(Path::new(root))
        || !p.file_name().and_then(|s| s.to_str()).is_some_and(|s| {
            s.starts_with("identity-resume.") && s.len() > "identity-resume.".len()
        })
    {
        return Err("Guest evidence is outside the producer root".into());
    }
    Ok(p)
}
fn terminal(control: &Path, unit: &str) -> Result<bool> {
    let output = Command::new("sudo")
        .args([
            "-n",
            "systemctl",
            "show",
            &format!("{unit}.service"),
            "--property=LoadState",
            "--property=ActiveState",
            "--property=SubState",
            "--property=Result",
            "--property=ExecMainStatus",
            "--property=ControlGroup",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    write(control.join("unit.terminal"), &output.stdout)?;
    write(control.join("unit.terminal.stderr"), &output.stderr)?;
    let status = format!("{}\n", output.status.code().unwrap_or(1));
    write(control.join("unit.show.status"), &status)?;
    let text = String::from_utf8(output.stdout).map_err(|_| "Invalid terminal text")?;
    inactive(&text, &status)
}
fn run(args: &[String]) -> Result<()> {
    if args.len() != 5 {
        return Err(
            "Usage: linux-custody admit|verify|stop CONTROL UNIT CONTROL_PREFIX UNIT_PREFIX".into(),
        );
    }
    let (mode, c, unit, cp, up) = (&args[0], &args[1], &args[2], &args[3], &args[4]);
    // Validate the supplied boundary before reading files or invoking sudo.
    admit(mode, c, unit, cp, up, unit)?;
    let control = Path::new(c);
    admit(mode, c, unit, cp, up, &read(control.join("unit.name"))?)?;
    if mode == "admit" {
        cgroup(&read(control.join("unit.cgroup"))?, unit)?;
        return Ok(());
    }
    let mut stopped = terminal(control, unit)?;
    if mode == "stop" && !stopped {
        let output = Command::new("sudo")
            .args(["-n", "systemctl", "stop", &format!("{unit}.service")])
            .output()
            .map_err(|e| e.to_string())?;
        let mut log = output.stdout;
        log.extend(output.stderr);
        write(control.join("stop.log"), log)?;
        if !output.status.success() {
            return Err("systemctl stop failed".into());
        }
        stopped = terminal(control, unit)?;
    }
    if !stopped {
        return Err("Admitted unit is not inactive or failed".into());
    }
    let group = cgroup(&read(control.join("unit.cgroup"))?, unit)?;
    observe_drain(Path::new(CGROUP_ROOT), &group, &read(MOUNTINFO)?)?;
    write(
        control.join("drain.receipt"),
        format!("UNIT={unit} CGROUP={group} DRAINED\n"),
    )?;
    if mode == "verify" {
        let g = read(control.join("guest.evidence"))?;
        let script = read(control.join("guest.sh"))?;
        let path = evidence_path(&script, &g)?;
        if read(path.join("exit.status"))?.trim_end_matches('\n') != "0" {
            return Err("Guest qualification did not succeed".into());
        }
    }
    Ok(())
}
fn main() {
    if let Err(error) = run(&env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn real_dispatch_keeps_stop_terminal_and_evidence_refusals() {
        use std::os::unix::fs::PermissionsExt;
        let root = env::temp_dir().join(format!("sifttext-custody-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let bin = root.join("custody");
        let hierarchy = root.join("cgroup");
        fs::create_dir(&hierarchy).unwrap();
        write(hierarchy.join("cgroup.controllers"), "cpu memory\n").unwrap();
        let mounts = root.join("mountinfo");
        write(
            &mounts,
            format!(
                "35 29 0:30 / {} rw shared:8 - cgroup2 cgroup2 rw\n",
                hierarchy.display()
            ),
        )
        .unwrap();
        let mut source = read(file!()).unwrap();
        for (name, original, fixture) in [
            ("CGROUP_ROOT", "/sys/fs/cgroup", &hierarchy),
            ("MOUNTINFO", "/proc/self/mountinfo", &mounts),
        ] {
            let marker = format!("const {name}: &str = {original:?};");
            assert_eq!(source.matches(&marker).count(), 1);
            source = source.replacen(
                &marker,
                &format!("const {name}: &str = {:?};", fixture.to_str().unwrap()),
                1,
            );
        }
        let test_source = root.join("custody.rs");
        write(&test_source, source).unwrap();
        assert!(
            Command::new("rustc")
                .args(["--edition=2024", "-D", "warnings"])
                .arg(&test_source)
                .arg("-o")
                .arg(&bin)
                .status()
                .unwrap()
                .success()
        );
        let unit = format!("qualification-{}", std::process::id());
        let prefix = format!("{}/control-", root.display());
        let control = format!("{prefix}{}", std::process::id());
        fs::create_dir(&control).unwrap();
        let c = Path::new(&control);
        write(c.join("unit.name"), format!("{unit}\n")).unwrap();
        write(
            c.join("unit.cgroup"),
            format!("0::/system.slice/{unit}.service\n"),
        )
        .unwrap();
        let e = root.join("evidence/identity-resume.test");
        fs::create_dir_all(&e).unwrap();
        write(
            c.join("guest.sh"),
            format!("Q={}/evidence\n", root.display()),
        )
        .unwrap();
        write(c.join("guest.evidence"), format!("{}\n", e.display())).unwrap();
        write(e.join("exit.status"), "0\n").unwrap();
        write(
            root.join("terminal"),
            "LoadState=loaded\nActiveState=inactive\n",
        )
        .unwrap();
        write(root.join("show.status"), "0\n").unwrap();
        write(
            root.join("stopped"),
            "LoadState=loaded\nActiveState=inactive\n",
        )
        .unwrap();
        write(root.join("sudo"), b"#!/bin/sh\ncase $3 in\nshow) /bin/cat \"$MOCK_ROOT/terminal\"; echo retained-stderr >&2; exit \"$(/bin/cat \"$MOCK_ROOT/show.status\")\";;\nstop) /bin/cp \"$MOCK_ROOT/stopped\" \"$MOCK_ROOT/terminal\"; echo stopped;;\nesac\n").unwrap();
        fs::set_permissions(root.join("sudo"), fs::Permissions::from_mode(0o755)).unwrap();
        let call = |mode: &str| {
            Command::new(&bin)
                .args([mode, &control, &unit, &prefix, "qualification-"])
                .env("PATH", &root)
                .env("MOCK_ROOT", &root)
                .output()
                .unwrap()
        };
        assert!(call("admit").status.success());
        assert!(call("verify").status.success());
        assert_eq!(
            read(c.join("drain.receipt")).unwrap(),
            format!("UNIT={unit} CGROUP=/system.slice/{unit}.service DRAINED\n")
        );
        assert_eq!(
            read(c.join("unit.terminal.stderr")).unwrap(),
            "retained-stderr\n"
        );
        assert_eq!(read(c.join("unit.show.status")).unwrap(), "0\n");
        write(
            root.join("terminal"),
            "LoadState=loaded\nActiveState=active\n",
        )
        .unwrap();
        assert!(!call("verify").status.success());
        assert!(call("stop").status.success());
        assert_eq!(read(c.join("stop.log")).unwrap(), "stopped\n");
        write(root.join("show.status"), "1\n").unwrap();
        assert!(!call("verify").status.success());
        write(
            root.join("terminal"),
            "LoadState=not-found\nActiveState=inactive\n",
        )
        .unwrap();
        assert!(call("verify").status.success());
        write(e.join("exit.status"), "1\n").unwrap();
        assert!(!call("verify").status.success());
        write(e.join("exit.status"), "0\n").unwrap();
        write(c.join("unit.cgroup"), "0::/system.slice/another.service\n").unwrap();
        assert!(!call("admit").status.success());
        assert!(!call("stop").status.success());
        write(
            c.join("unit.cgroup"),
            format!("0::/system.slice/{unit}.service\n"),
        )
        .unwrap();
        write(
            c.join("guest.evidence"),
            "/unrelated/identity-resume.test\n",
        )
        .unwrap();
        let refusal = call("verify");
        assert!(!refusal.status.success());
        assert!(
            String::from_utf8(refusal.stderr)
                .unwrap()
                .contains("outside the producer root")
        );
        write(c.join("unit.name"), "another\n").unwrap();
        assert!(!call("stop").status.success());
        write(c.join("unit.name"), format!("{unit}\n")).unwrap();
        write(c.join("guest.evidence"), format!("{}\n", e.display())).unwrap();
        fs::remove_dir_all(&hierarchy).unwrap();
        assert!(!call("verify").status.success());
        assert!(!call("stop").status.success());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn missing_hierarchy_cannot_prove_group_absence() {
        let root = env::temp_dir().join(format!("sifttext-cgroup-root-{}", std::process::id()));
        let group = "/system.slice/qualification-test.service";
        let mounts = format!(
            "35 29 0:30 / {} rw shared:8 - cgroup2 cgroup2 rw\n",
            root.display()
        );
        assert!(observe_drain(&root, group, &mounts).is_err());
        fs::create_dir(&root).unwrap();
        assert!(observe_drain(&root, group, &mounts).is_err());
        write(root.join("cgroup.controllers"), "cpu memory\n").unwrap();
        for invalid in [
            String::new(),
            mounts.replace("cgroup2 ", "tmpfs "),
            mounts.replace(root.to_str().unwrap(), "/elsewhere"),
            mounts.clone() + &mounts,
        ] {
            assert!(observe_drain(&root, group, &invalid).is_err());
        }
        // The real Linux root has controllers, but no cgroup.events file.
        assert!(observe_drain(&root, group, &mounts).is_ok());
        let path = root.join(group.trim_start_matches('/'));
        fs::create_dir_all(&path).unwrap();
        assert!(observe_drain(&root, group, &mounts).is_err());
        write(path.join("cgroup.events"), "populated 1\n").unwrap();
        assert!(observe_drain(&root, group, &mounts).is_err());
        write(path.join("cgroup.events"), "populated 0\n").unwrap();
        assert!(observe_drain(&root, group, &mounts).is_ok());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn custody_requires_admitted_terminal_drain_and_producer_success() {
        let u = "qualification-test";
        let c = "/tmp/control-test";
        assert!(
            admit(
                "verify",
                c,
                u,
                "/tmp/control-",
                "qualification-",
                &format!("{u}\n")
            )
            .is_ok()
        );
        for (mode, control, unit, saved) in [
            ("other", c, u, u),
            ("verify", "/tmp/control", u, u),
            ("verify", c, u, "another"),
            ("verify", c, "--all", "--all"),
        ] {
            assert!(
                admit(
                    mode,
                    control,
                    unit,
                    "/tmp/control-",
                    "qualification-",
                    saved
                )
                .is_err()
            );
        }
        let t = "LoadState=loaded\nActiveState=inactive\n";
        assert_eq!(inactive(t, "0\n"), Ok(true));
        assert_eq!(inactive(&t.replace("inactive", "active"), "0"), Ok(false));
        assert_eq!(inactive(&t.replace("inactive", "failed"), "0"), Ok(true));
        assert_eq!(inactive(&t.replace("loaded", "not-found"), "1"), Ok(true));
        for (text, status) in [
            (t, "1"),
            (t, "invalid"),
            ("ActiveState=inactive\nActiveState=active\n", "0"),
            ("LoadState=loaded\n", "0"),
        ] {
            assert!(inactive(text, status).is_err());
        }
        let cg = format!("0::/system.slice/{u}.service\n");
        assert!(cgroup(&cg, u).is_ok());
        for text in [
            cg.replace(u, "another"),
            cg.clone() + &cg,
            cg.replace("0::", "0:cpu:"),
        ] {
            assert!(cgroup(&text, u).is_err());
        }
        assert!(drained(None).is_ok());
        assert!(drained(Some("populated 0\nfrozen 0\n")).is_ok());
        for events in ["populated 1\n", "", "populated 0\npopulated 1\n"] {
            assert!(drained(Some(events)).is_err());
        }
        let script = "Q=/tmp/evidence\n";
        let g = "/tmp/evidence/identity-resume.test\n";
        assert!(evidence_path(script, g).is_ok());
        for (s, path) in [
            ("", g),
            (script, "/tmp/other/identity-resume.test"),
            (script, "/tmp/evidence/identity-resume.test/../else"),
            (script, "/tmp/evidence/identity-resume."),
        ] {
            assert!(evidence_path(s, path).is_err());
        }
        assert!(evidence_path(&(script.to_owned() + script), g).is_err());
    }
}
