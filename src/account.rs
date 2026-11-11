//! Browser OAuth is independent of knowledge, installed assets and update locks.
//! curl is an existing installation prerequisite; credentials travel only over stdin.
mod provider;
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
pub const WORKSPACE_ORIGIN: &str = "https://workspace.sifttext.com";
const LEGACY_GRANT: &str = "credentials.json";
const ACTIVE_PRODUCTION_ISSUER: &str = "keycloak-active";
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
    #[serde(default)]
    issuer: String,
    #[serde(default)]
    login_id: String,
    #[serde(default)]
    account_id: String,
}

impl Credentials {
    fn same_login(&self, other: &Self) -> bool {
        self.origin == other.origin
            && self.issuer == other.issuer
            && self.client_id == other.client_id
            && self.login_id == other.login_id
    }

    fn load(path: &Path) -> Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
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

// Arbitrary account origins must not receive saved credentials.
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
    origin: String,
    issuer: String,
    _lock: File,
}

fn account_resource() -> Result<String> {
    Ok(format!("{}/mcp", origin()?))
}

// Leave legacy credentials readable by the previous CLI. OIDC grants are bound
// to one resource and are never written to the legacy credential file.
fn grant_filename(origin: &str, issuer: &str, resource: &str) -> Result<String> {
    if issuer.is_empty() {
        return Ok(LEGACY_GRANT.to_owned());
    }
    let key = serde_json::to_vec(&(origin, issuer, resource))?;
    Ok(format!("grant-{:x}.json", Sha256::digest(key)))
}

fn saved_issuer(dir: &Path, origin: &str) -> Result<String> {
    if let Some(issuer) = provider::configured_issuer(origin)? {
        return Ok(issuer);
    }
    if origin == PLATFORM && dir.join(ACTIVE_PRODUCTION_ISSUER).try_exists()? {
        return Ok(provider::PRODUCTION_ISSUER.to_owned());
    }
    Ok(String::new())
}

impl Store {
    fn open(resource: &str, login_issuer: Option<&str>) -> Result<Self> {
        let origin = origin()?;
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
        let issuer = match login_issuer {
            Some(issuer) => issuer.to_owned(),
            None if managed() => String::new(),
            None => saved_issuer(&dir, &origin)?,
        };
        if issuer == provider::PRODUCTION_ISSUER {
            let audience = resource.strip_suffix("/mcp");
            if !matches!(audience, Some(PLATFORM | WORKSPACE_ORIGIN)) {
                return Err("production grants require the official account or Workspace resource".into());
            }
        }
        let filename = grant_filename(&origin, &issuer, resource)?;
        Ok(Self { dir, filename, origin, issuer, _lock: lock })
    }

    fn path(&self) -> PathBuf {
        self.dir.join(&self.filename)
    }

    fn load(&self) -> Result<Credentials> {
        let credentials = Credentials::load(&self.path())?;
        if self.issuer == provider::PRODUCTION_ISSUER && credentials.account_id.is_empty() {
            return Err("No verified production login. Run sifttext login.".into());
        }
        Ok(credentials)
    }

