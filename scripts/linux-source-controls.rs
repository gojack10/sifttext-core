//! Source-only Linux selector preflight. Run before bundling, guest access or builds.
use std::{collections::BTreeSet, env, fs, path::Path, process::Command};

fn git(src: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(src)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("Git source acquisition failed: {}", args.join(" ")));
    }
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
}
fn safe(name: &str, extra: &[u8]) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || extra.contains(&b))
}
fn check(src: &Path, rev: &str, targets: &str, required: &str) -> Result<(), String> {
    if rev.len() != 40
        || !rev
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Expected full lowercase source revision".into());
    }
    git(src, &["cat-file", "-e", &format!("{rev}^{{commit}}")])?;
    let targets: Vec<_> = targets.split_whitespace().collect();
    for target in &targets {
        if !safe(target, b"_-") {
            return Err("Unsafe target selector".into());
        }
        git(
            src,
            &["cat-file", "-e", &format!("{rev}:tests/{target}.rs")],
        )?;
    }
    for name in required.split_whitespace() {
        if !safe(name, b"_:") {
            return Err("Unsafe required selector".into());
        }
        let leaf = name.rsplit(':').next().unwrap();
        // ponytail: declaration-presence preflight, not a Rust parser; compiled
        // list/target-qualified execution receipts remain the reachability check.
        let paths = git(
            src,
            &[
                "grep",
                "-l",
                "-F",
                &format!("fn {leaf}("),
                rev,
                "--",
                "*.rs",
            ],
        )
        .map_err(|_| format!("Required control {name} is not defined in {rev}"))?;
        if paths.is_empty() {
            return Err(format!("Missing required control {name}"));
        }
        let mut found = false;
        for row in paths.lines() {
            let path = row
                .strip_prefix(&format!("{rev}:"))
                .ok_or("Malformed Git source locator")?;
            let body = git(src, &["show", row])?;
            if !body.lines().any(|line| {
                let line = line.trim_start();
                line.starts_with(&format!("fn {leaf}("))
                    || line.starts_with(&format!("pub fn {leaf}("))
                    || line.starts_with(&format!("async fn {leaf}("))
            }) {
                continue;
            }
            found = true;
            if let Some(target) = path
                .strip_prefix("tests/")
                .and_then(|s| s.strip_suffix(".rs"))
                && !target.contains('/')
                && !targets.contains(&target)
            {
                return Err(format!(
                    "Required control {name} lives in unscheduled tests/{target}.rs"
                ));
            }
        }
        if !found {
            return Err(format!("Missing required declaration {name}"));
        }
    }
    Ok(())
}
fn exact(src: &Path, rev: &str, family: &str, required: &str) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for name in family.split_whitespace() {
        if !safe(name, b"_:") || !name.contains("::") || name.ends_with("::") {
            return Err("Publication selector must be a complete unit name".into());
        }
        if !seen.insert(name) {
            return Err("Duplicate publication selector".into());
        }
        check(src, rev, "", name)?;
    }
    for name in required.split_whitespace() {
        if seen
            .iter()
            .filter(|s| s.rsplit("::").next() == Some(name))
            .count()
            != 1
        {
            return Err(format!(
                "Required publication control {name} must have exactly one scheduled selector"
            ));
        }
    }
    Ok(())
}
fn receipt_modes(text: &str) -> Result<BTreeSet<String>, String> {
    let mut modes = BTreeSet::new();
    // ponytail: only the recipe's one-line dispatch forms, not shell reachability.
    // Keep new forms explicit and independently checked when transport evolves.
    for line in text.lines().map(str::trim) {
        let line = line.strip_prefix("FC=$(").unwrap_or(line);
        let mut words = line.split_whitespace();
        if !matches!(
            words.next(),
            Some(
                "\"$CHECK\""
                    | "\"$SRC/target/debug/acceptance-receipts\""
                    | "target/debug/acceptance-receipts"
                    | "\"$TS/debug/acceptance-receipts\""
            )
        ) {
            continue;
        }
        let mode = words
            .next()
            .ok_or("Receipt dispatch has no mode")?
            .trim_matches('"');
        if !mode.starts_with("linux-") || !safe(mode, b"-") {
            return Err("Unsafe receipt dispatch mode".into());
        }
        modes.insert(mode.into());
    }
    Ok(modes)
}
fn stage(src: &Path, rev: &str, name: &str, dir: &Path) -> Result<(), String> {
    let read = |path: &str| {
        fs::read_to_string(dir.join(path)).map_err(|e| format!("Cannot read {path}: {e}"))
    };
    match name {
        "publication" => {
            let family = read("publication-selectors.txt")?;
            if family.split_whitespace().count() != 41 {
                return Err("Publication must schedule 41 unique unit selectors".into());
            }
            check(src, rev, "", &read("publication-required.txt")?)?;
            exact(src, rev, &family, &read("publication-required.txt")?)?;
        }
        "historical" => {
            let required = read("historical-required.txt")?;
            if required.split_whitespace().count() != 30 {
                return Err("Historical must require 30 controls".into());
            }
            check(src, rev, &read("historical-targets.txt")?, &required)?;
        }
        "package" => check(src, rev, "", "")?,
        _ => return Err("Unknown qualification stage".into()),
    }
    let mut modes = receipt_modes(&read("guest.sh")?)?;
    if modes.len() < 4 {
        return Err(format!(
            "Guest dispatches only {} receipt modes",
            modes.len()
        ));
    }
    for path in ["package-stage.sh", "isolation-stage.sh"] {
        modes.extend(receipt_modes(&read(path)?)?);
    }
    for mode in &modes {
        git(
            src,
            &[
                "grep",
                "-q",
                "-F",
                &format!("\"{mode}\""),
                rev,
                "--",
                "src/bin",
            ],
        )
        .map_err(|_| format!("Receipt mode {mode} is not implemented in {rev}"))?;
    }
    println!("{}", modes.len());
    Ok(())
}
fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    let result = match args.as_slice() {
        [src, rev, mode, name, dir] if mode == "stage" => stage(Path::new(src), rev, name, Path::new(dir)),
        [src, rev, targets, required] => check(Path::new(src), rev, targets, required),
        [src, rev, targets, required, mode, family] if mode == "exact" => {
            check(Path::new(src), rev, targets, required)
                .and_then(|()| exact(Path::new(src), rev, family, required))
        }
        _ => Err("Usage: linux-source-controls SOURCE REV TARGETS REQUIRED [exact FAMILY] | SOURCE REV stage STAGE PAYLOAD-DIR".into()),
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stage_modes_require_actual_supported_one_line_dispatches() {
        let comment = "# linux-count linux-exact linux-target linux-required\n";
        assert!(receipt_modes(comment).unwrap().is_empty());
        let forms = concat!(
            "  \"$CHECK\" linux-count exact x y\n",
            "  FC=$(\"$CHECK\" linux-count family x y)\n",
            "  \"$CHECK\" linux-exact x y z\n",
            "\"$SRC/target/debug/acceptance-receipts\" linux-package x y\n",
            "target/debug/acceptance-receipts linux-isolation-before x y\n",
            "\"$TS/debug/acceptance-receipts\" linux-isolation-hidden x y\n"
        );
        assert_eq!(
            receipt_modes(forms).unwrap(),
            [
                "linux-count",
                "linux-exact",
                "linux-package",
                "linux-isolation-before",
                "linux-isolation-hidden"
            ]
            .map(String::from)
            .into()
        );
        for invalid in [
            "\"$CHECK\"",
            "\"$CHECK\" linux-foo;true x",
            "\"$CHECK\" other x",
        ] {
            assert!(receipt_modes(invalid).is_err());
        }
    }
    #[test]
    fn preflight_reads_selected_commit_and_refuses_missing_unsafe_or_unscheduled_controls() {
        let dir = env::temp_dir().join(format!("sifttext-linux-preflight-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        git(&dir, &["init", "--quiet"]).unwrap();
        std::fs::create_dir(dir.join("tests")).unwrap();
        std::fs::create_dir(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("tests/native.rs"),
            "#[test]\nfn integration() {}\n",
        )
        .unwrap();
        std::fs::write(dir.join("src/lib.rs"), "#[test]\nfn unit() {}\n").unwrap();
        std::fs::create_dir(dir.join("src/bin")).unwrap();
        std::fs::write(
            dir.join("src/bin/modes.rs"),
            "[\"linux-count\", \"linux-exact\", \"linux-target\", \"linux-required\"]",
        )
        .unwrap();
        git(&dir, &["add", "."]).unwrap();
        git(
            &dir,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                "fixture",
            ],
        )
        .unwrap();
        let rev = git(&dir, &["rev-parse", "HEAD"]).unwrap();
        let rev = rev.trim();
        assert!(check(&dir, rev, "native", "mod::unit integration").is_ok());
        std::fs::write(
            dir.join("tests/native.rs"),
            "working tree must not replace selected commit",
        )
        .unwrap();
        assert!(check(&dir, rev, "native", "integration").is_ok());
        for (r, targets, names) in [
            (rev, "", "integration"),
            (rev, "missing", "unit"),
            (rev, "native", "absent"),
            (rev, "native", "unit.*"),
            (rev, "native;echo", "unit"),
            ("HEAD", "native", "unit"),
            ("0000000000000000000000000000000000000000", "native", "unit"),
        ] {
            assert!(
                check(&dir, r, targets, names).is_err(),
                "{r} {targets} {names}"
            );
        }
        assert!(exact(&dir, rev, "mod::unit", "unit").is_ok());
        for (family, required) in [
            ("unit", "unit"),
            ("mod::", "unit"),
            ("mod::absent", "absent"),
            ("mod::unit mod::unit", "unit"),
            ("mod::unit other::unit", "unit"),
            ("mod::unit", "absent"),
            ("mod;::unit", "unit"),
        ] {
            assert!(
                exact(&dir, rev, family, required).is_err(),
                "{family} {required}"
            );
        }
        let payload = dir.join("payload");
        fs::create_dir(&payload).unwrap();
        let dispatch = "\"$CHECK\" linux-count x\n\"$CHECK\" linux-exact x\n\"$CHECK\" linux-target x\n\"$CHECK\" linux-required x\n";
        fs::write(payload.join("guest.sh"), dispatch).unwrap();
        for path in ["package-stage.sh", "isolation-stage.sh"] {
            fs::write(payload.join(path), "# no additional receipt modes\n").unwrap();
        }
        assert!(stage(&dir, rev, "package", &payload).is_ok());
        fs::write(
            dir.join("src/bin/modes.rs"),
            "working bytes are not selected source",
        )
        .unwrap();
        assert!(stage(&dir, rev, "package", &payload).is_ok());
        for text in [
            "# linux-count linux-exact linux-target linux-required\n",
            "echo \"$CHECK\" linux-count linux-exact linux-target linux-required\n",
            &(dispatch.to_owned() + "\"$CHECK\" linux-absent x\n"),
        ] {
            fs::write(payload.join("guest.sh"), text).unwrap();
            assert!(stage(&dir, rev, "package", &payload).is_err(), "{text}");
        }
        fs::write(payload.join("guest.sh"), dispatch).unwrap();
        fs::write(
            payload.join("publication-selectors.txt"),
            "mod::unit\n".repeat(40),
        )
        .unwrap();
        fs::write(payload.join("historical-required.txt"), "unit\n".repeat(29)).unwrap();
        assert!(stage(&dir, rev, "publication", &payload).is_err());
        assert!(stage(&dir, rev, "historical", &payload).is_err());
        assert!(stage(&dir, rev, "unknown", &payload).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
