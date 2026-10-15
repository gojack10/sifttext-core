//! Cargo --nocapture framing shared by portable acceptance consumers.
pub fn normalize(text: &str, control: &str) -> Result<String, String> {
    let prefix = format!("test {control} ... ");
    let (mut pending, mut normalized) = (false, String::new());
    for line in text.lines() {
        if let Some(status) = line.strip_prefix(&prefix) {
            if pending {
                return Err("Duplicate unfinished portable control".into());
            }
            if !matches!(status, "ok" | "FAILED" | "ignored") {
                pending = true;
                continue;
            }
        } else if pending && matches!(line, "ok" | "FAILED" | "ignored") {
            normalized.push_str(&prefix);
            pending = false;
        } else if pending
            && (line.starts_with("test result:")
                || line.trim_start().starts_with("Running ")
                || line.trim_start().starts_with("Doc-tests "))
        {
            return Err("Unfinished portable control".into());
        }
        normalized.push_str(line);
        normalized.push('\n');
    }
    if pending {
        return Err("Unfinished portable control".into());
    }
    Ok(normalized)
}
