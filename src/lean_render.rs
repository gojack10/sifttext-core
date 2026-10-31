// Lossless LEAN rendering of generated outline rows: pure, reversible encoding only.
//
// Pinned dialect: inputs/lean/lean.ts, SHA-256
// e1b9dbd8b3a0845097e744fc25065901e52c703b9e17bc98c5b0a9e4bd467ba8.
// The pinned decoder is the acceptance oracle; this module never decodes, stores, refreshes or
// publishes anything. Supported boundary: root scalar metadata plus a `rows` array of
// scalar-valued objects. Anything outside that boundary is rejected, not silently dropped.
//
// Not yet registered in src/lib.rs: `src/bin/lean-render-observe.rs` includes this module by
// path so the pure contribution stays inside its own files until library registration lands.

#[path = "lean_render/receipt.rs"]
mod receipt;

/// Render nested CLI receipts without changing generated-outline canonical bytes.
pub use receipt::render as render_receipt;

use serde_json::{Map, Value};
use std::fmt::Write;

const ROWS: &str = "rows";

pub fn render(document: &Value) -> Result<String, String> {
    let mut text = String::new();
    write_into(document, &mut text)?;
    Ok(text)
}

/// Append the exact rendered bytes, without a final LF or a second output-sized string.
pub fn write_into(document: &Value, text: &mut String) -> Result<(), String> {
    // serde_json has already collapsed duplicate keys; native rows come from typed data.
    let Value::Object(root) = document else {
        return Err("outline document must be a JSON object".to_owned());
    };
    if root.is_empty() {
        text.push_str("{}");
        return Ok(());
    }
    // Instructions retain their owner-selected order; other metadata stays sorted, rows last.
    if root.len() == 2 && root.contains_key("version") && root.contains_key("path") {
        write_field_into("version", &root["version"], text)?;
        text.push('\n');
        return write_field_into("path", &root["path"], text);
    }
    let mut first = true;
    for (key, value) in root.iter().filter(|(key, _)| key.as_str() != ROWS) {
        if !first { text.push('\n'); }
        first = false;
        write_field_into(key, value, text)?;
    }
    if let Some(rows) = root.get(ROWS) {
        if !first { text.push('\n'); }
        write_field_into(ROWS, rows, text)?;
    }
    Ok(())
}

/// One root field, for canonical field-by-field generated artifact assembly.
pub(crate) fn write_field_into(key: &str, value: &Value, text: &mut String) -> Result<(), String> {
    validate_key(key)?;
    if key == ROWS {
        encode_rows(rows_of(value)?, text)
    } else {
        text.push_str(key);
        text.push(':');
        text.push_str(&scalar(value).map_err(|error| format!("metadata {key}: {error}"))?);
        Ok(())
    }
}

fn rows_of(value: &Value) -> Result<&[Value], String> {
    let Value::Array(items) = value else {
        return Err("rows must be an array of objects".to_owned());
    };
    for item in items {
        let Value::Object(row) = item else {
            return Err("each row must be an object".to_owned());
        };
        if row.is_empty() {
            return Err("each row must carry at least one field".to_owned());
        }
        for key in row.keys() { validate_key(key)?; }
    }
    Ok(items)
}

fn encode_rows(items: &[Value], text: &mut String) -> Result<(), String> {
    write!(text, "{ROWS}[{}]:", items.len()).unwrap();
    if items.is_empty() { return Ok(()) }
    // rows_of already validated every object and key; no second row inventory is needed.
    let fields: Vec<&str> = items[0].as_object().unwrap().keys().map(String::as_str).collect();
    let uniform = items.iter().all(|row| row.as_object().unwrap().keys().map(String::as_str).eq(fields.iter().copied()));
    let shared: Vec<&str> = fields.into_iter()
        .filter(|key| items.iter().all(|row| row.get(*key).is_some())).collect();
    if uniform || (items.len() >= 2 && !shared.is_empty()) {
        text.push_str(&shared.join("\t"));
        if !uniform { text.push_str("\t~"); }
        for item in items {
            let row = item.as_object().unwrap();
            text.push_str("\n  ");
            for (i, key) in shared.iter().enumerate() {
                if i > 0 { text.push('\t'); }
                text.push_str(&cell_of(row, key)?);
            }
            for (key, value) in row.iter().filter(|(key, _)| !shared.contains(&key.as_str())) {
                text.push('\t');
                text.push_str(key);
                text.push(':');
                text.push_str(&cell(value).map_err(|error| format!("row field {key}: {error}"))?);
            }
        }
    } else {
        for item in items {
            for (i, (key, value)) in item.as_object().unwrap().iter().enumerate() {
                text.push_str(if i == 0 { "\n  - " } else { "\n    " });
                text.push_str(key);
                text.push(':');
                text.push_str(&scalar(value)?);
            }
        }
    }
    Ok(())
}

