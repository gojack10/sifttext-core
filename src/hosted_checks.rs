//! Account-authenticated one-shot checking and the binary-owned publication gate.
//! Public receipts are diagnostics, never a hosted acceptance credential.
use crate::{
    Result,
    account::SyncAccess,
    git_snapshot::{GitReader, GitView},
    require,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    thread,
    time::{Duration, Instant},
};

const ROOT: &str = "/api/checks";
const MAX_INPUT: u64 = crate::account::CHECK_REQUEST_LIMIT as u64;
const RETENTION: &str = "Your submitted text stays with the saved result until you delete it.\nDelete: sifttext check-result ID --delete";

fn response(
    access: &mut SyncAccess,
    method: &str,
    path: &str,
    body: Option<&Value>,
    announce: bool,
) -> Result<Value> {
    let receipt = if method == "POST" && path == ROOT {
        Some(access.prepare_check_request(body.ok_or("missing check body")?)?)
    } else {
        None
    };
    if announce && let Some((id, _)) = &receipt {
        let mut out = std::io::stdout().lock();
        writeln!(out, "Request: {id}")?;
        out.flush()?;
    }
    let recover = |error: Box<dyn std::error::Error>| -> Box<dyn std::error::Error> {
        if let Some((id, path)) = &receipt {
            format!("{error}. Retrieve with sifttext check-result {id}; do not start another check. Local receipt: {}", path.display()).into()
        } else {
            error
        }
    };
    let (mut status, mut value) = access.request_idempotent(method, path, body,
        receipt.as_ref().map(|(id, _)| id.as_str())).map_err(&recover)?;
    // A transient response can fail after dispatch while the saved job is running.
    // Resolve its pre-dispatch identity with GET, never another execution.
    if status == 503 && value["error"] == "service_unavailable"
        && value["request_id"].as_str().is_none()
        && let Some((id, _)) = &receipt
    {
        (status, value) = access.request("GET", &format!("{ROOT}/{id}"), None).map_err(&recover)?;
    }
    let start = Instant::now();
    while status == 202 {
        let id = request_id(&value)?.to_owned();
        if start.elapsed() > Duration::from_secs(600) {
            return Err(format!(
                "Retrieve this running check with sifttext check-result {id} (no charge)."
            )
            .into());
        }
        thread::sleep(Duration::from_secs(
            value["retry_after"].as_u64().unwrap_or(2).clamp(1, 10),
        ));
        match access.request("GET", &format!("{ROOT}/{id}"), None) {
            Ok(next) => (status, value) = next,
            Err(error) => {
                return Err(format!(
                    "{error}\n\nRecover this request with sifttext check-result {id} instead of starting another check."
                )
                .into());
            }
        }
    }
    if method == "DELETE" && status == 204 {
        return Ok(Value::Null);
    }
    if status == 200
        || value["status"] == "failed"
        || status == 402
        || status == 429
        || status == 503
    {
        value["http_status"] = json!(status);
        if announce && receipt.is_some() {
            value["request_announced"] = json!(true);
        }
        return Ok(value);
    }
    let code = value["error"].as_str().unwrap_or("invalid_response");
    if code == "request_not_recorded" {
        return Err("No request is recorded for this account and client ID. It may not have reached the service. No check was started by this lookup.".into());
    }
    let detail = value["detail"].as_str().unwrap_or("No further service detail.");
    Err(recover(format!("Hosted check unavailable (HTTP {status}, {code}): {detail} Retain your input for recovery.").into()))
}

fn request_id(value: &Value) -> Result<&str> {
    let id = value["request_id"]
        .as_str()
        .ok_or("missing check request ID")?;
    require(
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "invalid check request ID",
    )?;
    Ok(id)
}

fn completed(value: &Value) -> bool {
    value["http_status"] == 200
        && value["status"] == "completed"
        && value["complete"] == true
        && value["passed"].is_boolean()
        && value["output"].is_string()
        && value["credits_used"].is_number()
        && value["balance"]["total"].is_number()
        && request_id(value).is_ok()
}

fn input(path: &Path) -> Result<String> {
    let mut data = String::new();
    if path == Path::new("-") {
        std::io::stdin().lock().take(MAX_INPUT + 1).read_to_string(&mut data)?;
    } else {
        fs::File::open(path)
            .map_err(|error| format!("Cannot read {}: {error}. This argument is a file path. Pipe text to sifttext slop-check - or sifttext epistemic-check -.", path.display()))?
            .take(MAX_INPUT + 1)
            .read_to_string(&mut data)?;
    }
    require(
        data.len() as u64 <= MAX_INPUT,
        "Check input exceeds 128 KiB. Select a smaller input to leave room for JSON fields and escaping.",
    )?;
    require(!data.trim().is_empty(), "check input is empty")?;
    Ok(data)
}