    fn check_provider(&self, credentials: &Credentials) -> Result<()> {
        if credentials.origin != self.origin || credentials.issuer != self.issuer {
            return Err("saved login belongs to another account service or issuer; sign in again".into());
        }
        Ok(())
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

    // Normal requests use the last successful production sign-in without
    // contacting discovery. Re-signing in after rollback selects legacy again.
    fn activate(&self) -> Result<()> {
        if self.origin != PLATFORM {
            return Ok(());
        }
        let active = self.dir.join(ACTIVE_PRODUCTION_ISSUER);
        if self.issuer == provider::PRODUCTION_ISSUER {
            let file = tempfile::NamedTempFile::new_in(&self.dir)?;
            file.as_file().sync_all()?;
            file.persist(active)?;
        } else {
            Self::remove(&active)?;
        }
        File::open(&self.dir)?.sync_all()?;
        Ok(())
    }

    fn remove(path: &Path) -> Result<()> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn clear(&self, resource: &str) -> Result<()> {
        Self::remove(&self.path())?;
        if self.origin == PLATFORM && resource == account_resource()? {
            // A default-account logout also drops any saved login for the
            // inactive production authority. Workspace grants are separate.
            let inactive = if self.issuer.is_empty() { provider::PRODUCTION_ISSUER } else { "" };
            Self::remove(&self.dir.join(grant_filename(&self.origin, inactive, resource)?))?;
        }
        File::open(&self.dir)?.sync_all()?;
        Ok(())
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
    let (status, body) = http_response(origin, path, bearer, body, method, idempotency_key)?;
    Ok((status, json_response(status, &body)?))
}

fn http_response(
    origin: &str,
    path: &str,
    bearer: Option<&str>,
    body: Option<(&str, &str)>,
    method: &str,
    idempotency_key: Option<&str>,
) -> Result<(u16, Vec<u8>)> {
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
    let mut request_body = None;
    if let Some((content_type, data)) = body {
        config.push_str(&format!("header = {}\n",
            config_string(&format!("Content-Type: {content_type}"))?));
        if data.len() > CHECK_REQUEST_LIMIT {
            // curl limits a physical config line to 10 MiB. Selected sync
            // bodies can be larger; keep them in an owner-only temporary file
            // for this request, while authorization stays off argv and disk.
            let mut file = tempfile::Builder::new().prefix("sifttext-http-").tempfile()?;
            file.write_all(data.as_bytes())?;
            config.push_str(&format!("data-binary = {}\n",
                config_string(&format!("@{}", file.path().to_str().ok_or("invalid HTTP body path")?))?));
            request_body = Some(file);
        } else {
            config.push_str(&format!("data = {}\n", config_string(data)?));
        }
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
    drop(request_body);
    sent?;
    if !output.status.success() {
        return Err("Platform request failed; check your connection and try again".into());
    }
    let mut response = output.stdout;
    let body_end = response
        .iter()
        .rposition(|&byte| byte == b'\n')
        .ok_or("missing HTTP status")?;
    let status = std::str::from_utf8(&response[body_end + 1..])?.parse()?;
    response.truncate(body_end);
    Ok((status, response))
}

fn json_response(status: u16, body: &[u8]) -> Result<Value> {
    if body.is_empty() {
        Ok(Value::Null)
    } else {
        serde_json::from_slice(body).map_err(|_| format!("Platform returned a non-JSON response (HTTP {status}); request did not yield a checker result").into())
    }
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

fn tokens(
    provider: &provider::Provider,
    origin: &str,
    client_id: &str,
    login_id: &str,
    fields: &[(&str, &str)],
) -> Result<Credentials> {
    let (status, body) = http_response(
        &provider.token,
        "",
        None,
        Some(("application/x-www-form-urlencoded", &form(fields))),
        "POST",
        None,
    )?;
    if status != 200 && provider.issuer.is_empty() {
        return Err("Platform authorization is no longer available. Run sifttext login to reconnect.".into());
    }
    let value = json_response(status, &body)?;
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
        issuer: provider.issuer.clone(),
        resource: fields.iter().find(|(key, _)| *key == "resource")
            .map(|(_, value)| (*value).to_owned()).unwrap_or_default(),
        login_id: login_id.to_owned(),
        account_id: String::new(),
    })
}

fn refresh_for(store: &Store, credentials: &mut Credentials, resource: &str) -> Result<()> {
    store.check_provider(credentials)?;
    if !credentials.issuer.is_empty() && credentials.resource != resource {
        return Err("resource-bound grant cannot change audience; login separately for this resource".into());
    }
    let provider = provider::for_issuer(&store.origin, &store.issuer)?;
    let mut refreshed = tokens(
        &provider,
        &credentials.origin,
        &credentials.client_id,
        &credentials.login_id,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", &credentials.client_id),
            ("refresh_token", &credentials.refresh_token),
            ("resource", resource),
        ],
    )?;
    refreshed.account_id.clone_from(&credentials.account_id);
    *credentials = refreshed;
    // The first production login must establish its account before replacing a
    // verified grant, even if account verification requires a token refresh.
    if store.issuer == provider::PRODUCTION_ISSUER && credentials.account_id.is_empty() {
        return Ok(());
    }
    // Persist rotation before any later account/network failure loses the new refresh token.
    store.save(credentials)
}

// Lock credential access/rotation, not the work using an access token. Atomic
// credential replacement permits lock-free reads between network requests.
pub struct SyncAccess {
    dir: PathBuf,
    file: PathBuf,
    credentials: Option<Credentials>,
}
impl SyncAccess {
    /// No directory creation or network access for an ordinary logged-out save.
    pub(crate) fn saved_login_exists() -> Result<bool> {
        if managed() { return Ok(true); }
        let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
        let dir = PathBuf::from(home).join(".config/sifttext/account");
        let origin = origin()?;
        let issuer = saved_issuer(&dir, &origin)?;
        let path = dir.join(grant_filename(&origin, &issuer, &account_resource()?)?);
        match fs::symlink_metadata(path) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn checks() -> Result<Self> {
        Self::open(&account_resource()?)
    }

    pub(crate) fn checks_for_sync(sync: &Self) -> Result<Self> {
        let account = Self::checks()?;
        if let Some(workspace) = sync.credentials.as_ref()
            && workspace.issuer == provider::PRODUCTION_ISSUER
            && !account.credentials.as_ref().is_some_and(|grant| {
                grant.issuer == workspace.issuer && grant.account_id == workspace.account_id
            })
        {
            return Err("Sign in to the same SiftText account for Workspace and checker review.".into());
        }
        Ok(account)
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
        let store = Store::open(resource, None)?;
        if managed() { return Ok(Self { dir: store.dir.clone(), file: store.path(), credentials: None }); }
        let mut credentials = if store.issuer.is_empty() || resource == account_resource()? {
            store.load()?
        } else {
            store.load().map_err(|_| {
                let command = if store.issuer == provider::PRODUCTION_ISSUER {
                    "sifttext login".to_owned()
                } else {
                    format!("sifttext login --resource {resource}")
                };
                format!("No saved grant for {resource}. Run {command}.")
            })?
        };
        store.check_provider(&credentials)?;
        let saved_resource = if credentials.resource.is_empty() {
            format!("{}/mcp", credentials.origin)
        } else {
            credentials.resource.clone()
        };
        if saved_resource != resource || credentials.expires_at <= now()?.saturating_add(60) {
            refresh_for(&store, &mut credentials, resource)?;
        }
        Ok(Self { dir: store.dir.clone(), file: store.path(), credentials: Some(credentials) })
    }
    pub fn token(&self) -> Result<&str> {
        // The socket supplies authority; this is never a bearer credential.
        let Some(credentials) = self.credentials.as_ref() else { return Ok(""); };
        let saved = Credentials::load(&self.file)?;
        // Rotation may serve another resource without replacing the login.
        if !saved.same_login(credentials)
            || saved_issuer(&self.dir, &credentials.origin)? != credentials.issuer
        {
            return Err("saved login changed during sync; no further transfer attempted".into());
        }
        Ok(&credentials.access_token)
    }
    pub fn renew(&mut self, resource: &str) -> Result<()> {
        let Some(credentials) = self.credentials.as_mut() else { return Ok(()); };
        let store = Store::open(resource, None)?;
        if store.path() != self.file {
            return Err("saved grant changed during sync; retry using the matching sign-in".into());
        }
        let saved = store.load()?;
        // Never adopt a new login or replay a concurrently rotated refresh token.
        if store.dir != self.dir || !saved.same_login(credentials) {
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

    pub fn restore_legacy_resource(&mut self, resource: &str) -> Result<()> {
        if self.credentials.as_ref().is_some_and(|grant| grant.issuer.is_empty()) {
            self.renew(resource)?;
        }
        Ok(())
    }
}

fn account(store: &Store, credentials: &mut Credentials, resource: &str) -> Result<Value> {
    store.check_provider(credentials)?;
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
        if credentials.issuer.is_empty() {
            return Err(
                "could not verify Platform account; run sifttext login if authorization was revoked"
                    .into(),
            );
        }
        return Err(format!("Account verification at {endpoint}{path} returned HTTP {}. A 403 can mean this signed-in identity has no enabled SiftText account mapping; a 401 means the resource rejected its token.", response.0).into());
    }
    if !response.1["paid"].is_boolean() {
        return Err("Platform did not confirm subscription status".into());
    }
    if !credentials.account_id.is_empty() && text(&response.1, "id")? != credentials.account_id {
        return Err("The account service returned a different SiftText account. Sign in again.".into());
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
    if let Some(issuer) = expected_issuer
        && fields.get("iss").map(String::as_str) != Some(issuer)
    {
        return Err("authorization callback issuer mismatch".into());
    }
    if fields.contains_key("error") {
        return Err(format!("Browser authorization failed: {}", oauth_error_name(fields.get("error").map(String::as_str))).into());
    }
    fields
        .remove("code")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "missing callback code".into())
}

fn authorize(
    store: &Store,
    provider: &provider::Provider,
    resource: &str,
    combined: bool,
    browser_callback: &mut Option<TcpStream>,
) -> Result<(Credentials, Value)> {
    let origin = &store.origin;
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
    let scope = if combined { "openid account:read mcp" }
        else if provider.issuer.is_empty() { "mcp" }
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
    if let Some(mut stream) = browser_callback.take() {
        // Continue from the verified callback in the tab that started login.
        write!(
            stream,
            "HTTP/1.1 303 See Other\r\nLocation: {url}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
        )?;
    } else {
        writeln!(
            std::io::stdout(),
            "Open this URL to sign in (Ctrl-C cancels):\n{url}"
        )?;
        std::io::stdout().flush()?;
        open_browser(&url);
    }
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
                // Reply after account verification, or redirect to the next resource.
                *browser_callback = Some(stream);
                break result?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20))
            }
            Err(e) => return Err(e.into()),
        }
    };
    drop(listener);
    let login_id = random()?;
    let mut credentials = tokens(
        provider,
        origin,
        client_id,
        &login_id,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("code", &code),
            ("redirect_uri", &redirect),
            ("code_verifier", &verifier),
            ("resource", resource),
        ],
    )?;
    let production = provider.issuer == provider::PRODUCTION_ISSUER;
    if !production {
        store.save(&credentials)?;
    }
    let account = account(store, &mut credentials, resource)?;
    if production {
        credentials.account_id = text(&account, "id")?.to_owned();
    }
    Ok((credentials, account))
}

