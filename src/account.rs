//! Browser OAuth is independent of knowledge, installed assets and update locks.
//! curl is an existing installation prerequisite; credentials travel only over stdin.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use crate::Result;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::fd::AsRawFd,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const PLATFORM: &str = "https://app.sifttext.com";
// The checker service bounds the complete UTF-8 JSON body, not just source text.
pub(crate) const CHECK_REQUEST_LIMIT: usize = 128 * 1024;

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
}

impl Credentials {
    fn load(dir: &Path) -> Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(dir.join("credentials.json"))
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
}

fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

pub(crate) fn random() -> Result<String> {
    let mut bytes = [0; 32];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

// The override is intentionally loopback-only, for local integration tests/development.
fn origin() -> Result<String> {
    let origin = std::env::var("SIFTTEXT_AUTH_ORIGIN").unwrap_or_else(|_| PLATFORM.into());
    let loopback = origin
        .strip_prefix("http://127.0.0.1:")
        .and_then(|port| port.parse::<u16>().ok())
        .is_some_and(|port| port != 0);
    if origin != PLATFORM && !loopback {
        return Err(
            "authentication origin must be the official Platform or an explicit loopback port"
                .into(),
        );
    }
    Ok(origin)
}

struct Store {
    dir: PathBuf,
    _lock: File,
}

impl Store {
    fn open() -> Result<Self> {
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
        // Managed requests have no rotating login file. Their independent
        // create-new recovery receipts do not need the native login lease.
        if !managed() && unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("another authentication command is running".into());
        }
        Ok(Self { dir, _lock: lock })
    }

    fn path(&self) -> PathBuf {
        self.dir.join("credentials.json")
    }

    fn load(&self) -> Result<Credentials> {
        Credentials::load(&self.dir)
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

pub fn http(
    origin: &str,
    path: &str,
    bearer: Option<&str>,
    body: Option<(&str, &str)>,
) -> Result<(u16, Value)> {
    http_method(
        origin, path, bearer, body,
        if body.is_some() { "POST" } else { "GET" },
    )
}

pub fn http_method(
    origin: &str,
    path: &str,
    bearer: Option<&str>,
    body: Option<(&str, &str)>,
    method: &str,
) -> Result<(u16, Value)> {
    http_method_idempotent(origin, path, bearer, body, method, None)
}

fn http_method_idempotent(
    origin: &str,
    path: &str,
    bearer: Option<&str>,
    body: Option<(&str, &str)>,
    method: &str,
    idempotency_key: Option<&str>,
) -> Result<(u16, Value)> {
    if !matches!(method, "GET" | "POST" | "DELETE") {
        return Err("unsupported Platform HTTP method".into());
    }
    let socket = std::env::var_os("SIFTTEXT_ACCOUNT_SOCKET");
    let mut config = format!(
        "url = {}\nrequest = {}\n",
        config_string(&format!("{}{path}", if socket.is_some() { "http://localhost" } else { origin }))?,
        config_string(method)?,
    );
    if let Some(socket) = &socket {
        let socket = PathBuf::from(socket);
        crate::require(socket.is_absolute(), "absolute managed account socket required")?;
        config.push_str(&format!("unix-socket = {}\nproxy = \"\"\n",
            config_string(socket.to_str().ok_or("invalid account socket path")?)?));
    }
    if let Some(token) = bearer.filter(|_| socket.is_none()) {
        config.push_str(&format!(
            "header = {}\n",
            config_string(&format!("Authorization: Bearer {token}"))?
        ));
    }
    if let Some(key) = idempotency_key {
        crate::require(!key.is_empty() && key.len() <= 128
            && key.bytes().all(|b| (b' '..=b'~').contains(&b)),
            "invalid checkout request key")?;
        config.push_str(&format!("header = {}\n",
            config_string(&format!("Idempotency-Key: {key}"))?));
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
            if path == "/api/checks" { "600" } else { "20" },
            "--max-filesize",
            if path.starts_with("/sync/v1/") { "46137344" } else { "1048576" },
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
        serde_json::from_str(body).map_err(|_| format!("Platform returned a non-JSON response (HTTP {status}); request did not yield a checker result"))?
    };
    Ok((status, value))
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Platform response missing {key}").into())
}

fn tokens(origin: &str, client_id: &str, fields: &[(&str, &str)]) -> Result<Credentials> {
    let (status, value) = http(
        origin,
        "/oauth/token",
        None,
        Some(("application/x-www-form-urlencoded", &form(fields))),
    )?;
    if status != 200 {
        return Err("Authorization expired or was revoked. Run sifttext login.".into());
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
        resource: fields.iter().find(|(key, _)| *key == "resource")
            .map(|(_, value)| (*value).to_owned()).unwrap_or_default(),
    })
}

fn refresh_for(store: &Store, credentials: &mut Credentials, resource: &str) -> Result<()> {
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

fn refresh(store: &Store, credentials: &mut Credentials) -> Result<()> {
    let resource = format!("{}/mcp", credentials.origin);
    refresh_for(store, credentials, &resource)
}

// Lock credential access/rotation, not the work using an access token. Atomic
// credential replacement permits lock-free reads between network requests.
pub struct SyncAccess {
    dir: PathBuf,
    credentials: Option<Credentials>,
}
impl SyncAccess {
    /// No directory creation or network access for an ordinary logged-out save.
    pub(crate) fn saved_login_exists() -> Result<bool> {
        if managed() { return Ok(true); }
        let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
        let path = PathBuf::from(home).join(".config/sifttext/account/credentials.json");
        match fs::symlink_metadata(path) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn checks() -> Result<Self> {
        Self::open(&format!("{}/mcp", origin()?))
    }

    /// Persist non-content recovery custody before any checker dispatch.
    pub(crate) fn prepare_check_request(&self, body: &Value) -> Result<(String, PathBuf)> {
        crate::require(serde_json::to_vec(body)?.len() <= CHECK_REQUEST_LIMIT,
            "Encoded check request exceeds 128 KiB. Select a smaller input to allow for JSON fields and escaping.")?;
        let id = format!("client_{}", random()?);
        let path = self.dir.join(format!("check-{id}.json"));
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600)
            .custom_flags(libc::O_NOFOLLOW).open(&path)?;
        let origin = self.credentials.as_ref().map(|credentials| credentials.origin.as_str())
            .unwrap_or("managed");
        serde_json::to_writer(&mut file, &json!({"request_id":id,
            "origin":origin,"checker":body["checker"],"created":now()?}))?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        File::open(&self.dir)?.sync_all()?;
        Ok((id, path))
    }

    pub(crate) fn request(
        &mut self, method: &str, path: &str, body: Option<&Value>,
    ) -> Result<(u16, Value)> {
        self.request_idempotent(method, path, body, None)
    }

    pub(crate) fn request_idempotent(
        &mut self, method: &str, path: &str, body: Option<&Value>, key: Option<&str>,
    ) -> Result<(u16, Value)> {
        let encoded = body.map(serde_json::to_string).transpose()?;
        if path == "/api/checks" || path.starts_with("/api/checks/") {
            crate::require(encoded.as_ref().is_none_or(|body| body.len() <= CHECK_REQUEST_LIMIT),
                "Encoded check request exceeds 128 KiB. Select a smaller input to allow for JSON fields and escaping.")?;
        }
        let body = encoded.as_deref().map(|s| ("application/json", s));
        let Some(credentials) = self.credentials.as_ref() else {
            return http_method_idempotent(&origin()?, path, None, body, method, key);
        };
        let origin = credentials.origin.clone();
        let resource = format!("{origin}/mcp");
        if credentials.resource != resource {
            self.renew(&resource)?;
        }
        let mut response = http_method_idempotent(&origin, path, Some(self.token()?), body, method, key)?;
        if response.0 == 401 {
            self.renew(&resource)?;
            response = http_method_idempotent(&origin, path, Some(self.token()?), body, method, key)?;
        }
        Ok(response)
    }

    pub fn open(resource: &str) -> Result<Self> {
        let store = Store::open()?;
        if managed() { return Ok(Self { dir: store.dir.clone(), credentials: None }); }
        let mut credentials = store.load()?;
        if credentials.origin != origin()? {
            return Err("saved login belongs to another Platform".into());
        }
        let saved_resource = if credentials.resource.is_empty() {
            format!("{}/mcp", credentials.origin)
        } else {
            credentials.resource.clone()
        };
        if saved_resource != resource || credentials.expires_at <= now()?.saturating_add(60) {
            refresh_for(&store, &mut credentials, resource)?;
        }
        Ok(Self { dir: store.dir.clone(), credentials: Some(credentials) })
    }
    pub fn token(&self) -> Result<&str> {
        // The socket supplies authority; this is never a bearer credential.
        let Some(credentials) = self.credentials.as_ref() else { return Ok(""); };
        let saved = Credentials::load(&self.dir)?;
        // Rotation may serve another resource without replacing the login.
        if saved.origin != credentials.origin || saved.client_id != credentials.client_id {
            return Err("saved login changed during sync; no further transfer attempted".into());
        }
        Ok(&credentials.access_token)
    }
    pub fn renew(&mut self, resource: &str) -> Result<()> {
        let Some(credentials) = self.credentials.as_mut() else { return Ok(()); };
        let store = Store::open()?;
        let saved = store.load()?;
        // Never adopt a new login or replay a concurrently rotated refresh token.
        if store.dir != self.dir || saved.origin != credentials.origin
            || saved.client_id != credentials.client_id
        {
            return Err("saved login changed during sync; no further transfer attempted".into());
        }
        let rotated = saved != *credentials;
        *credentials = saved;
        if rotated && credentials.resource == resource
            && credentials.expires_at > now()?.saturating_add(60)
        {
            return Ok(());
        }
        refresh_for(&store, credentials, resource)
    }
}

fn account(store: &Store, credentials: &mut Credentials) -> Result<Value> {
    if credentials.origin != origin()? {
        return Err("saved login belongs to a different Platform; run sifttext login".into());
    }
    if (!credentials.resource.is_empty()
        && credentials.resource != format!("{}/mcp", credentials.origin))
        || credentials.expires_at <= now()?.saturating_add(30) {
        refresh(store, credentials)?;
    }
    let mut response = http(
        &credentials.origin,
        "/oauth/account",
        Some(&credentials.access_token),
        None,
    )?;
    if response.0 == 401 {
        refresh(store, credentials)?;
        response = http(
            &credentials.origin,
            "/oauth/account",
            Some(&credentials.access_token),
            None,
        )?;
    }
    if response.0 != 200 {
        return Err(
            "could not verify Platform account; run sifttext login if authorization was revoked"
                .into(),
        );
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
    if fields.contains_key("error") {
        return Err("browser authorization was denied or cancelled".into());
    }
    fields
        .remove("code")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "missing callback code".into())
}

fn login(store: &Store) -> Result<()> {
    let origin = origin()?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let authority = listener.local_addr()?.to_string();
    let redirect = format!("http://{authority}/callback");
    let state = random()?;
    let verifier = random()?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let registration = json!({"client_name":"SiftText CLI", "redirect_uris":[redirect], "token_endpoint_auth_method":"none", "grant_types":["authorization_code","refresh_token"], "response_types":["code"]});
    let (status, registration) = http(
        &origin,
        "/oauth/register",
        None,
        Some(("application/json", &registration.to_string())),
    )?;
    if status != 200 && status != 201 {
        return Err("could not register SiftText CLI with Platform".into());
    }
    let client_id = text(&registration, "client_id")?;
    let resource = format!("{origin}/mcp");
    let url = format!(
        "{origin}/oauth/authorize?{}",
        form(&[
            ("response_type", "code"),
            ("client_id", client_id),
            ("redirect_uri", &redirect),
            ("state", &state),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("scope", "mcp"),
            ("resource", &resource),
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
                    deadline.saturating_duration_since(Instant::now()),
                );
                let (status, body) = if result.is_ok() {
                    (
                        "200 OK",
                        "Authorization received. Return to your terminal for account/payment verification.",
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
            ("resource", &resource),
        ],
    )?;
    store.save(&credentials)?;
    let account = account(store, &mut credentials)?;
    show_account(&account)?;
    if account["paid"] == true {
        writeln!(std::io::stdout(), "Authentication successful.")?;
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

pub fn guide_path() -> Result<PathBuf> {
    let path = crate::local::runtime()?.join("docs/account-sync.md");
    if !path.is_file() {
        return Err("installed account/sync guide missing; preserve local knowledge and repair the package".into());
    }
    Ok(path)
}

fn show_account(account: &Value) -> Result<()> {
    // Serialize only known account fields; never dump a token-bearing response.
    let output = json!({"id":account["id"], "email":account["email"],
        "subscription_status":account["subscription_status"], "paid":account["paid"],
        "subscription_plan":account["subscription_plan"],
        "guide":guide_path()?,
        "next":"Read the installed account/sync guide and inspect local consent before setup. Login and payment do not authorize upload."});
    writeln!(
        std::io::stdout(),
        "{}",
        crate::lean_render::render_receipt(&output)
            .map_err(std::io::Error::other)?
    )?;
    Ok(())
}

pub fn managed() -> bool {
    std::env::var_os("SIFTTEXT_ACCOUNT_SOCKET").is_some()
}

pub fn run(command: &str) -> Result<()> {
    if managed() {
        if matches!(command, "login" | "logout") {
            return Err("The server manages this hosted workspace's session. Use sifttext account to inspect it; sign in or disconnect through your MCP client.".into());
        }
        let (status, account) = http(&origin()?, "/oauth/account", None, None)?;
        crate::require(status == 200 && account["paid"].is_boolean(),
            &format!("Host account request failed (HTTP {status}, {}): {}",
                account["error"].as_str().unwrap_or("invalid_account_response"),
                account["detail"].as_str().unwrap_or("The host did not confirm this workspace's account.")))?;
        show_account(&account)?;
        return Ok(());
    }
    let store = Store::open()?;
    match command {
        "login" => login(&store),
        "account" => {
            let mut credentials = store.load()?;
            show_account(&account(&store, &mut credentials)?)
        }
        "logout" => {
            let credentials = store.load().ok();
            // Local removal comes first, even when offline or the saved file is malformed.
            store.clear()?;
            let revoked = credentials
                .filter(|c| origin().is_ok_and(|o| o == c.origin))
                .is_some_and(|c| {
                    http(
                        &c.origin,
                        "/oauth/revoke",
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
                "Logged out locally. {}",
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