fn credit(value: &Value) -> String {
    let balance = &value["balance"];
    if balance["total"].is_number() {
        if value["retrieved"] == true {
            return format!(
                "Credits: original usage {}, balance at result {} (retrieval adds no charge)",
                value["credits_used"], balance["total"]
            );
        }
        format!(
            "Credits: used {} | remaining {} | monthly {} | purchased {} | available for slop {}",
            if value["reused"] == true {
                Some(serde_json::Number::from(0))
            } else {
                value["credits_used"].as_number().cloned()
            }
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "0".into()),
            balance["total"],
            balance["monthly"],
            balance["purchased"],
            balance["slop_available"]
        )
    } else {
        "Credits: usage and balance unconfirmed (retrieve any completed request before retrying)."
            .into()
    }
}

/// Exact checker prose/reason codes, followed by ID and credits. No invented explanation.
pub fn render(value: &Value) -> String {
    render_result(value, true)
}

fn render_result(value: &Value, disclose_retention: bool) -> String {
    let mut lines = Vec::new();
    let output = value["output"].as_str().filter(|s| !s.trim().is_empty());
    if let Some(output) = output {
        lines.push(output.trim_end().to_owned());
    }
    if completed(value) {
        // The server's verdict and findings are authoritative display text.
        // Historical publication records omit output and still need a verdict.
        if output.is_none() {
            let verdict = value["result"]["verdict"].as_str().unwrap_or(
                if value["passed"] == true { "passed" } else { "findings" },
            );
            lines.push(format!("Check: {verdict}"));
        }
        if value["reused"] == true || value["retrieved"] == true {
            lines.push("Saved result (no new charge).".into());
        }
    } else {
        lines.push(format!(
            "Check: unavailable ({})",
            value["error"].as_str().unwrap_or("incomplete response")
        ));
        if let Some(detail) = value["detail"].as_str().filter(|text| !text.trim().is_empty()) {
            lines.push(detail.trim().to_owned());
        }
    }
    if disclose_retention {
        lines.push(RETENTION.into());
    }
    if value["request_announced"] != true && let Ok(id) = request_id(value) {
        lines.push(format!("Request: {id}"));
    }
    lines.push(credit(value));
    lines.join("\n")
}

pub fn is_command(command: &str) -> bool {
    matches!(
        command,
        "slop-check" | "epistemic-check" | "check-result" | "credits"
    )
}

fn checkout(access: &mut SyncAccess, saved_key: Option<&str>) -> Result<()> {
    let key = saved_key.map(str::to_owned).map(Ok).unwrap_or_else(crate::account::random)?;
    // A key is not a credential. Print the exact recovery command before sending:
    // a lost response must reuse this checkout, not create another purchase.
    request_id(&json!({"request_id":key}))?;
    {
        let mut out = std::io::stdout().lock();
        writeln!(out, "One-time $10 checkout. Complete payment yourself in Stripe.")?;
        writeln!(out, "Checkout request: {key}")?;
        writeln!(out, "Retry this checkout: sifttext credits --buy-10 {key}")?;
        out.flush()?;
    }
    let (status, value) = access.request_idempotent(
        "POST", &format!("{ROOT}/checkout"), Some(&json!({})), Some(&key),
    )?;
    require(status == 200, &format!("Checkout unavailable (HTTP {status}, {}).\n\nRecover this checkout with the request key printed above.",
        value["error"].as_str().unwrap_or("invalid_response")))?;
    let url = value["url"].as_str().ok_or("Retry with the same request key because the server did not return a checkout URL.")?;
    require(url.starts_with("https://checkout.stripe.com/")
        && url.bytes().all(|b| b.is_ascii_graphic()),
        "The server returned an invalid checkout URL. Retain the request key for recovery.")?;
    require(value["amount_usd"] == "10.00"
        && matches!(value["credits"].as_u64(), Some(1000 | 2500)),
        "Unexpected checkout price or credit amount. Do not pay. Retain the request key for review.")?;
    let mut out = std::io::stdout().lock();
    writeln!(out, "Open this URL to complete the purchase:\n{url}")?;
    writeln!(out, "Credits: {} after confirmed $10 payment. This command does not pay or enable automatic top-ups.", value["credits"])?;
    Ok(())
}

/// A failed checker command whose findings/error and credits were already written.
/// The CLI must preserve the nonzero exit without appending a second diagnostic.
#[derive(Debug)]
pub struct ReportedCheckFailure;

impl std::fmt::Display for ReportedCheckFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("check findings or failure reported above")
    }
}

impl std::error::Error for ReportedCheckFailure {}

