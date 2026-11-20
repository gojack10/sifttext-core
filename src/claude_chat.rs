//! Read a claude.ai chat's stored record with the Claude Code sign-in on this computer.
//!
//! Anthropic runs each claude.ai chat, classic Project chats included, as a Claude Code
//! cloud session tagged `bard-conversation:<chat id>`. Claude Code's own sign-in can read
//! that session's events through the API Claude Code itself calls. The route is
//! undocumented and can change without notice.
//!
//! The sign-in is only read. Renewing it here would rotate the refresh token out from
//! under Claude Code and sign the user out, so an expired sign-in is reported instead.
//!
//! Only the Claude Code command line stores a sign-in this module can read. The desktop
//! app hands its own sign-in to the Code tab's process and withholds it from the commands
//! that process runs, so a computer with the desktop app alone has none.

use crate::{Result, require};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

const API: &str = "https://api.anthropic.com";
const KEYCHAIN_ITEM: &str = "Claude Code-credentials";
const PAGE: usize = 500;
/// The trial accepts 128 MiB per chat, so no single reply needs to be larger.
const REPLY_LIMIT: &str = "134217728";

const NOT_SIGNED_IN: &str = "No Claude Code sign-in was found on this computer, so the linked chat cannot be read. The Claude desktop app's Code tab does not leave one. Sign in once with the Claude Code command line (`claude auth login`), using the Claude account that owns the chat, then retry.";
const SIGN_IN_EXPIRED: &str = "Claude Code's sign-in on this computer has expired. Open Claude Code so it renews the sign-in, then retry.";
const CHAT_NOT_FOUND: &str = "No chat with that link was found under the Claude account signed in to Claude Code on this computer. Check that the link comes from the same Claude account.";

/// A chat named by its claude.ai link or ID, or by the cloud session that runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    Chat(String),
    Session(String),
}

/// Recognize `https://claude.ai/chat/<id>`, a bare chat ID, or a session ID written as
/// `cse_…` or `session_…`. A chat's own scheduling tools report its session as
/// `session_…`; the session API names the same session `cse_…`.
pub fn reference(text: &str) -> Option<Reference> {
    let text = text.trim();
    if let Some(id) = text.strip_prefix("cse_").or_else(|| text.strip_prefix("session_"))
        && !id.is_empty()
        && id.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return Some(Reference::Session(format!("cse_{id}")));
    }
    let id = match text.split_once("claude.ai/chat/") {
        Some((_, rest)) => rest.split(['/', '?', '#']).next().unwrap_or_default(),
        None => text,
    };
    crate::uuid(id).then(|| Reference::Chat(id.to_owned()))
}

/// The downloaded chat as the exact bytes to upload, with the facts a person checks first.
pub struct Chat {
    pub summary: Value,
    pub bytes: Vec<u8>,
}

struct SignIn {
    token: String,
    organization: String,
}

/// Where Claude Code keeps its account file and its sign-in file, and whether it uses its
/// default location. A relocated config directory has its own Keychain item under another
/// name, so only the default location reads the default item.
fn claude_home() -> Result<(PathBuf, PathBuf, bool)> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        let dir = PathBuf::from(dir);
        return Ok((dir.join(".claude.json"), dir.join(".credentials.json"), false));
    }
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?);
    Ok((home.join(".claude.json"), home.join(".claude/.credentials.json"), true))
}

fn sign_in() -> Result<SignIn> {
    let (account_file, credentials_file, default_location) = claude_home()?;
    // macOS keeps the sign-in in the Keychain item Claude Code created through
    // `security`, so reading it the same way raises no prompt. Other systems use a file.
    let keychain = (cfg!(target_os = "macos") && default_location)
        .then(|| Command::new("/usr/bin/security")
            .args(["find-generic-password", "-s", KEYCHAIN_ITEM, "-w"])
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| output.stdout))
        .flatten();
    let stored = keychain.or_else(|| std::fs::read(&credentials_file).ok()).ok_or(NOT_SIGNED_IN)?;
    let stored: Value = serde_json::from_slice(&stored).map_err(|_| NOT_SIGNED_IN)?;
    let oauth = &stored["claudeAiOauth"];
    let token = oauth["accessToken"].as_str().filter(|token| !token.is_empty()).ok_or(NOT_SIGNED_IN)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    require(oauth["expiresAt"].as_u64().is_none_or(|expires| u128::from(expires) > now), SIGN_IN_EXPIRED)?;
    let account: Value = serde_json::from_slice(&std::fs::read(&account_file).map_err(|_| NOT_SIGNED_IN)?)
        .map_err(|_| NOT_SIGNED_IN)?;
    let organization = account["oauthAccount"]["organizationUuid"].as_str()
        .filter(|id| crate::uuid(id)).ok_or(NOT_SIGNED_IN)?;
    Ok(SignIn { token: token.to_owned(), organization: organization.to_owned() })
}

