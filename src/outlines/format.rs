use crate::{
    lean_render::{cell, scalar},
    outline_rows::PATH_TEMPLATE,
};
use serde_json::{Map, Value};

// Full outline rows have fixed columns so each row has the same token cost
// wherever it appears. Optional fields are always keyed.
const FULL_OUTLINE_FIELDS: [&str; 6] =
    ["children", "depth", "id", "micro_summary", "name", "status"];

pub(crate) fn full_outline_header(version: &str, rows: i64) -> String {
    let version = scalar(&Value::from(version)).expect("version is a LEAN scalar");
    format!(
        "coverage:full\npath_template:{}\nversion:{version}\nrows[{rows}]:{}\t~\n",
        PATH_TEMPLATE,
        FULL_OUTLINE_FIELDS.join("\t")
    )
}

pub(crate) fn write_full_outline_row(
    row: &Map<String, Value>,
    text: &mut String,
) -> Result<(), String> {
    text.push_str("  ");
    for (index, key) in FULL_OUTLINE_FIELDS.iter().enumerate() {
        if index > 0 {
            text.push('\t');
        }
        text.push_str(&cell(row.get(*key).ok_or("missing full outline column")?)?);
    }
    for (key, value) in row
        .iter()
        .filter(|(key, _)| !FULL_OUTLINE_FIELDS.contains(&key.as_str()))
    {
        text.push('\t');
        text.push_str(key);
        text.push(':');
        text.push_str(&cell(value)?);
    }
    text.push('\n');
    Ok(())
}

pub(crate) fn matches_full_outline(document: &Value, text: &str) -> Result<bool, String> {
    let Some(root) = document.as_object() else {
        return Ok(false);
    };
    if root.len() != 4
        || root.get("coverage").and_then(Value::as_str) != Some("full")
        || root.get("path_template").and_then(Value::as_str) != Some(PATH_TEMPLATE)
    {
        return Ok(false);
    }
    let (Some(version), Some(rows)) = (
        root.get("version").and_then(Value::as_str),
        root.get("rows").and_then(Value::as_array),
    ) else {
        return Ok(false);
    };
    if version.len() != 64 || !version.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(false);
    }
    let mut rendered = full_outline_header(version, rows.len() as i64);
    for row in rows {
        write_full_outline_row(
            row.as_object().ok_or("invalid full outline row")?,
            &mut rendered,
        )?;
    }
    Ok(rendered == text)
}
