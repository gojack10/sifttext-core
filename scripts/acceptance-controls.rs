//! Source-only preflight and target-qualified execution checks. No native work.
use std::{env, fs, path::Path};
#[path = "cargo-portable-framing.rs"]
mod portable_framing;

fn read(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("Cannot read {path}: {error}"))
}

fn declaration(source: &str, control: &str, package: bool) -> bool {
    let (mut test, mut ignored, mut found, mut bad) = (false, false, 0, false);
    // ponytail: formatted declarations, not a Rust parser; compilation/execution
    // remains authoritative for cfg/macro reachability.
    for line in source.lines().map(str::trim) {
        if line == "#[test]" {
            test = true;
            continue;
        }
        if line
            .strip_prefix("#[ignore")
            .is_some_and(|rest| rest.starts_with([' ', '=', ']']))
        {
            ignored = true;
            continue;
        }
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if line == format!("fn {control}() {{") {
            found += 1;
            bad |= !test || package != ignored;
        }
        test = false;
        ignored = false;
    }
    found == 1 && !bad
}

fn executed(log: &str, target: &str, selector: &str, package: bool) -> bool {
    let normalized;
    let log = if package {
        let Ok(text) = portable_framing::normalize(log, selector) else {
            return false;
        };
        normalized = text;
        &normalized
    } else {
        log
    };
    let (mut active, mut seen, mut passed, mut bad) = (false, 0, 0, false);
    for line in log.lines() {
        let words: Vec<_> = line.split_whitespace().collect();
        if matches!(words.first(), Some(&"Running" | &"Doc-tests")) {
            bad |= active; // An unfinished harness cannot be hidden by a later one.
            active = words.first() == Some(&"Running")
                && words.get(1) == Some(&format!("tests/{target}.rs").as_str());
            seen = 0;
        }
        if active && line == format!("test {selector} ... ok") {
            seen += 1;
        }
        if active && line.starts_with("test result:") {
            let success = line.strip_prefix("test result: ok. ").is_some_and(|rest| {
                rest.split_once(" passed; 0 failed;")
                    .is_some_and(|(count, _)| {
                        !count.starts_with('0')
                            && !count.is_empty()
                            && count.bytes().all(|byte| byte.is_ascii_digit())
                    })
            });
            bad |= !success;
            if package {
                bad |= !line.starts_with("test result: ok. 1 passed; 0 failed; 0 ignored;");
            }
            passed += seen;
            active = false;
        }
    }
    passed == 1 && !bad && !active
}

fn check(args: &[String]) -> Result<(), String> {
    let (stage, log) = match args {
        [] => ("", String::new()),
        [stage, path] if matches!(stage.as_str(), "tests" | "package") => {
            if !Path::new(path).is_file() {
                return Err(format!("Missing execution log: {path}"));
            }
            (stage.as_str(), read(path)?)
        }
        [_, _] => return Err("Unknown log stage".into()),
        _ => return Err("Usage: check-acceptance-controls.sh [tests|package LOG]".into()),
    };
    let inventory = read("scripts/acceptance-controls.tsv")?;
    let (mut rows, mut executed_rows) = (0, 0);
    for line in inventory.lines() {
        let row: Vec<_> = line.split_whitespace().collect();
        if row.is_empty() || row[0].starts_with('#') {
            continue;
        }
        if !matches!(row[0], "tests" | "package") {
            return Err(format!("Unknown acceptance stage: {}", row[0]));
        }
        let [row_stage, target, source, control] = row.as_slice() else {
            return Err("Malformed acceptance control row".into());
        };
        if !target
            .bytes()
            .chain(control.bytes())
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b':'))
        {
            return Err("Unsafe acceptance selector".into());
        }
        let target_source = format!("tests/{target}.rs");
        if *source != target_source && *source != "tests/outline_oracle/capacity.rs" {
            return Err(format!("Unexpected control source: {source}"));
        }
        if !Path::new(&target_source).is_file() || !Path::new(source).is_file() {
            return Err(format!("Missing acceptance source: {target} {source}"));
        }
        let included = *source != target_source;
        if included
            && (*target != "outline_file_oracle"
                || !read(&target_source)?
                    .lines()
                    .any(|line| line == "include!(\"outline_oracle/capacity.rs\");"))
        {
            return Err(format!(
                "Control source is not included by target: {target} {source}"
            ));
        }
        if !declaration(&read(source)?, control, *row_stage == "package") {
            return Err(format!(
                "Missing, duplicate or wrong-stage acceptance control: {row_stage} {target} {control}"
            ));
        }
        if *row_stage == stage {
            let selector = if included {
                format!("capacity::{control}")
            } else {
                (*control).into()
            };
            if !executed(&log, target, &selector, stage == "package") {
                return Err(format!(
                    "Required control did not execute once in its stage/target: {row_stage} {target} {control}"
                ));
            }
            executed_rows += 1;
        }
        rows += 1;
    }
    if rows == 0 {
        return Err("Empty acceptance control inventory".into());
    }
    println!("Acceptance source selectors checked: {rows} (execution still required)");
    if !stage.is_empty() {
        if executed_rows == 0 {
            return Err(format!("Empty execution stage: {stage}"));
        }
        println!("Required {stage} controls executed once: {executed_rows}");
    }
    Ok(())
}

fn main() {
    if let Err(error) = check(&env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