fn quoted(value: &str) -> Result<String> {
    require(!value.chars().any(char::is_control), "invalid control character in Claude request")?;
    Ok(format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")))
}

/// One GET. The sign-in travels to curl over stdin, never in its arguments.
fn get(sign_in: &SignIn, path: &str) -> Result<Value> {
    let config = format!(
        "url = {}\nheader = {}\nheader = {}\n",
        quoted(&format!("{API}{path}"))?,
        quoted(&format!("Authorization: Bearer {}", sign_in.token))?,
        quoted(&format!("x-organization-uuid: {}", sign_in.organization))?,
    );
    let mut child = Command::new("curl")
        .args([
            "-q", "--silent", "--show-error", "--connect-timeout", "5", "--max-time", "120",
            "--max-filesize", REPLY_LIMIT, "--proto", "=https",
            "--header", "anthropic-version: 2023-06-01",
            "--header", "anthropic-beta: ccr-byoc-2025-07-29",
            "--header", "accept: application/json",
            "--write-out", "\n%{http_code}", "--config", "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "reading a linked chat requires curl (an installer prerequisite)")?;
    let sent = child.stdin.take().ok_or("missing HTTP input")?.write_all(config.as_bytes());
    let output = child.wait_with_output()?;
    sent?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.lines().rev().find_map(|line| line.strip_prefix("curl: ("))
            .map_or_else(|| format!("curl {}", output.status), |line| format!("curl ({line}"));
        return Err(format!("Claude could not be reached ({detail}); check your connection and try again").into());
    }
    let mut reply = output.stdout;
    let end = reply.iter().rposition(|&byte| byte == b'\n').ok_or("missing HTTP status")?;
    let status: u16 = std::str::from_utf8(&reply[end + 1..])?.parse()?;
    reply.truncate(end);
    match status {
        200 => serde_json::from_slice(&reply)
            .map_err(|_| "Claude returned a reply that is not JSON; the chat was not read".into()),
        401 | 403 => Err(SIGN_IN_EXPIRED.into()),
        404 => Err(CHAT_NOT_FOUND.into()),
        _ => Err(format!("Claude answered HTTP {status}; the chat was not read").into()),
    }
}

fn session(sign_in: &SignIn, reference: &Reference) -> Result<Value> {
    let found = match reference {
        // A session read by its own ID arrives wrapped; one found by its chat tag does not.
        Reference::Session(id) => {
            let mut reply = get(sign_in, &format!("/v1/code/sessions/{id}"))?;
            if reply["response_shape"].is_object() { reply["response_shape"].take() } else { reply }
        }
        Reference::Chat(id) => get(sign_in, &format!("/v1/code/sessions?tags=bard-conversation:{id}"))?
            ["data"].get(0).cloned().unwrap_or(Value::Null),
    };
    require(found["id"].as_str().is_some_and(|id| id.starts_with("cse_")), CHAT_NOT_FOUND)?;
    Ok(found)
}

/// What identifies the chat to its owner. These fields stay the same between two reads of
/// an unchanged chat, so both reads produce the same bytes and the same upload.
fn identity(reference: &Reference, session: &Value) -> Value {
    let chat = match reference {
        Reference::Chat(id) => Some(id.clone()),
        Reference::Session(_) => session["tags"].as_array().and_then(|tags| tags.iter()
            .find_map(|tag| tag.as_str()?.strip_prefix("bard-conversation:").map(str::to_owned))),
    };
    json!({
        "source": "claude.ai",
        "chat_id": chat,
        "link": chat.as_ref().map(|id| format!("https://claude.ai/chat/{id}")),
        "session_id": session["id"],
        "title": session["title"],
        "created_at": session["created_at"],
        "last_event_at": session["last_event_at"],
    })
}

/// Read the title and dates only, for showing a person what a link names before any upload.
pub fn describe(reference: &Reference) -> Result<Value> {
    let sign_in = sign_in()?;
    Ok(identity(reference, &session(&sign_in, reference)?))
}

/// Download the whole chat, oldest event first, as JSON Lines: one line naming the chat,
/// then one line per stored event exactly as Claude returned it.
pub fn fetch(reference: &Reference) -> Result<Chat> {
    let sign_in = sign_in()?;
    let session = session(&sign_in, reference)?;
    let id = session["id"].as_str().ok_or(CHAT_NOT_FOUND)?;
    let mut identity = identity(reference, &session);
    let mut events = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut path = format!("/v1/code/sessions/{id}/events?limit={PAGE}&sort_order=asc");
        if let Some(cursor) = &cursor {
            path.push_str(&format!("&cursor={cursor}"));
        }
        let page = get(&sign_in, &path)?;
        let data = page["data"].as_array().ok_or("Claude returned no event list; the chat was not read")?;
        events.extend(data.iter().cloned());
        let next = page["resume_cursor"].as_str()
            .filter(|next| next.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'=')))
            .map(str::to_owned);
        // A short page is the end. An unchanged cursor would repeat the same page forever.
        if data.len() < PAGE || next.is_none() || next == cursor {
            break;
        }
        cursor = next;
    }
    require(!events.is_empty(), "That chat has no stored messages yet; nothing was uploaded")?;
    let mut bytes = serde_json::to_vec(&json!({"type": "linked-chat", "chat": identity}))?;
    bytes.push(b'\n');
    for event in &events {
        serde_json::to_writer(&mut bytes, event)?;
        bytes.push(b'\n');
    }
    identity["events"] = json!(events.len());
    identity["bytes"] = json!(bytes.len());
    Ok(Chat { summary: identity, bytes })
}
