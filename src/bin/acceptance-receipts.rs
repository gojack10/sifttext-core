//! Standalone, stdlib-only checker for the worker's Cargo execution receipts.
use std::{collections::BTreeSet, env, fs};
#[path = "acceptance_receipts/isolation.rs"]
mod isolation;
#[path = "acceptance_receipts/linux.rs"]
mod linux;
#[path = "acceptance_receipts/package.rs"]
mod package;
#[path = "../../scripts/cargo-portable-framing.rs"]
mod portable_framing;

const FAMILIES: [&str; 7] = [
    "lib:token_metadata_tests",
    "lib:status_tests",
    "lib:index_tests",
    "lib:neighborhood_tests",
    "lib:search_tests",
    "lib:outlines_tests",
    "lean:rendered_outline",
];
const PORTABLE: &str = "relocated_runtime_refuses_missing_and_tampered_assets";

fn summary(line: &str) -> Option<(usize, usize, usize)> {
    let rest = line.strip_prefix("test result: ok. ")?;
    let mut fields = rest.split(';');
    let mut count = |suffix| fields.next()?.trim().strip_suffix(suffix)?.parse().ok();
    Some((count(" passed")?, count(" failed")?, count(" ignored")?))
}

fn check(mode: &str, text: &str) -> Result<(), String> {
    let normalized;
    let text = if mode == "portable" {
        normalized = portable_framing::normalize(text, PORTABLE)?;
        &normalized
    } else {
        text
    };
    if !matches!(mode, "frontier" | "portable") {
        return Err("Usage: acceptance-receipts frontier|portable LOG".into());
    }
    let mut target = None;
    let mut seen = BTreeSet::new();
    let mut passed = BTreeSet::new();
    let mut portable_count = 0;
    for line in text.lines() {
        let header = line.trim_start();
        if header.starts_with("Running ") || header.starts_with("Doc-tests ") {
            if target.is_some() {
                return Err("Required Cargo harness has no terminal summary".into());
            }
            target = match (mode, header) {
                ("frontier", h) if h.starts_with("Running unittests src/lib.rs (") => Some("lib"),
                ("frontier", h)
                    if h.starts_with("Running unittests src/bin/lean-render-observe.rs (") =>
                {
                    Some("lean")
                }
                ("portable", h) if h.starts_with("Running tests/portable_runtime.rs (") => {
                    Some("portable")
                }
                _ => None,
            };
            seen.clear();
        }
        let Some(current) = target else { continue };
        if let Some(name) = line
            .strip_prefix("test ")
            .and_then(|s| s.strip_suffix(" ... ok"))
            .filter(|s| !s.contains(char::is_whitespace))
            && !seen.insert(name.to_owned())
            && mode == "portable"
        {
            return Err("Required portable control did not execute exactly once".into());
        }
        if line.starts_with("test result:") {
            let (count, failed, ignored) = summary(line).ok_or_else(|| {
                "Required Cargo harness has a failed or malformed summary".to_owned()
            })?;
            if count == 0 || failed != 0 {
                return Err("Required Cargo harness did not execute successfully".into());
            }
            if mode == "portable" {
                if count != 1
                    || ignored != 0
                    || !line.contains("; 0 measured; 0 filtered out;")
                    || seen.len() != 1
                    || !seen.contains(PORTABLE)
                {
                    return Err("Required portable control did not execute exactly once".into());
                }
                portable_count += 1;
            } else {
                for name in &seen {
                    if let Some((module, _)) = name.split_once("::") {
                        passed.insert(format!("{current}:{module}"));
                    }
                }
            }
            target = None;
        }
    }
    if target.is_some() {
        return Err("Required Cargo harness has no terminal summary".into());
    }
    if mode == "portable" {
        if portable_count != 1 {
            return Err("Required portable control did not execute exactly once".into());
        }
    } else {
        for family in FAMILIES {
            if !passed.contains(family) {
                return Err(format!(
                    "Required Rust frontier family did not execute successfully: {family}"
                ));
            }
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    let result = if args
        .first()
        .is_some_and(|s| s.starts_with("linux-isolation-"))
    {
        isolation::run(&args).map(|output| println!("{output}"))
    } else if args.first().is_some_and(|s| {
        matches!(
            s.as_str(),
            "linux-package" | "linux-package-list" | "linux-runtime"
        )
    }) {
        package::run(&args).map(|output| println!("{output}"))
    } else if args.first().is_some_and(|s| s.starts_with("linux-")) {
        linux::check(&args).map(|output| println!("{output}"))
    } else if args.len() == 2 {
        fs::read_to_string(&args[1])
            .map_err(|e| format!("Cannot read execution log: {e}"))
            .and_then(|text| check(&args[0], &text))
    } else {
        Err("Usage: acceptance-receipts frontier|portable LOG".into())
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frontier() -> String {
        let mut log = "     Running unittests src/lib.rs (target/debug/deps/lib)\n".to_owned();
        for family in &FAMILIES[..6] {
            log += &format!(
                "test {}::control ... ok\n",
                family.strip_prefix("lib:").unwrap()
            );
        }
        log += "test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n";
        log += "     Running unittests src/bin/lean-render-observe.rs (target/debug/deps/lean)\ntest rendered_outline::control ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n";
        log
    }

    fn portable() -> String {
        format!(
            "     Running tests/portable_runtime.rs (target/debug/deps/portable)\ntest {PORTABLE} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n"
        )
    }

    #[test]
    fn required_receipts_are_target_qualified_closed_and_successful() {
        let f = frontier();
        let p = portable();
        assert!(check("frontier", &f).is_ok());
        assert!(check("portable", &p).is_ok());
        for (mode, log) in [
            ("frontier", f.replace("src/lib.rs", "src/bin/unrelated.rs")),
            ("frontier", f.replace("token_metadata_tests", "unrelated")),
            (
                "frontier",
                f.replace(
                    "token_metadata_tests::control ... ok",
                    "token_metadata_tests::control ... ignored",
                ),
            ),
            (
                "frontier",
                f.replace("6 passed; 0 failed", "5 passed; 1 failed"),
            ),
            (
                "frontier",
                f.replace("6 passed; 0 failed", "0 passed; 0 failed"),
            ),
            (
                "frontier",
                f.replace(
                    "test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n",
                    "",
                ),
            ),
            (
                "portable",
                p.replace("tests/portable_runtime.rs", "tests/unrelated.rs"),
            ),
            ("portable", p.replace(PORTABLE, "unrelated")),
            ("portable", p.replace(" ... ok", " ... ignored")),
            (
                "portable",
                p.replace(
                    "1 passed; 0 failed; 0 ignored",
                    "1 passed; 0 failed; 1 ignored",
                ),
            ),
            (
                "portable",
                p.replace("1 passed; 0 failed", "0 passed; 0 failed"),
            ),
            ("portable", p.clone() + &p),
            (
                "portable",
                p.replace(" ... ok\n", &format!(" ... ok\ntest {PORTABLE} ... ok\n")),
            ),
            ("portable", p.replace("0 failed", "1 failed")),
            ("portable", p.replace("0 measured", "1 measured")),
            ("portable", p.replace("0 filtered out", "1 filtered out")),
            (
                "portable",
                p.replace("test result: ok.", "test result: FAILED."),
            ),
            ("portable", p.lines().take(2).collect::<Vec<_>>().join("\n")),
            (
                "portable",
                p.lines().take(2).collect::<Vec<_>>().join("\n") + "\n" + &p,
            ),
        ] {
            assert!(
                check(mode, &log).is_err(),
                "accepted {mode} mutation: {log}"
            );
        }
        assert!(
            check(
                "portable",
                "test result: ok. 1 passed; 0 failed; 0 ignored;\n"
            )
            .is_err()
        );
        assert!(check("unknown", &p).is_err());
    }
}