fn login(store: &Store, provider: &provider::Provider, resource: &str, combined: bool) -> Result<()> {
    let mut browser_callback = None;
    let result: Result<()> = (|| {
        let (credentials, account) =
            authorize(store, provider, resource, combined, &mut browser_callback)?;
        if combined {
            let workspace_resource = format!("{WORKSPACE_ORIGIN}/mcp");
            let workspace_store = Store {
                dir: store.dir.clone(),
                filename: grant_filename(&store.origin, &store.issuer, &workspace_resource)?,
                origin: store.origin.clone(),
                issuer: store.issuer.clone(),
                _lock: store._lock.try_clone()?,
            };
            writeln!(std::io::stdout(), "Connecting Workspace in the same browser tab.")?;
            let (workspace_credentials, workspace_account) =
                authorize(&workspace_store, provider, &workspace_resource, true, &mut browser_callback)?;
            if text(&workspace_account, "id")? != text(&account, "id")? {
                return Err("Account and Workspace sign-ins selected different SiftText accounts. Run sifttext login and use the same account throughout.".into());
            }
            workspace_store.save(&workspace_credentials)?;
        }
        if provider.issuer == provider::PRODUCTION_ISSUER {
            store.save(&credentials)?;
        }
        store.activate()?;
        show_account(&account)?;
        if combined {
            writeln!(std::io::stdout(), "Authentication successful. Account and Workspace connected.")?;
            Ok(())
        } else if !provider.issuer.is_empty() {
            writeln!(std::io::stdout(), "Authentication successful. Service entitlements are enforced separately.")?;
            Ok(())
        } else if account["paid"] == true {
            writeln!(std::io::stdout(), "Authentication successful.")?;
            Ok(())
        } else {
            writeln!(
                std::io::stdout(),
                "Payment pending or subscription inactive. Complete payment on Platform, then run sifttext login again.\n{}/",
                store.origin,
            )?;
            open_browser(&format!("{}/", store.origin));
            Err("paid activation has not been confirmed; offline knowledge remains available".into())
        }
    })();
    if let Some(mut stream) = browser_callback {
        let (status, body) = if result.is_ok() {
            ("200 OK", "Sign-in complete. You can close this tab and return to your terminal.")
        } else {
            ("400 Bad Request", "Sign-in could not be completed. Return to your terminal for the error and retry.")
        };
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
    }
    result
}

