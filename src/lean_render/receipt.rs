//! Nested transaction receipts in the pinned LEAN dialect.
//! Generated outlines keep their existing scalar/row encoder and canonical bytes.

use super::{cell, encode_scalar, scalar, validate_key};
use serde_json::Value;
use std::fmt::Write;

pub fn render(document: &Value) -> Result<String, String> {
    let mut text = String::new();
    match document {
        Value::Object(object) if object.is_empty() => text.push_str("{}"),
        Value::Object(object) => {
            for (key, value) in object {
                property(key, value, 0, &mut text)?;
            }
        }
        Value::Array(items) => array("", items, 0, true, &mut text)?,
        Value::String(value) => quoted(value, &mut text),
        _ => text.push_str(&scalar(document)?),
    }
    if text.ends_with('\n') {
        text.pop();
    }
    Ok(text)
}

fn pad(indent: usize, text: &mut String) {
    for _ in 0..indent {
        text.push_str("  ");
    }
}

fn property(key: &str, value: &Value, indent: usize, text: &mut String) -> Result<(), String> {
    validate_key(key)?;
    if let Value::Array(items) = value {
        return array(key, items, indent, true, text);
    }
    pad(indent, text);
    text.push_str(key);
    text.push(':');
    match value {
        Value::Object(object) if object.is_empty() => text.push_str("{}\n"),
        Value::Object(object) => {
            text.push('\n');
            for (key, value) in object {
                property(key, value, indent + 1, text)?;
            }
        }
        _ => {
            text.push_str(&scalar(value)?);
            text.push('\n');
        }
    }
    Ok(())
}

fn array(
    key: &str,
    items: &[Value],
    indent: usize,
    inline_scalars: bool,
    text: &mut String,
) -> Result<(), String> {
    pad(indent, text);
    write!(text, "{key}[{}]:", items.len()).unwrap();
    if items.is_empty() {
        text.push('\n');
        return Ok(());
    }
    if inline_scalars
        && items
            .iter()
            .all(|value| !value.is_object() && !value.is_array())
    {
        for (index, value) in items.iter().enumerate() {
            if index > 0 {
                text.push('\t');
            }
            text.push_str(&cell(value)?);
        }
        text.push('\n');
        return Ok(());
    }
    // Uniform scalar objects use the same tabular syntax as outline rows.
    if let Some(first) = items[0].as_object().filter(|object| !object.is_empty()) {
        if items.iter().all(|item| {
            item.as_object().is_some_and(|object| {
                object.keys().eq(first.keys())
                    && object
                        .values()
                        .all(|value| !value.is_object() && !value.is_array())
            })
        }) {
            for (index, key) in first.keys().enumerate() {
                validate_key(key)?;
                if index > 0 {
                    text.push('\t');
                }
                text.push_str(key);
            }
            text.push('\n');
            for item in items {
                pad(indent + 1, text);
                for (index, value) in item.as_object().unwrap().values().enumerate() {
                    if index > 0 {
                        text.push('\t');
                    }
                    text.push_str(&cell(value)?);
                }
                text.push('\n');
            }
            return Ok(());
        }
    }
    text.push('\n');
    for item in items {
        match item {
            Value::Object(object) if !object.is_empty() => {
                // The first property follows "- "; its nested children retain
                // their ordinary indentation, as do the remaining properties.
                let start = text.len();
                for (index, (key, value)) in object.iter().enumerate() {
                    if index == 0
                        && let Value::Array(items) = value
                    {
                        validate_key(key)?;
                        // The pinned list-object grammar has no inline flat
                        // array arm for its first property; use dashed items.
                        array(key, items, indent + 2, false, text)?;
                    } else {
                        property(key, value, indent + 2, text)?;
                    }
                }
                let marker = start + (indent + 1) * 2;
                text.replace_range(marker..marker + 2, "- ");
            }
            Value::Array(items) => {
                let start = text.len();
                array("", items, indent + 2, true, text)?;
                let marker = start + (indent + 1) * 2;
                text.replace_range(marker..marker + 2, "- ");
            }
            _ => {
                pad(indent + 1, text);
                text.push_str("- ");
                match item {
                    Value::Object(_) => text.push_str("{}"),
                    // Quote strings to distinguish text such as "key:value"
                    // or "[0]:" from list objects and nested arrays.
                    Value::String(value) => quoted(value, text),
                    _ => text.push_str(&scalar(item)?),
                }
                text.push('\n');
            }
        }
    }
    Ok(())
}

fn quoted(value: &str, text: &mut String) {
    let encoded = encode_scalar(value);
    if encoded.starts_with('"') {
        text.push_str(&encoded);
    } else {
        text.push('"');
        text.push_str(&encoded);
        text.push('"');
    }
}
