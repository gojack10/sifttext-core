//! Agent-facing onboarding, projected from retained state without changing it.
use super::{commands, sync};
use serde_json::{Value, json};
use sifttext_file_graph::{Result, hosted_checks, lean_render, updates};
use std::{collections::{BTreeMap, BTreeSet}, path::Path};

fn field(lines: &mut Vec<String>, key: &str, value: Value) -> Result<()> {
    lines.push(lean_render::render(&json!({key:value}))?);
    Ok(())
}

fn findings(check: &Value, flagged: &mut BTreeMap<(String, String), Value>) {
    for result in check["results"].as_array().into_iter().flatten() {
        if !hosted_checks::has_findings(&result["response"]) { continue }
        if let (Some(node), Some(request)) = (result["node"].as_str(), result["response"]["request_id"].as_str()) {
            flagged.insert((node.to_owned(), request.to_owned()), result.clone());
        }
    }
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
    let mut flagged = BTreeMap::new();
    let mut stopped = false;
    let mut transport = "unknown".to_owned();
    field(&mut lines, "onboarding-entry", entry["path"].clone())?;
    match sync::instructions(&home) {
        Ok(receipts) => for receipt in receipts {
            if let Some(state) = receipt["transport"].as_str() { transport = state.to_owned(); }
            if receipt["status"] == "sync_failed" { stopped = true; }
            if let Some(guide) = receipt["repair_guide"].as_str() { guides.insert(guide.to_owned()); }
            findings(&receipt["hosted_check"], &mut flagged);
        },
        Err(error) => {
            stopped = true;
            eprintln!("Retained sync state unavailable: {error}");
        }
    }
    match hosted_checks::retained_findings(&home) {
        Ok(results) => findings(&json!({"results":results}), &mut flagged),
        Err(error) => eprintln!("Retained node checks unavailable: {error}"),
    }
    // Retain older receipts too: they still identify the actual saved request.
    match hosted_checks::retained_publication(&home) {
        Ok(Some(check)) => findings(&check, &mut flagged),
        Ok(None) => (),
        Err(error) => eprintln!("Retained submit check unavailable: {error}"),
    }
    field(&mut lines, "hosted-sync", json!(if !guides.is_empty() {
        "needs-repair"
    } else if stopped {
        "stopped"
    } else {
        &transport
    }))?;
    references(&mut lines, "repair-guide", &guides)?;
    if !guides.is_empty() {
        actions.push("Read repair-guide (each row if several). Resolve it for the user, and keep other work moving while you do. Only something genuinely significant goes to the user.".to_owned());
    }
    if stopped {
        actions.push(format!("Inspect `sifttext sync status '{}'`. Resolve the stopped sync for the user, and keep other work moving while you do. Only something genuinely significant goes to the user.",
            home.display().to_string().replace('\'', "'\\''")));
    }
    if !flagged.is_empty() {
        field(&mut lines, "check-findings", json!(flagged.len()))?;
        let rows = flagged.iter().map(|((node, request), item)| vec![json!(node), json!(request),
            item.get("text_sha256").cloned().unwrap_or(json!("historical; retrieve submitted text"))]).collect::<Vec<_>>();
        lines.push(lean_render::render_table("flagged-nodes", &["flagged-node", "check-result", "text-sha256"], &rows)?);
        actions.push("Run `sifttext check-result ID` for each check-result above. Compare its submitted text with flagged-node, repair supported findings through SiftText Commit, and keep other work moving. Only something genuinely significant goes to the user. Findings do not block own-device sync; do not share a flagged node with another person until resolved.".to_owned());
    }
    match updates::instruction_notice() {
        Ok(notice) if notice["status"] == "update_available" => {
            field(&mut lines, "update-available", notice["application"].clone())?;
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