pub fn guide_path() -> Result<PathBuf> {
    let path = crate::local::runtime()?.join("nodes/12e1e893-b0ab-4f82-b0db-37de70b95034/node.md");
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

pub fn run(command: &str, selected_resource: Option<&str>) -> Result<()> {
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
    let default_resource = account_resource()?;
    let resource = selected_resource.unwrap_or(&default_resource);
    if command == "login" {
        let provider = provider::for_login(&origin()?)?;
        if selected_resource.is_some() && provider.issuer.is_empty() {
            return Err("The current sign-in service uses a single login. Run sifttext login.".into());
        }
        let store = Store::open(resource, Some(&provider.issuer))?;
        let combined = selected_resource.is_none() && provider.issuer == provider::PRODUCTION_ISSUER;
        return login(&store, &provider, resource, combined);
    }
    let store = Store::open(resource, None)?;
    if selected_resource.is_some() && store.issuer.is_empty() {
        return Err("The current sign-in service uses a single login. Omit --resource.".into());
    }
    match command {
        "account" => {
            let mut credentials = store.load()?;
            show_account(&account(&store, &mut credentials, resource)?)
        }
        "logout" => {
            let credentials = store.load().ok();
            // Local removal comes first, even when offline or the saved file is malformed.
            store.clear(resource)?;
            let revoked = credentials
                .filter(|c| store.check_provider(c).is_ok())
                .is_some_and(|c| {
                    let Ok(provider) = provider::for_issuer(&store.origin, &store.issuer) else { return false; };
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
