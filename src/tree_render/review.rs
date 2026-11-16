//! One self-contained review surface for offline files and authenticated hosting.
use super::*;
use crate::account::{SyncAccess, TransportFailure};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::time::{Duration, Instant};

pub(super) fn page(payload: &Value) -> Result<String> {
    let mut style = include_str!("assets/tree_delta_view.css").to_owned();
    for (name, bytes) in [
        ("JetBrainsMono-Light.woff2", include_bytes!("assets/fonts/JetBrainsMono-Light.woff2").as_slice()),
        ("JetBrainsMono-LightItalic.woff2", include_bytes!("assets/fonts/JetBrainsMono-LightItalic.woff2").as_slice()),
        ("JetBrainsMono-Regular.woff2", include_bytes!("assets/fonts/JetBrainsMono-Regular.woff2").as_slice()),
        ("JetBrainsMono-Medium.woff2", include_bytes!("assets/fonts/JetBrainsMono-Medium.woff2").as_slice()),
    ] {
        style = style.replace(&format!("fonts/{name}"), &format!("data:font/woff2;base64,{}", STANDARD.encode(bytes)));
    }
    let data = serde_json::to_string(payload)?.replace('<', "\\u003c");
    Ok(include_str!("assets/review.html").replace("/*STYLE*/", &style)
        .replace("/*DIFF*/", include_str!("assets/diff.min.js"))
        .replace("/*MODEL*/", include_str!("assets/review_data.js"))
        .replace("/*VIEW*/", include_str!("assets/tree_delta_view.js"))
        .replace("/*DATA*/null", &data))
}

/// The hosted save, retries included, ends within this time and leaves the local HTML.
const UPLOAD_LIMIT: Duration = Duration::from_secs(30);
/// Seconds to wait before each retry of an upload that stalled, could not connect or met a
/// server failure. A duplicate stored render is harmless: renders are private and expire.
const RETRY_WAITS: [u64; 3] = [1, 2, 3];
/// A retry is skipped when less time than this would remain for it.
const SHORTEST_TRY: Duration = Duration::from_secs(2);

enum Attempt {
    Saved(Value),
    Retryable(String),
    Final(String),
}

/// Saves the snapshot privately. The error is the reason the agent receives as `hosted`.
pub(super) fn upload(payload: &Value) -> std::result::Result<Value, String> {
    let deadline = Instant::now() + UPLOAD_LIMIT;
    let mut access = None;
    let mut waits = RETRY_WAITS.iter().map(|seconds| Duration::from_secs(*seconds));
    let mut tries = 0;
    loop {
        tries += 1;
        let cause = match attempt(&mut access, payload, deadline) {
            Attempt::Saved(receipt) => return Ok(receipt),
            Attempt::Final(cause) => cause,
            Attempt::Retryable(cause) => match waits.next() {
                Some(wait) if Instant::now() + wait + SHORTEST_TRY <= deadline => {
                    std::thread::sleep(wait);
                    continue;
                }
                _ => cause,
            },
        };
        return Err(format!("unavailable after {tries} {}: {cause}",
            if tries == 1 { "try" } else { "tries" }));
    }
}

fn attempt(access: &mut Option<SyncAccess>, payload: &Value, deadline: Instant) -> Attempt {
    let reply = (|| -> Result<(u16, Vec<u8>)> {
        if access.is_none() {
            *access = Some(SyncAccess::checks()?);
        }
        access.as_mut().ok_or("account access unavailable")?.post_until("/api/renders", payload, deadline)
    })();
    let (status, body) = match reply {
        Ok(reply) => reply,
        Err(error) => return match error.downcast_ref::<TransportFailure>() {
            Some(failure) => Attempt::Retryable(failure.0.clone()),
            // Sign-in failures already name their step.
            None => Attempt::Final(error.to_string()),
        },
    };
    // A failing proxy answers with an HTML page; only the service's JSON carries a reason.
    let receipt: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if status == 201 && receipt["render_id"].is_string() && receipt["review_url"].is_string() {
        return Attempt::Saved(receipt);
    }
    let mut cause = format!("HTTP {status}");
    if let Some(error) = receipt["error"].as_str() {
        cause.push_str(&format!(" {error}"));
    }
    if let Some(detail) = receipt["detail"].as_str() {
        cause.push_str(&format!(": {detail}"));
    }
    match status {
        408 | 429 | 500..=599 => Attempt::Retryable(cause),
        // The request already renewed the saved sign-in once.
        401 => Attempt::Final(format!("{cause}. The server rejected the saved sign-in. {}",
            if crate::account::managed() { "Sign in again through your MCP client." }
            else { "Run sifttext login." })),
        200..=299 => Attempt::Final(format!("{cause} without a review link")),
        _ => Attempt::Final(cause),
    }
}