fn cell_of(row: &Map<String, Value>, field: &str) -> Result<String, String> {
    let value = row
        .get(field)
        .ok_or_else(|| format!("row lost supplied field {field}"))?;
    cell(value)
}

pub(crate) fn scalar(value: &Value) -> Result<String, String> {
    primitive(value, true)
}

pub(crate) fn cell(value: &Value) -> Result<String, String> {
    primitive(value, false)
}

fn primitive(value: &Value, scalar_context: bool) -> Result<String, String> {
    match value {
        Value::Null => Ok("_".to_owned()),
        Value::Bool(true) => Ok("T".to_owned()),
        Value::Bool(false) => Ok("F".to_owned()),
        Value::Number(number) => Ok(number.to_string()),
        Value::String(text) => Ok(if scalar_context {
            encode_scalar(text)
        } else {
            encode_cell(text)
        }),
        _ => Err("unsupported non-scalar value".to_owned()),
    }
}

// Keys follow the pinned law: /^[\w][\w-]*$/ ASCII. Dots, colons, spaces and slashes stay out.
fn validate_key(key: &str) -> Result<(), String> {
    let valid = !key.is_empty()
        && key.bytes().enumerate().all(|(index, byte)| match index {
            0 => byte.is_ascii_alphanumeric() || byte == b'_',
            _ => byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-',
        });
    if valid {
        Ok(())
    } else {
        Err(format!(
            "unsupported key {key:?}: LEAN keys must match /^[\\w][\\w-]*$/"
        ))
    }
}

fn needs_quoting(text: &str) -> bool {
    text.is_empty()
        || matches!(text, "T" | "F" | "_")
        || has_outer_space(text)
        || looks_numeric(text)
        || text
            .bytes()
            .any(|byte| matches!(byte, b'\t' | b'\n' | b'\\' | b'"'))
}

// The pinned decoder reads a bare `key:{}` as an empty object, so a literal `{}` string keeps
// its quotes. This is the only quoting the encoder adds beyond the pinned spec list.
fn encode_scalar(text: &str) -> String {
    if !needs_quoting(text) && text != "{}" {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn encode_cell(text: &str) -> String {
    if !needs_quoting(text) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '"' => out.push_str("\"\""),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

// JS `trim` is the boundary the pinned decoder applies to every bare token. Rust's whitespace set
// is used plus U+FEFF, which JS trims but Unicode White_Space does not list.
fn has_outer_space(text: &str) -> bool {
    let is_space = |ch: char| ch.is_whitespace() || ch == '\u{feff}';
    text.chars().next().is_some_and(is_space) || text.chars().next_back().is_some_and(is_space)
}

// Pinned NUMBER_REGEX: /^-?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$
fn looks_numeric(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut at = 0;
    if bytes.first() == Some(&b'-') {
        at = 1;
    }
    let mut ok = false;
    if digits(bytes, &mut at) {
        if bytes.get(at) == Some(&b'.') {
            at += 1;
            digits(bytes, &mut at);
        }
        ok = true;
    } else if bytes.get(at) == Some(&b'.') {
        at += 1;
        ok = digits(bytes, &mut at);
    }
    if !ok {
        return false;
    }
    if bytes.get(at).is_some_and(|byte| *byte == b'e' || *byte == b'E') {
        let mut exponent = at + 1;
        if bytes.get(exponent).is_some_and(|byte| *byte == b'+' || *byte == b'-') {
            exponent += 1;
        }
        if !digits(bytes, &mut exponent) {
            return false;
        }
        at = exponent;
    }
    at == bytes.len()
}

fn digits(bytes: &[u8], at: &mut usize) -> bool {
    let before = *at;
    while bytes.get(*at).is_some_and(u8::is_ascii_digit) {
        *at += 1;
    }
    *at > before
}
