//! Linux Cargo receipt checks; process/source/drain guards remain with the launcher.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

type Rows = BTreeMap<String, String>;

fn rows(log: &str, target: &str) -> Result<Rows, String> {
    let (mut active, mut started, mut closed) = (false, false, false);
    let mut rows = Rows::new();
    for line in log.lines() {
        let header = line.trim_start();
        if header.starts_with("Running ") || header.starts_with("Doc-tests ") {
            if active {
                return Err("Unfinished Cargo harness".into());
            }
            active = header.starts_with(&format!("Running {target} ("));
            if active {
                if started {
                    return Err("Duplicate Cargo harness".into());
                }
                started = true;
            }
        }
        if !active {
            continue;
        }
        if let Some((name, status)) = line
            .strip_prefix("test ")
            .and_then(|s| s.split_once(" ... "))
            && (name.is_empty()
                || name.contains(char::is_whitespace)
                || rows.insert(name.into(), status.into()).is_some())
        {
            return Err("Malformed or duplicate Cargo test row".into());
        }
        if line.starts_with("test result:") {
            let (passed, failed, ignored) =
                super::summary(line).ok_or("Failed or malformed Cargo summary")?;
            if failed != 0
                || rows
                    .values()
                    .any(|s| s != "ok" && s != "ignored" && !s.starts_with("ignored, "))
                || rows.values().filter(|s| *s == "ok").count() != passed
                || rows.len() != passed.checked_add(ignored).ok_or("Cargo count overflow")?
            {
                return Err("Cargo row/summary mismatch or failed tests".into());
            }
            active = false;
            closed = true;
        }
    }
    if !started || !closed || active {
        return Err("Missing terminal target-qualified Cargo harness".into());
    }
    Ok(rows)
}

fn listed(list: &str) -> Result<BTreeSet<String>, String> {
    let mut names = BTreeSet::new();
    for name in list.lines().filter_map(|s| s.strip_suffix(": test")) {
        if name.is_empty() || name.contains(char::is_whitespace) || !names.insert(name.into()) {
            return Err("Malformed or duplicate listed test".into());
        }
    }
    if names.is_empty() {
        return Err("No listed tests".into());
    }
    Ok(names)
}

fn selector(name: &str) -> Result<(), String> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b':'))
    {
        return Err("Unsafe Linux selector".into());
    }
    Ok(())
}

fn execution(mode: &str, name: &str, list: &str, log: &str) -> Result<String, String> {
    if mode == "linux-target" {
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return Err("Unsafe Linux target".into());
        }
    } else {
        selector(name)?;
    }
    let mut expected = listed(list)?;
    let target = if mode == "linux-target" {
        format!("tests/{name}.rs")
    } else {
        expected.retain(|s| {
            if mode == "linux-exact" {
                s == name
            } else {
                s.starts_with(name)
            }
        });
        "unittests src/lib.rs".into()
    };
    if expected.is_empty() {
        return Err("Selector lists no tests".into());
    }
    let actual = rows(log, &target)?;
    if actual.keys().cloned().collect::<BTreeSet<_>>() != expected {
        return Err("Listed and executed test identities differ".into());
    }
    let passed = actual.values().filter(|s| *s == "ok").count();
    let ignored = actual.len() - passed;
    if mode != "linux-target" && ignored != 0 {
        return Err("Required unit tests ignored".into());
    }
    Ok(if mode == "linux-target" {
        format!(
            "TARGET {name} listed={} passed={passed} ignored={ignored} failed=0",
            expected.len()
        )
    } else {
        format!("FAMILY {name} passed={passed} failed=0 ignored=0")
    })
}

fn required(name: &str, logs: &[String]) -> Result<String, String> {
    selector(name)?;
    let mut observed = BTreeSet::new();
    let mut closed_rows = BTreeSet::new();
    let matches = |test: &str| {
        test == name
            || test
                .strip_suffix(name)
                .is_some_and(|prefix| prefix.ends_with("::"))
    };
    for log in logs {
        for line in log.lines() {
            if let Some((test, _)) = line
                .strip_prefix("test ")
                .and_then(|s| s.split_once(" ... "))
                && matches(test)
            {
                observed.insert(line.to_owned());
            }
        }
        // Only closed harnesses can contribute required execution, not loose printed rows.
        let (mut target, mut block) = (String::new(), String::new());
        for line in log.lines() {
            let header = line.trim_start();
            if header.starts_with("Running ") || header.starts_with("Doc-tests ") {
                if block.lines().any(|s| {
                    s.strip_prefix("test ")
                        .and_then(|s| s.split_once(" ... "))
                        .is_some_and(|(test, _)| matches(test))
                }) && !target.is_empty()
                {
                    return Err("Unfinished required-control harness".into());
                }
                target.clear();
                if let Some(rest) = header
                    .strip_prefix("Running ")
                    .and_then(|s| s.split_once(" ("))
                {
                    target = rest.0.into();
                    block = format!("{header}\n");
                }
            } else if !target.is_empty() {
                block.push_str(line);
                block.push('\n');
                if line.starts_with("test result:") {
                    let actual = rows(&block, &target)?;
                    for (test, status) in actual {
                        if matches(&test) {
                            closed_rows.insert(format!("test {test} ... {status}"));
                        }
                    }
                    target.clear();
                }
            }
        }
        if !target.is_empty()
            && block.lines().any(|s| {
                s.strip_prefix("test ")
                    .and_then(|s| s.split_once(" ... "))
                    .is_some_and(|(test, _)| matches(test))
            })
        {
            return Err("Unfinished required-control harness".into());
        }
    }
    if observed.len() != 1
        || observed != closed_rows
        || !observed
            .iter()
            .next()
            .is_some_and(|s| s.ends_with(" ... ok"))
    {
        return Err(format!(
            "Required control {name} missing, ambiguous or not passing"
        ));
    }
    Ok(observed.into_iter().next().unwrap())
}

