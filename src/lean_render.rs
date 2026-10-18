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

use serde_json::{Map, Value};

const ROWS: &str = "rows";

pub fn render(document: &Value) -> Result<String, String> {
    // ponytail: serde_json's map collapses a duplicate JSON key to its last value before this
    // encoder runs. Real outline rows come from typed graph data, not hand-written JSON; add an
    // explicit duplicate-key rejection here if any caller ever feeds raw authored JSON in.
    let Value::Object(root) = document else {
        return Err("outline document must be a JSON object".to_owned());
    };
    // The pinned encoder writes an empty object as "{}" and its decoder reads nothing else; an
    // empty line list is not decodable LEAN.
    if root.is_empty() {
        return Ok("{}".to_owned());
    }
    let mut lines: Vec<String> = Vec::new();
    let mut rows: Option<Vec<&Map<String, Value>>> = None;
    // The instruction response has an owner-selected field order; other documents stay sorted.
    let keys = if root.len() == 2
        && root.contains_key("version")
        && root.contains_key("path")
    {
        vec!["version", "path"]
    } else {
        root.keys().map(String::as_str).collect()
    };
    for key in keys {
        let value = &root[key];
        validate_key(key)?;
        if key == ROWS {
            rows = Some(rows_of(value)?);
        } else {
            let text = scalar(value).map_err(|error| format!("metadata {key}: {error}"))?;
            lines.push(format!("{key}:{text}"));
        }
    }
    if let Some(items) = rows {
        encode_rows(&items, &mut lines)?;
    }
    Ok(lines.join("\n"))
}

fn rows_of(value: &Value) -> Result<Vec<&Map<String, Value>>, String> {
    let Value::Array(items) = value else {
        return Err("rows must be an array of objects".to_owned());
    };
    items
        .iter()
        .map(|item| {
            let Value::Object(row) = item else {
                return Err("each row must be an object".to_owned());
            };
            // A fieldless row has no tabular header and no dashed item to decode back.
            if row.is_empty() {
                return Err("each row must carry at least one field".to_owned());
            }
            for key in row.keys() {
                validate_key(key)?;
            }
            Ok(row)
        })
        .collect()
}

fn encode_rows(items: &[&Map<String, Value>], lines: &mut Vec<String>) -> Result<(), String> {
    if items.is_empty() {
        lines.push(format!("{ROWS}[0]:"));
        return Ok(());
    }
    let fields: Vec<&str> = items[0].keys().map(String::as_str).collect();
    if items
        .iter()
        .all(|row| row.keys().map(String::as_str).eq(fields.iter().copied()))
    {
        lines.push(format!("{ROWS}[{}]:{}", items.len(), fields.join("\t")));
        for row in items {
            let mut cells = Vec::with_capacity(fields.len());
            for field in &fields {
                cells.push(cell_of(row, field)?);
            }
            lines.push(format!("  {}", cells.join("\t")));
        }
        return Ok(());
    }
    // Semi-tabular: objects that share at least one field in every row. Optional fields ride as
    // trailing `key:value` cells under the pinned `~` header marker.
    let shared: Vec<&str> = fields
        .into_iter()
        .filter(|key| items.iter().all(|row| row.contains_key(*key)))
        .collect();
    if items.len() >= 2 && !shared.is_empty() {
        lines.push(format!("{ROWS}[{}]:{}\t~", items.len(), shared.join("\t")));
        for row in items {
            let mut cells = Vec::with_capacity(row.len());
            for key in &shared {
                cells.push(cell_of(row, key)?);
            }
            for (key, value) in row.iter().filter(|(key, _)| !shared.contains(&key.as_str())) {
                let text = cell(value).map_err(|error| format!("row field {key}: {error}"))?;
                cells.push(format!("{key}:{text}"));
            }
            lines.push(format!("  {}", cells.join("\t")));
        }
        return Ok(());
    }
    // No shared field: the pinned dashed list keeps every supplied key and the supplied order.
    lines.push(format!("{ROWS}[{}]:", items.len()));
    for row in items {
        let mut supplied = row.iter();
        let (key, value) = supplied
            .next()
            .ok_or_else(|| "each row must carry at least one field".to_owned())?;
        lines.push(format!("  - {key}:{}", scalar(value)?));
        for (key, value) in supplied {
            lines.push(format!("    {key}:{}", scalar(value)?));
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

fn scalar(value: &Value) -> Result<String, String> {
    primitive(value, true)
}

fn cell(value: &Value) -> Result<String, String> {
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
