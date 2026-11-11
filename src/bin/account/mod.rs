//! Browser OAuth is independent of knowledge, installed assets and update locks.
//! curl is an existing installation prerequisite; credentials travel only over stdin.
mod provider;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sifttext_file_graph::Result;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::fd::AsRawFd,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const PLATFORM: &str = "https://app.sifttext.com";

pub fn is_command(command: &str) -> bool {
    matches!(command, "login" | "account" | "logout")
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct Credentials {
    origin: String,
    client_id: String,
    access_token: String,
    refresh_token: String,
    expires_at: u64,
    #[serde(default)]
    resource: String,
    #[serde(default)]
    issuer: String,
}

fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

fn random() -> Result<String> {
    let mut bytes = [0; 32];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

// Exact staging origin only; arbitrary servers must not receive saved credentials.
fn origin() -> Result<String> {
    let origin = std::env::var("SIFTTEXT_AUTH_ORIGIN").unwrap_or_else(|_| PLATFORM.into());
    let loopback = origin
        .strip_prefix("http://127.0.0.1:")
        .and_then(|port| port.parse::<u16>().ok())
        .is_some_and(|port| port != 0);
    if origin != PLATFORM && origin != "https://staging.sifttext.com" && !loopback {
        return Err(
            "authentication origin must be official Platform, staging, or an explicit loopback port"
                .into(),
        );
    }
    Ok(origin)
}

struct Store {
    dir: PathBuf,
    filename: String,
    _lock: File,
}

fn account_resource() -> Result<String> {
    Ok(format!("{}/mcp", origin()?))
}

fn check_provider(credentials: &Credentials) -> Result<()> {
    if credentials.origin != origin()? || credentials.issuer != provider::issuer()? {
        return Err("saved login belongs to another account service or issuer; sign in again".into());
    }
    Ok(())
}

impl Store {
    fn open(resource: &str) -> Result<Self> {
        let issuer = provider::issuer()?;
        let filename = if issuer.is_empty() {
            // Keep existing production credentials and behavior untouched in this local slice.
            "credentials.json".to_owned()
        } else {
            let key = serde_json::to_vec(&(origin()?, issuer, resource))?;
            format!("grant-{:x}.json", Sha256::digest(key))
        };
        let home = fs::canonicalize(std::env::var_os("HOME").ok_or("HOME is not set")?)?;
        let destination = home.join(".config/sifttext/account");
        // Check before creating anything: a Git-backed HOME must remain untouched.
        if destination
            .ancestors()
            .any(|p| p.join(".git").symlink_metadata().is_ok())
        {
            return Err("refusing to store authentication inside a Git repository".into());
        }
        let home_meta = fs::metadata(&home)?;
        if home_meta.uid() != unsafe { libc::geteuid() } || home_meta.mode() & 0o022 != 0 {
            return Err("authentication HOME must be owned and not writable by other users".into());
        }
        let mut dir = home.clone();
        for component in [".config", "sifttext", "account"] {
            dir.push(component);
            if !dir.try_exists()? {
                fs::DirBuilder::new().mode(0o700).create(&dir)?;
            }
            let meta = fs::symlink_metadata(&dir)?;
            if !meta.is_dir()
                || meta.uid() != unsafe { libc::geteuid() }
                || meta.mode() & 0o022 != 0
            {
                return Err("unsafe authentication directory (must be owned, non-symlink and not writable by others)".into());
            }
        }
        let meta = fs::symlink_metadata(&dir)?;
        if meta.mode() & 0o777 != 0o700 {
            return Err("authentication directory must have mode 0700".into());
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(dir.join("lock"))?;
        check_file(&lock)?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("another authentication command is running".into());
        }
        Ok(Self { dir, filename, _lock: lock })
    }

    fn path(&self) -> PathBuf {
        self.dir.join(&self.filename)
    }

    fn load(&self) -> Result<Credentials> {
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(self.path())
            .map_err(|_| "No usable saved login. Run sifttext login.")?;
        check_file(&file)?;
        let mut bytes = Vec::new();
        (&mut file).take(65537).read_to_end(&mut bytes)?;
        if bytes.len() > 65536 {
            return Err("oversized credential file".into());
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| "Invalid saved login. Run sifttext login.".into())
    }

    fn save(&self, credentials: &Credentials) -> Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(&self.dir)?;
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
        serde_json::to_writer(&mut file, credentials)?;
        file.as_file().sync_all()?;
        file.persist(self.path())
            .map_err(|_| "could not save rotated credentials")?;
        File::open(&self.dir)?.sync_all()?;
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        match fs::remove_file(self.path()) {
            Ok(()) => {
                File::open(&self.dir)?.sync_all()?;
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

fn check_file(file: &File) -> Result<()> {
    let meta = file.metadata()?;
    if !meta.is_file()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.nlink() != 1
        || meta.mode() & 0o777 != 0o600
    {
        return Err(
            "authentication file must be owned, regular, not hard-linked and mode 0600".into(),
        );
    }
    Ok(())
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn form(values: &[(&str, &str)]) -> String {
    values
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn config_string(value: &str) -> Result<String> {
    if value.chars().any(char::is_control) {
        return Err("invalid control character in authentication request".into());
    }
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

pub(super) fn http(
    origin: &str,
    path: &str,
    bearer: Option<&str>,
    body: Option<(&str, &str)>,
) -> Result<(u16, Value)> {
    let mut config = format!("url = {}\n", config_string(&format!("{origin}{path}"))?);
    if let Some(token) = bearer {
        config.push_str(&format!(
            "header = {}\n",
            config_string(&format!("Authorization: Bearer {token}"))?
        ));
    }
    if let Some((content_type, data)) = body {
        config.push_str(&format!(
            "header = {}\ndata = {}\n",
            config_string(&format!("Content-Type: {content_type}"))?,
            config_string(data)?
        ));
    }
    let mut child = Command::new("curl")
        .args([
            "-q",
            "--silent",
            "--show-error",
            "--connect-timeout",
            "5",
            "--max-time",
            "20",
            "--max-filesize",
            if path.starts_with("/sync/v1/") { "46137344" } else { "1048576" },
            "--proxy",
            "",
            "--proto",
            "=http,https",
            "--write-out",
            "\n%{http_code}",
            "--config",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "authentication requires curl (an installer prerequisite)")?;
    let sent = child
        .stdin
        .take()
        .ok_or("missing HTTP input")?
        .write_all(config.as_bytes());
    let output = child.wait_with_output()?;
    sent?;
    if !output.status.success() {
        return Err("Platform request failed; check your connection and try again".into());
    }
    let text = std::str::from_utf8(&output.stdout).map_err(|_| "invalid Platform response")?;
    let (body, status) = text.rsplit_once('\n').ok_or("missing HTTP status")?;
    let status = status.parse()?;
    let value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(body).map_err(|_| "invalid Platform JSON response")?
    };
    Ok((status, value))
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Platform response missing {key}").into())
}

fn oauth_error_name(value: Option<&str>) -> &'static str {
    // Never print arbitrary provider descriptions, credentials or callback contents.
    match value {
        Some("invalid_client") => "invalid_client (client registration/configuration)",
        Some("invalid_scope") => "invalid_scope (requested client scopes)",
        Some("invalid_target") => "invalid_target (resource audience configuration)",
        Some("invalid_grant") => "invalid_grant (expired/revoked grant or callback mismatch)",
        Some("unauthorized_client") => "unauthorized_client (client flow configuration)",
        Some("access_denied") => "access_denied (authorization not granted)",
        Some("temporarily_unavailable") => "temporarily_unavailable",
        _ => "unrecognized authorization error",
    }
}

fn tokens(origin: &str, client_id: &str, fields: &[(&str, &str)]) -> Result<Credentials> {
    let provider = provider::discover()?;
    let (status, value) = http(
        &provider.token,
        "",
        None,
        Some(("application/x-www-form-urlencoded", &form(fields))),
    )?;
    if status != 200 {
        return Err(format!("Token endpoint returned HTTP {status}: {}. Use the matching login command to retry.", oauth_error_name(value["error"].as_str())).into());
    }
    if !text(&value, "token_type")?.eq_ignore_ascii_case("bearer") {
        return Err("unsupported OAuth token type".into());
    }
    let lifetime = value["expires_in"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or("missing token expiry")?;
    Ok(Credentials {
        origin: origin.into(),
        client_id: client_id.into(),
        access_token: text(&value, "access_token")?.into(),
        refresh_token: text(&value, "refresh_token")?.into(),
        expires_at: now()?.saturating_add(lifetime),
        issuer: provider.issuer,
        resource: fields.iter().find(|(key, _)| *key == "resource")
            .map(|(_, value)| (*value).to_owned()).unwrap_or_default(),
    })
}

fn refresh_for(store: &Store, credentials: &mut Credentials, resource: &str) -> Result<()> {
    check_provider(credentials)?;
    if !credentials.issuer.is_empty() && credentials.resource != resource {
        return Err("resource-bound grant cannot change audience; login separately for this resource".into());
    }
    *credentials = tokens(
        &credentials.origin,
        &credentials.client_id,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", &credentials.client_id),
            ("refresh_token", &credentials.refresh_token),
            ("resource", resource),
        ],
    )?;
    // Persist rotation before any later account/network failure loses the new refresh token.
    store.save(credentials)
}

// Keep one login/rotation lease for the complete sync exchange. Renewal never
// silently reloads a different account; sync verifies its stable home before replay.
pub(super) struct SyncAccess {
    store: Store,
    credentials: Credentials,
}
impl SyncAccess {
    pub(super) fn open(resource: &str) -> Result<Self> {
        let store = Store::open(resource)?;
        let mut credentials = store.load().map_err(|_| format!("No saved grant for {resource}. Run sifttext login --resource {resource}."))?;
        check_provider(&credentials)?;
        let saved_resource = if credentials.resource.is_empty() {
            format!("{}/mcp", credentials.origin)
        } else {
            credentials.resource.clone()
        };
        if saved_resource != resource || credentials.expires_at <= now()?.saturating_add(60) {
            refresh_for(&store, &mut credentials, resource)?;
        }
        Ok(Self { store, credentials })
    }
    pub(super) fn token(&self) -> Result<&str> {
        if self.store.load()? != self.credentials {
            return Err("saved login changed during sync; no further transfer attempted".into());
        }
        Ok(&self.credentials.access_token)
    }
    pub(super) fn renew(&mut self, resource: &str) -> Result<()> {
        self.token()?;
        refresh_for(&self.store, &mut self.credentials, resource)
    }
}

fn account(store: &Store, credentials: &mut Credentials, resource: &str) -> Result<Value> {
    check_provider(credentials)?;
    if (!credentials.resource.is_empty() && credentials.resource != resource)
        || credentials.expires_at <= now()?.saturating_add(30) {
        refresh_for(store, credentials, resource)?;
    }
    let workspace = !credentials.issuer.is_empty() && resource != account_resource()?;
    let path = if workspace { "/oauth/hosted-account" } else { "/oauth/account" };
    let endpoint = if workspace {
        resource.strip_suffix("/mcp").ok_or("invalid workspace resource")?.to_owned()
    } else { credentials.origin.clone() };
    let mut response = http(
        &endpoint,
        path,
        Some(&credentials.access_token),
        None,
    )?;
    if response.0 == 401 {
        refresh_for(store, credentials, resource)?;
        response = http(
            &endpoint,
            path,
            Some(&credentials.access_token),
            None,
        )?;
    }
    if response.0 != 200 {
        return Err(format!("Grant saved, but account verification at {endpoint}{path} returned HTTP {}. A 403 can mean this signed-in identity has no enabled SiftText account mapping; a 401 means the resource rejected its token.", response.0).into());
    }
    if !response.1["paid"].is_boolean() {
        return Err("Platform did not confirm subscription status".into());
    }
    Ok(response.1)
}

fn open_browser(url: &str) {
    if std::env::var_os("SIFTTEXT_NO_BROWSER").is_some() {
        return;
    }
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    // Opening is best effort; detached stdio and a separate process group keep terminal use independent.
    use std::os::unix::process::CommandExt;
    match Command::new(opener)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
    {
        Ok(mut child) => {
            thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(_) => {
            let _ = writeln!(
                std::io::stderr(),
                "Could not open your browser; use the URL above."
            );
        }
    }
}

fn decode(value: &str) -> Result<String> {
    let mut bytes = Vec::new();
    let mut input = value.bytes();
    while let Some(byte) = input.next() {
        bytes.push(match byte {
            b'%' => {
                let a = input
                    .next()
                    .and_then(|c| (c as char).to_digit(16))
                    .ok_or("invalid callback encoding")?;
                let b = input
                    .next()
                    .and_then(|c| (c as char).to_digit(16))
                    .ok_or("invalid callback encoding")?;
                (a * 16 + b) as u8
            }
            b'+' => b' ',
            _ => byte,
        });
    }
    Ok(String::from_utf8(bytes)?)
}

fn callback(
    stream: &mut TcpStream,
    authority: &str,
    state: &str,
    expected_issuer: Option<&str>,
    remaining: Duration,
) -> Result<String> {
    let deadline = Instant::now() + remaining;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("browser sign-in timed out".into());
        }
        stream.set_read_timeout(Some(remaining.min(Duration::from_secs(2))))?;
        let mut byte = [0];
        if stream.read(&mut byte)? == 0 || bytes.len() >= 16384 {
            return Err("invalid callback request".into());
        }
        bytes.push(byte[0]);
    }
    let request = std::str::from_utf8(&bytes)?;
    let mut lines = request.split("\r\n");
    let mut first = lines.next().ok_or("missing callback request")?.split(' ');
    if first.next() != Some("GET") {
        return Err("callback must use GET".into());
    }
    let target = first.next().ok_or("missing callback target")?;
    if first.next() != Some("HTTP/1.1") || first.next().is_some() {
        return Err("invalid callback HTTP version".into());
    }
    let hosts: Vec<_> = lines
        .filter_map(|line| line.split_once(':'))
        .filter(|(key, _)| key.eq_ignore_ascii_case("host"))
        .map(|(_, value)| value.trim())
        .collect();
    if hosts != [authority] {
        return Err("callback redirect host mismatch".into());
    }
    let query = target
        .strip_prefix("/callback?")
        .filter(|s| !s.contains('#'))
        .ok_or("callback redirect path mismatch")?;
    let mut fields = BTreeMap::new();
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').ok_or("invalid callback query")?;
        if fields.insert(decode(key)?, decode(value)?).is_some() {
            return Err("duplicate callback field".into());
        }
    }
    if fields.get("state").map(String::as_str) != Some(state) {
        return Err("callback state mismatch".into());
    }
    if let Some(issuer) = expected_issuer {
        if fields.get("iss").map(String::as_str) != Some(issuer) {
            return Err("authorization callback issuer mismatch".into());
        }
    }
    if fields.contains_key("error") {
        return Err(format!("Browser authorization failed: {}", oauth_error_name(fields.get("error").map(String::as_str))).into());
    }
    fields
        .remove("code")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "missing callback code".into())
}

fn login(store: &Store, resource: &str) -> Result<()> {
    let origin = origin()?;
    let provider = provider::discover()?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let authority = listener.local_addr()?.to_string();
    let redirect = format!("http://{authority}/callback");
    let state = random()?;
    let verifier = random()?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let client_id = if let Some(client_id) = &provider.client_id {
        client_id.clone()
    } else {
        let registration = json!({"client_name":"SiftText CLI", "redirect_uris":[redirect], "token_endpoint_auth_method":"none", "grant_types":["authorization_code","refresh_token"], "response_types":["code"]});
        let (status, value) = http(
            provider.registration.as_deref().ok_or("missing client registration endpoint")?,
            "", None, Some(("application/json", &registration.to_string())),
        )?;
        if status != 200 && status != 201 {
            return Err("could not register SiftText CLI with Platform".into());
        }
        text(&value, "client_id")?.to_owned()
    };
    let client_id = client_id.as_str();
    let scope = if provider.issuer.is_empty() { "mcp" }
        else if resource == account_resource()? { "openid account:read" }
        else { "openid mcp" };
    let url = format!(
        "{}?{}", provider.authorization,
        form(&[
            ("response_type", "code"),
            ("client_id", client_id),
            ("redirect_uri", &redirect),
            ("state", &state),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("scope", scope),
            ("resource", resource),
        ])
    );
    writeln!(
        std::io::stdout(),
        "Open this URL to sign in (Ctrl-C cancels):\n{url}"
    )?;
    std::io::stdout().flush()?;
    open_browser(&url);
    let seconds = std::env::var("SIFTTEXT_AUTH_TIMEOUT_SECONDS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|s| *s > 0 && *s <= 600)
        .unwrap_or(180);
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let code = loop {
        if Instant::now() >= deadline {
            return Err("browser sign-in timed out; run sifttext login to retry".into());
        }
        match listener.accept() {
            Ok((mut stream, peer)) => {
                if !peer.ip().is_loopback() {
                    continue;
                }
                let result = callback(
                    &mut stream,
                    &authority,
                    &state,
                    if provider.verify_callback_issuer { Some(&provider.issuer) } else { None },
                    deadline.saturating_duration_since(Instant::now()),
                );
                let (status, body) = if result.is_ok() {
                    (
                        "200 OK",
                        "Authorization received. Return to your terminal for server verification.",
                    )
                } else {
                    (
                        "400 Bad Request",
                        "Invalid sign-in callback. Return to your terminal and retry.",
                    )
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
                break result?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20))
            }
            Err(e) => return Err(e.into()),
        }
    };
    drop(listener);
    let mut credentials = tokens(
        &origin,
        client_id,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("code", &code),
            ("redirect_uri", &redirect),
            ("code_verifier", &verifier),
            ("resource", resource),
        ],
    )?;
    store.save(&credentials)?;
    let account = account(store, &mut credentials, resource)?;
    show_account(&account)?;
    if !provider.issuer.is_empty() || account["paid"] == true {
        writeln!(std::io::stdout(), "Authentication successful. Service entitlements are enforced separately.")?;
        Ok(())
    } else {
        writeln!(
            std::io::stdout(),
            "Payment pending or subscription inactive. Complete payment on Platform, then run sifttext login again.\n{origin}/"
        )?;
        open_browser(&format!("{origin}/"));
        Err("paid activation has not been confirmed; offline knowledge remains available".into())
    }
}

fn show_account(account: &Value) -> Result<()> {
    // Serialize only known account fields; never dump a token-bearing response.
    let output = json!({"id":account["id"], "email":account["email"],
        "subscription_status":account["subscription_status"], "paid":account["paid"],
        "subscription_plan":account["subscription_plan"],
        "guide":super::sync::guide_path()?,
        "next":"Read the installed account/sync guide and inspect local consent before setup. Login and payment do not authorize upload."});
    writeln!(
        std::io::stdout(),
        "{}",
        serde_json::to_string(&output)?
    )?;
    Ok(())
}

pub fn run(command: &str, selected_resource: Option<&str>) -> Result<()> {
    let default_resource = account_resource()?;
    let resource = selected_resource.unwrap_or(&default_resource);
    if selected_resource.is_some() {
        if provider::issuer()?.is_empty() {
            return Err("explicit resource login is available only for the isolated OIDC configuration".into());
        }
        if resource != default_resource && resource != super::sync::resource()? {
            return Err("resource must match the configured account or workspace service".into());
        }
    }
    let store = Store::open(resource)?;
    match command {
        "login" => login(&store, resource),
        "account" => {
            let mut credentials = store.load()?;
            show_account(&account(&store, &mut credentials, resource)?)
        }
        "logout" => {
            let credentials = store.load().ok();
            // Local removal comes first, even when offline or the saved file is malformed.
            store.clear()?;
            let revoked = credentials
                .filter(|c| check_provider(c).is_ok())
                .is_some_and(|c| {
                    let Ok(provider) = provider::discover() else { return false; };
                    http(
                        &provider.revocation,
                        "",
                        None,
                        Some((
                            "application/x-www-form-urlencoded",
                            &form(&[
                                ("token", &c.refresh_token),
                                ("client_id", &c.client_id),
                                ("token_type_hint", "refresh_token"),
                            ]),
                        )),
                    )
                    .is_ok_and(|(status, _)| (200..300).contains(&status))
                });
            writeln!(
                std::io::stdout(),
                "Selected grant removed locally; other saved grant files are unchanged. {}",
                if revoked {
                    "Remote credentials revoked."
                } else {
                    "Remote revocation could not be confirmed (offline or no saved login)."
                }
            )?;
            Ok(())
        }
        _ => unreachable!(),
    }
}