pub fn check(args: &[String]) -> Result<String, String> {
    match args {
        [mode, kind, name, path] if mode == "linux-count" && matches!(kind.as_str(), "exact" | "family" | "target") => {
            selector(&if kind == "target" { name.replace('-', "_") } else { name.clone() })?;
            let names = listed(&fs::read_to_string(path).map_err(|e| e.to_string())?)?;
            let count = names.iter().filter(|s| match kind.as_str() {
                "exact" => *s == name,
                "family" => s.starts_with(name),
                _ => true,
            }).count();
            if count == 0 { return Err("Selector lists no tests".into()); }
            Ok(count.to_string())
        }
        [mode, name, list, log] if matches!(mode.as_str(), "linux-target" | "linux-family" | "linux-exact") => {
            execution(mode, name, &fs::read_to_string(list).map_err(|e| e.to_string())?, &fs::read_to_string(log).map_err(|e| e.to_string())?)
        }
        [mode, name, paths @ ..] if mode == "linux-required" && !paths.is_empty() => {
            let logs = paths.iter().map(|p| fs::read_to_string(p).map_err(|e| e.to_string())).collect::<Result<Vec<_>, _>>()?;
            required(name, &logs)
        }
        _ => Err("Usage: acceptance-receipts linux-{target|family|exact} NAME LIST LOG | linux-count {target|family|exact} NAME LIST | linux-required NAME LOG...".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn log(target: &str) -> String {
        format!(
            "     Running {target} (target/debug/deps/check)\ntest resources::control ... ok\ntest resources::other ... ok\ntest result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n"
        )
    }
    #[test]
    fn linux_receipts_refuse_missing_ignored_failed_conflicting_and_unclosed_controls() {
        let list = "resources::control: test\nresources::other: test\n2 tests, 0 benchmarks\n";
        let unit = log("unittests src/lib.rs");
        let target = log("tests/native.rs");
        assert!(execution("linux-family", "resources::", list, &unit).is_ok());
        assert!(execution("linux-target", "native", list, &target).is_ok());
        let exact = unit
            .replace("test resources::other ... ok\n", "")
            .replace("2 passed", "1 passed");
        assert!(execution("linux-exact", "resources::control", list, &exact).is_ok());
        assert!(required("resources::control", &[unit.clone(), unit.clone()]).is_ok());
        let ignored = target
            .replace("other ... ok", "other ... ignored, optional")
            .replace(
                "2 passed; 0 failed; 0 ignored",
                "1 passed; 0 failed; 1 ignored",
            );
        assert!(execution("linux-target", "native", list, &ignored).is_ok());
        assert!(
            execution(
                "linux-target",
                "native",
                list,
                &ignored.replace("ignored, optional", "ignoredjunk")
            )
            .is_err()
        );
        for bad in [
            unit.replace("test resources::control ... ok\n", ""),
            unit.replace("control ... ok", "control ... ignored"),
            unit.replace("control ... ok", "control ... FAILED"),
            unit.replace("2 passed; 0 failed", "1 passed; 1 failed"),
            unit.replace("test result: ok.", "test result: FAILED."),
            unit.lines().take(3).collect::<Vec<_>>().join("\n"),
            unit.replace("src/lib.rs", "src/bin/other.rs"),
            unit.replace("control ... ok", "wrong ... ok"),
            unit.replace(
                "control ... ok\n",
                "control ... ok\ntest resources::control ... ok\n",
            ),
            unit.clone() + &unit,
        ] {
            assert!(
                execution("linux-family", "resources::", list, &bad).is_err(),
                "{bad}"
            );
        }
        for bad in [
            unit.replace("control", "wrong"),
            unit.replace("control ... ok", "control ... ignored")
                .replace(
                    "2 passed; 0 failed; 0 ignored",
                    "1 passed; 0 failed; 1 ignored",
                ),
            unit.replace("control ... ok", "control ... FAILED"),
            unit.clone() + "test resources::control ... FAILED\n",
            unit.clone() + &unit.lines().take(3).collect::<Vec<_>>().join("\n"),
        ] {
            assert!(required("resources::control", &[bad]).is_err());
        }
        let conflict = unit.replace("resources::control", "other::resources::control");
        assert!(required("resources::control", &[unit.clone(), conflict]).is_err());
        assert!(execution("linux-family", "resources.*", list, &unit).is_err());
        assert!(execution("linux-family", "missing::", list, &unit).is_err());
        assert!(
            execution(
                "linux-family",
                "resources::",
                &(list.to_owned() + list),
                &unit
            )
            .is_err()
        );
        assert!(required("control", &["test control ... ok\n".into()]).is_err());
        // Cargo --list headers are written to stderr without execution summaries.
        assert!(
            required(
                "resources::control",
                &[
                    unit,
                    "Running unittests src/lib.rs (target/debug/deps/list)\n".into()
                ]
            )
            .is_ok()
        );
    }
}
