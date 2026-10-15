//! Installed Linux package receipts; source custody and process drain stay with the launcher.
use std::fs;
const CONTROL: &str = "relocated_runtime_refuses_missing_and_tampered_assets";

fn listed(list: &str) -> Result<(), String> {
    if list
        .lines()
        .filter(|s| s.ends_with(": test"))
        .collect::<Vec<_>>()
        != [format!("{CONTROL}: test")]
    {
        return Err("Package must list exactly the portable control".into());
    }
    Ok(())
}

fn check(list: &str, log: &str) -> Result<String, String> {
    listed(list)?;
    super::check("portable", log)?;
    if !log
        .lines()
        .any(|s| s.contains("PORTABLE_WORKFLOW_CHECKED nodes="))
        || !log
            .lines()
            .any(|s| s.contains("RELOCATED_RUNTIME_CHECKED "))
        || log
            .lines()
            .filter(|s| s.contains("RUNTIME_ASSET_REFUSAL missing/tampered "))
            .count()
            != 8
    {
        return Err("Missing portable workflow, relocation or eight asset refusal receipts".into());
    }
    Ok("portable=1/1 assets=8".into())
}

pub fn run(args: &[String]) -> Result<String, String> {
    if let [mode, path] = args
        && mode == "linux-package-list"
    {
        listed(&fs::read_to_string(path).map_err(|e| e.to_string())?)?;
        return Ok("portable-listed=1".into());
    }
    if let [mode, path] = args
        && mode == "linux-runtime"
    {
        let receipt = fs::read_to_string(path).map_err(|e| e.to_string())?;
        return if receipt.contains("\"status\": \"checked\"") {
            Ok("runtime=checked".into())
        } else {
            Err("Missing checked runtime receipt".into())
        };
    }
    let [_, list, log] = args else {
        return Err("Usage: acceptance-receipts linux-package LIST LOG".into());
    };
    check(
        &fs::read_to_string(list).map_err(|e| e.to_string())?,
        &fs::read_to_string(log).map_err(|e| e.to_string())?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn package_requires_exact_execution_workflow_relocation_and_refusals() {
        let list = format!("{CONTROL}: test\n1 test, 0 benchmarks\n");
        let log = format!(
            "Running tests/portable_runtime.rs (target/debug/deps/portable)\ntest {CONTROL} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n"
        ) + "PORTABLE_WORKFLOW_CHECKED nodes=78\nRELOCATED_RUNTIME_CHECKED source=fixture\n"
            + &"RUNTIME_ASSET_REFUSAL missing/tampered asset\n".repeat(8);
        assert_eq!(check(&list, &log).unwrap(), "portable=1/1 assets=8");
        let split = log.replace(" ... ok\n", " ... fixture output\nok\n");
        assert!(check(&list, &split).is_ok());
        for bad in [
            split.replace("\nok\n", "\nFAILED\n"),
            split.replace("\nok\n", "\n"),
            split.replace("\nok\n", &format!("\ntest {CONTROL} ... fixture\nok\n")),
        ] {
            assert!(check(&list, &bad).is_err());
        }
        for bad in [
            list.replace(CONTROL, "wrong"),
            list.clone() + &list,
            String::new(),
        ] {
            assert!(check(&bad, &log).is_err());
        }
        for bad in [
            log.replace("PORTABLE_WORKFLOW_CHECKED", "missing"),
            log.replace("RELOCATED_RUNTIME_CHECKED", "missing"),
            log.replacen("RUNTIME_ASSET_REFUSAL", "missing", 1),
            log.clone() + "RUNTIME_ASSET_REFUSAL missing/tampered extra\n",
            log.replace(" ... ok", " ... ignored"),
            log.replace("0 failed", "1 failed"),
            log.replace(
                "Running tests/portable_runtime.rs",
                "Running tests/unrelated.rs",
            ),
            log.replace(
                "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n",
                "",
            ),
        ] {
            assert!(check(&list, &bad).is_err(), "{bad}");
        }
    }
}
