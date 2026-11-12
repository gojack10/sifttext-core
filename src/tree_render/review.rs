//! One self-contained review surface for offline files and authenticated hosting.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};

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

pub(super) fn upload(payload: &Value) -> Result<Value> {
    let mut access = crate::account::SyncAccess::checks()?;
    let (status, receipt) = access.request("POST", "/api/renders", Some(payload))?;
    require(status == 201 && receipt["render_id"].is_string() && receipt["review_url"].is_string(), "hosted review unavailable")?;
    Ok(receipt)
}