pub fn run(args: &[String]) -> Result<()> {
    let mut access = SyncAccess::checks()?;
    let value = match args {
        [cmd, file] if cmd == "slop-check" || cmd == "epistemic-check" => {
            let text = input(Path::new(file))?;
            response(&mut access, "POST", ROOT, Some(&json!({"checker":if cmd == "slop-check" {"slop"} else {"epistemic"},"text":text})), true)?
        }
        [cmd, flag, file] if cmd == "epistemic-check" && flag == "--packet" => {
            let packet: Value = serde_json::from_str(&input(Path::new(file))?)?;
            response(&mut access, "POST", ROOT, Some(&json!({"checker":"epistemic", "packet":packet})), true)?
        }
        [cmd, id] if cmd == "check-result" => {
            request_id(&json!({"request_id":id}))?;
            let mut value = response(&mut access, "GET", &format!("{ROOT}/{id}"), None, false)?;
            value["retrieved"] = json!(true);
            value
        }
        [cmd, id, flag] if cmd == "check-result" && flag == "--delete" => {
            request_id(&json!({"request_id":id}))?;
            let deleted = response(&mut access, "DELETE", &format!("{ROOT}/{id}"), None, false)?;
            require(deleted.is_null(), "result deletion not confirmed")?;
            writeln!(std::io::stdout().lock(), "Deleted check result {id}. The charge and financial debit metadata are retained.")?;
            return Ok(());
        }
        [cmd, flag] if cmd == "credits" && flag == "--buy-10" => {
            return checkout(&mut access, None);
        }
        [cmd, flag, key] if cmd == "credits" && flag == "--buy-10" => {
            return checkout(&mut access, Some(key));
        }
        [cmd] if cmd == "credits" => {
            let value = response(&mut access, "GET", &format!("{ROOT}/balance"), None, false)?;
            // The balance route returns a bare balance object.
            if !value["total"].is_number() {
                return Err(format!("Credit balance unavailable: {} ({}).",
                    value["detail"].as_str().unwrap_or("The service did not return a balance"),
                    value["error"].as_str().unwrap_or("invalid_response")).into());
            }
            if let Some(end) = value["period_end"].as_str() {
                writeln!(std::io::stdout().lock(), "Monthly credits renew: {end}")?;
            }
            writeln!(std::io::stdout().lock(), "{}", credit(&json!({"balance":value,"credits_used":0})))?;
            return Ok(());
        }
        _ => return Err("usage:\n  sifttext slop-check FILE|-\n  sifttext epistemic-check FILE|- | --packet FILE\n  sifttext check-result ID [--delete]\n  sifttext credits [--buy-10 [REQUEST_KEY]]".into()),
    };
    let mut out = std::io::stdout().lock();
    if !completed(&value) {
        writeln!(out, "Check did not complete. Retain input and retrieve the request if available.")?;
    }
    writeln!(out, "{}", render(&value))?;
    out.flush()?;
    if completed(&value) && value["passed"] == true {
        Ok(())
    } else {
        Err(ReportedCheckFailure.into())
    }
}

/// Called only on validated immutable final input, before conditional ref publication.
/// Changed node markdown is the entire upload allowlist; no artifacts or unrelated nodes.
pub(crate) fn publication(reader: &mut GitReader, before: Option<&GitView>, after: &GitView) -> Value {
    match SyncAccess::saved_login_exists() {
        Ok(false) => return json!({"status":"not_required","reason":"logged_out"}),
        Err(error) => return unavailable(error),
        Ok(true) => (),
    }
    match SyncAccess::checks() {
        Ok(mut access) => publication_with_access(reader, before, after, &mut access),
        Err(error) => unavailable(error),
    }
}

fn unavailable(error: Box<dyn std::error::Error>) -> Value {
    json!({"status":"unchecked","reason":"service_unavailable","detail":error.to_string(),"uncertain_usage":true})
}

/// Gate only the already-selected independent publication repository. The caller
/// owns this auth lease and must restore its sync resource and reverify home.
pub fn sync_publication(
    policy: &crate::Policy,
    repo: &Path,
    baseline: Option<&str>,
    candidate: &str,
    access: &mut SyncAccess,
) -> Value {
    let mut read = || -> Result<Value> {
        let mut reader = GitReader::new(policy, repo)?;
        let before = baseline.map(|sha| reader.view(sha)).transpose()?;
        let after = reader.view(candidate)?;
        Ok(publication_with_access(
            &mut reader,
            before.as_deref(),
            &after,
            access,
        ))
    };
    match read() {
        Ok(value) => value,
        Err(error) => unavailable(error),
    }
}

