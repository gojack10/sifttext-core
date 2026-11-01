//! Agent-facing onboarding, projected from retained state without changing it.
use super::{commands, sync};
use serde_json::{Value, json};
use sifttext_file_graph::{Result, hosted_checks, lean_render, updates};
use std::{collections::BTreeSet, path::Path};

fn field(lines: &mut Vec<String>, key: &str, value: Value) -> Result<()> {
    lines.push(lean_render::render(&json!({key:value}))?);
    Ok(())
}

fn flagged_nodes(check: &Value, nodes: &mut BTreeSet<String>) {
    if check["status"] != "findings" {
        return;
    }
    for result in check["results"].as_array().into_iter().flatten() {
        if result["response"]["passed"] == false {
            if let Some(node) = result["node"].as_str() {
                nodes.insert(node.to_owned());
            }
        }
    }
}

// LEAN forbids duplicate keys. Keep the requested scalar for one item and use
// a uniform table for repeated rows, rather than silently losing continuations.
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
    let mut flagged = BTreeSet::new();
    let mut stopped = false;
    field(&mut lines, "onboarding-entry", entry["path"].clone())?;
    match sync::instructions(&home) {
        Ok(receipts) => {
            for receipt in receipts {
                if receipt["status"] == "sync_failed" {
                    stopped = true;
                }
                if let Some(guide) = receipt["repair_guide"].as_str() {
                    guides.insert(guide.to_owned());
                }
                flagged_nodes(&receipt["hosted_check"], &mut flagged);
            }
        }
        Err(_) => stopped = true,
    }
    match hosted_checks::retained_publication(&home) {
        Ok(Some(check)) => flagged_nodes(&check, &mut flagged),
        Ok(None) => (),
        Err(error) => eprintln!("Retained submit check unavailable: {error}"),
    }
    field(&mut lines, "hosted-sync", json!(if !guides.is_empty() {
        "needs-repair"
    } else if stopped {
        "stopped"
    } else {
        "up-to-date"
    }))?;
    references(&mut lines, "repair-guide", &guides)?;
    if !guides.is_empty() {
        actions.push(format!("Read {}. Resolve it for the user, and keep other work moving while you do. Only something genuinely significant goes to the user.",
            if guides.len() == 1 { "repair-guide" } else { "each repair-guide in repair-guides" }));
    }
    if stopped {
        actions.push(format!("Inspect `sifttext sync status '{}'`. Resolve the stopped sync for the user, and keep other work moving while you do. Only something genuinely significant goes to the user.",
            home.display().to_string().replace('\'', "'\\''")));
    }
    if !flagged.is_empty() {
        field(&mut lines, "check-findings", json!(flagged.len()))?;
        references(&mut lines, "flagged-note", &flagged)?;
        actions.push(format!("Read the retained check result for {}. Resolve its findings for the user, and keep other work moving while you do. Only something genuinely significant goes to the user.",
            if flagged.len() == 1 { "flagged-note" } else { "each flagged-note in flagged-notes" }));
    }
    match updates::instruction_notice() {
        Ok(notice) if notice["status"] == "update_available" => {
            field(&mut lines, "update-available", notice["application"].clone())?;
            field(&mut lines, "update-guide", json!(home.join("nodes").join(commands::UPDATE_GUIDE).join("node.md")))?;
            // Existing notices deliberately omit permission when standing permission is recorded.
            actions.push(if notice["permission"] == "ask" {
                "Read update-guide. When you reach a convenient pause, ask the user for permission to update SiftText."
            } else {
                "Read update-guide. At a convenient pause, update SiftText. The user has already given standing permission."
            }.to_owned());
        }
        Ok(notice) if notice["status"] == "needs_agent" => {
            field(&mut lines, "update-guide", json!(home.join("nodes").join(commands::UPDATE_GUIDE).join("node.md")))?;
            actions.push("Read update-guide. Inspect `sifttext update --local-status` for the retained update repair. Resolve it for the user, and keep other work moving while you do. Only something genuinely significant goes to the user.".to_owned());
        }
        Err(error) => eprintln!("SiftText update deferred: {error}"),
        _ => (),
    }
    if !actions.is_empty() {
        field(&mut lines, "agent-instructions", json!(actions.join(" ")))?;
    }
    lines.push("sifttext-help-output:".into());
    lines.extend(commands::index()?.lines().map(|line| format!("  {line}")));
    Ok(lines.join("\n"))
}