fn publication_with_access(
    reader: &mut GitReader,
    before: Option<&GitView>,
    after: &GitView,
    access: &mut SyncAccess,
) -> Value {
    let mut results = Vec::new();
    let mut attempt = || -> Result<Value> {
        let (status, account) = access.request("GET", &format!("{ROOT}/balance"), None)?;
        require(
            status == 200 && account["paid"].is_boolean(),
            &format!("Account balance request failed (HTTP {status}, {}): {}",
                account["error"].as_str().unwrap_or("invalid_account_response"),
                account["detail"].as_str().unwrap_or("The service did not confirm subscription status.")),
        )?;
        if account["paid"] != true {
            return Ok(json!({"status":"not_required","reason":"free_account"}));
        }
        for (id, node) in &after.nodes {
            if before
                .and_then(|view| view.nodes.get(id))
                .is_some_and(|old| old.raw_hash == node.raw_hash)
            {
                continue;
            }
            let text = String::from_utf8(reader.bytes(after, id)?)?;
            let value = response(
                access,
                "POST",
                ROOT,
                Some(&json!({"checker":"epistemic", "text":text, "context":{"node_id":id}})),
                false,
            )?;
            let passed = completed(&value) && value["passed"] == true;
            let findings = completed(&value) && value["passed"] == false;
            results.push(json!({"node":id,"response":value}));
            if findings {
                return Ok(json!({"status":"findings","results":results}));
            }
            if !passed {
                return Ok(
                    json!({"status":"unchecked","reason":"checker_unavailable_or_no_credits","results":results}),
                );
            }
        }
        Ok(if results.is_empty() {
            json!({"status":"not_required","reason":"no_changed_node_text"})
        } else {
            json!({"status":"passed","results":results})
        })
    };
    match attempt() {
        Ok(value) => value,
        Err(error) => {
            json!({"status":"unchecked","reason":"service_unavailable","detail":error.to_string(),"uncertain_usage":true,"results":results})
        }
    }
}

/// Keep publication diagnostics without duplicating hosted quoted text or input.
/// Deleting the hosted result leaves only non-content local status/charge metadata.
pub fn publication_record(check: &Value) -> Value {
    let mut record = check.clone();
    record["historical"] = json!(true);
    if let Some(results) = record["results"].as_array_mut() {
        for item in results {
            if let Some(response) = item["response"].as_object_mut() {
                response.retain(|key, _| {
                    matches!(
                        key.as_str(),
                        "request_id"
                            | "http_status"
                            | "checker"
                            | "checker_version"
                            | "status"
                            | "complete"
                            | "passed"
                            | "reused"
                            | "credits_used"
                            | "balance"
                            | "error"
                    )
                });
                response.insert("output".into(), json!(""));
                response.insert("retrieved".into(), json!(true));
            }
        }
    }
    record
}

pub fn render_publication(check: &Value) -> String {
    let mut lines = Vec::new();
    match check["status"].as_str() {
        Some("passed") => (),
        Some("findings") => lines.push("Not accepted. Draft preserved.".into()),
        Some("not_required") if check["reason"] == "no_changed_node_text" => lines.push("Epistemic submit check: no changed node text.".into()),
        Some("not_required") => lines.push("Epistemic submit check: not required for logged-out/free local use.".into()),
        _ => lines.push("Epistemic submit check: unchecked (no credits or service unavailable). To check these nodes later, run epistemic-check explicitly.".into()),
    }
    if let Some(detail) = check["detail"].as_str() {
        lines.push(detail.to_owned());
    }
    let mut last_response = None;
    let mut used = 0.0;
    let mut retrieved = false;
    if let Some(results) = check["results"].as_array() {
        for item in results {
            lines.push(format!(
                "Node: {}",
                item["node"].as_str().unwrap_or("unknown")
            ));
            let text = render_result(&item["response"], false);
            if let Some((body, _)) = text.rsplit_once('\n') {
                lines.push(body.into());
                let response = &item["response"];
                if response["reused"] != true {
                    used += response["credits_used"].as_f64().unwrap_or(0.0);
                }
                retrieved |= response["retrieved"] == true;
                last_response = Some(response);
            }
        }
    }
    if let Some(response) = last_response {
        lines.push(RETENTION.into());
        if check["historical"] == true {
            lines.push(format!(
                "Credits: balance at last recorded result {} (no new check or charge)",
                response["balance"]["total"]
            ));
            return lines.join("\n");
        }
        if check["uncertain_usage"] == true {
            lines.push(format!("Credits: confirmed usage {used}, balance at last confirmed result {} (later request usage unconfirmed)", response["balance"]["total"]));
            return lines.join("\n");
        }
        if retrieved {
            lines.push(format!("Credits: original request usage {used}, balance at last result {} (retrieval adds no charge)", response["balance"]["total"]));
            return lines.join("\n");
        }
        let mut total = response.clone();
        total["credits_used"] = json!((used * 1_000_000.0).round() / 1_000_000.0);
        total["reused"] = json!(false);
        lines.push(credit(&total));
    } else if check["status"] == "unchecked" {
        lines.push("Credits: usage and balance unconfirmed (retrieve any completed request before retrying).".into());
    }
    lines.join("\n")
}
