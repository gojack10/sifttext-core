//! Synthetic process-level OAuth exercise: no production network, account or browser.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::{MetadataExt, PermissionsExt},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

#[derive(Default)]
struct Observed {
    redirect: String,
    challenge: String,
    refreshes: usize,
    revoked: bool,
    account_calls: usize,
    reject_access_once: bool,
    reject_refresh: bool,
}

struct Server {
    origin: String,
    observed: Arc<Mutex<Observed>>,
    paid: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

fn decode(s: &str) -> String {
    let mut out = Vec::new();
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        out.push(if b == b'%' {
            let a = (bytes.next().unwrap() as char).to_digit(16).unwrap();
            let b = (bytes.next().unwrap() as char).to_digit(16).unwrap();
            (a * 16 + b) as u8
        } else if b == b'+' {
            b' '
        } else {
            b
        });
    }
    String::from_utf8(out).unwrap()
}

fn params(s: &str) -> BTreeMap<String, String> {
    s.split('&')
        .map(|s| {
            let (k, v) = s.split_once('=').unwrap();
            (decode(k), decode(v))
        })
        .collect()
}

impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let observed = Arc::new(Mutex::new(Observed::default()));
        let paid = Arc::new(AtomicBool::new(true));
        let stop = Arc::new(AtomicBool::new(false));
        let (o, p, s) = (observed.clone(), paid.clone(), stop.clone());
        let expected_resource = format!("{origin}/mcp");
        let thread = thread::spawn(move || {
            while !s.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let mut headers = BTreeMap::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    let (key, value) = line.split_once(':').unwrap();
                    headers.insert(key.to_lowercase(), value.trim().to_owned());
                }
                let length = headers
                    .get("content-length")
                    .map(|s| s.parse::<usize>().unwrap())
                    .unwrap_or(0);
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).unwrap();
                let body = String::from_utf8(bytes).unwrap();
                let route = first.split_whitespace().nth(1).unwrap();
                let mut o = o.lock().unwrap();
                let (status, response) = match route {
                    "/oauth/register" => {
                        let value: Value = serde_json::from_str(&body).unwrap();
                        assert_eq!(value["token_endpoint_auth_method"], "none");
                        o.redirect = value["redirect_uris"][0].as_str().unwrap().to_owned();
                        assert!(o.redirect.starts_with("http://127.0.0.1:"));
                        assert!(o.redirect.ends_with("/callback"));
                        (201, json!({"client_id":"synthetic-client"}))
                    }
                    "/oauth/token" => {
                        let fields = params(&body);
                        assert_eq!(fields["client_id"], "synthetic-client");
                        assert_eq!(fields["resource"], expected_resource);
                        if fields["grant_type"] == "authorization_code" {
                            assert_eq!(fields["code"], "synthetic-code");
                            assert_eq!(fields["redirect_uri"], o.redirect);
                            assert_eq!(
                                URL_SAFE_NO_PAD
                                    .encode(Sha256::digest(fields["code_verifier"].as_bytes())),
                                o.challenge
                            );
                        } else {
                            assert_eq!(fields["grant_type"], "refresh_token");
                            assert_eq!(fields["refresh_token"], format!("refresh-{}", o.refreshes));
                            o.refreshes += 1;
                        }
                        if o.reject_refresh {
                            (400, json!({"error":"invalid_grant"}))
                        } else {
                            (
                                200,
                                json!({"access_token":format!("access-{}", o.refreshes), "refresh_token":format!("refresh-{}", o.refreshes), "expires_in":3600, "token_type":"Bearer"}),
                            )
                        }
                    }
                    "/oauth/account" => {
                        o.account_calls += 1;
                        assert_eq!(
                            headers["authorization"],
                            format!("Bearer access-{}", o.refreshes)
                        );
                        if o.reject_access_once {
                            o.reject_access_once = false;
                            (401, json!({"error":"invalid_token"}))
                        } else {
                            let paid = p.load(Ordering::SeqCst);
                            (
                                200,
                                json!({"id":"synthetic-user", "username":"fixture", "email":"fixture@example.test", "display_name":"Fixture", "subscription_status":if paid {"active"} else {"pending"}, "paid":paid}),
                            )
                        }
                    }
                    "/oauth/revoke" => {
                        assert_eq!(params(&body)["token"], format!("refresh-{}", o.refreshes));
                        o.revoked = true;
                        (200, json!({}))
                    }
                    _ => panic!("unexpected request: {first}"),
                };
                let body = response.to_string();
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        Self {
            origin,
            observed,
            paid,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn cli(home: &std::path::Path, origin: &str, command: &str) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sifttext"));
    cmd.arg(command)
        .env("HOME", home)
        .env("SIFTTEXT_AUTH_ORIGIN", origin)
        .env("SIFTTEXT_NO_BROWSER", "1")
        .env("SIFTTEXT_AUTH_TIMEOUT_SECONDS", "3")
        .env("SIFTTEXT_RUNTIME_HOME", home.join("nonexistent-runtime"))
        .env("SIFTTEXT_INSTALL_ROOT", home.join("nonexistent-install"));
    cmd
}

fn credential_path(home: &std::path::Path) -> std::path::PathBuf {
    home.join(".config/sifttext/account/credentials.json")
}

fn start_login(
    home: &std::path::Path,
    server: &Server,
) -> (Child, BufReader<std::process::ChildStdout>, String) {
    let mut child = cli(home, &server.origin, "login")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    assert!(line.contains("Open this URL"));
    let mut url = String::new();
    stdout.read_line(&mut url).unwrap();
    let fields = params(url.trim().split_once('?').unwrap().1);
    assert_eq!(fields["scope"], "mcp");
    assert_eq!(fields["code_challenge_method"], "S256");
    assert_eq!(fields["resource"], format!("{}/mcp", server.origin));
    assert!(fields["state"].len() >= 43);
    server.observed.lock().unwrap().challenge = fields["code_challenge"].clone();
    (child, stdout, url.trim().to_owned())
}

fn callback(
    url: &str,
    state_override: Option<&str>,
    path: &str,
    host_override: Option<&str>,
) -> String {
    let fields = params(url.split_once('?').unwrap().1);
    let authority = fields["redirect_uri"]
        .strip_prefix("http://")
        .unwrap()
        .strip_suffix("/callback")
        .unwrap();
    let mut stream = TcpStream::connect(authority).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        stream,
        "GET {path}?code=synthetic-code&state={} HTTP/1.1\r\nHost: {}\r\n\r\n",
        state_override.unwrap_or(&fields["state"]),
        host_override.unwrap_or(authority)
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn finish(
    child: Child,
    mut stdout: BufReader<std::process::ChildStdout>,
) -> (bool, String, String) {
    let mut text = String::new();
    stdout.read_to_string(&mut text).unwrap();
    let output = child.wait_with_output().unwrap();
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(!text.contains("access-"));
    assert!(!text.contains("refresh-"));
    assert!(!error.contains("access-"));
    assert!(!error.contains("refresh-"));
    (output.status.success(), text, error)
}

#[test]
fn login_refresh_rotation_account_and_remote_logout() {
    let home = tempfile::tempdir().unwrap();
    let server = Server::new();
    let (child, stdout, url) = start_login(home.path(), &server);
    assert!(callback(&url, None, "/callback", None).contains("200 OK"));
    let (ok, text, error) = finish(child, stdout);
    assert!(ok, "{error}");
    assert!(text.contains("Authentication successful."));
    assert_eq!(server.observed.lock().unwrap().account_calls, 1);
    let path = credential_path(home.path());
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(path.parent().unwrap()).unwrap().mode() & 0o777,
        0o700
    );
    let mut credentials: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    credentials["expires_at"] = json!(0);
    fs::write(&path, credentials.to_string()).unwrap();
    let output = cli(home.path(), &server.origin, "account")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("\"paid\": true"));
    let credentials: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(credentials["refresh_token"], "refresh-1");
    server.observed.lock().unwrap().reject_access_once = true;
    assert!(
        cli(home.path(), &server.origin, "account")
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(server.observed.lock().unwrap().refreshes, 2);
    let output = cli(home.path(), &server.origin, "logout").output().unwrap();
    assert!(output.status.success());
    assert!(!path.exists());
    assert!(server.observed.lock().unwrap().revoked);
}

#[test]
fn wrong_state_path_host_and_timeout_never_activate_or_store() {
    let home = tempfile::tempdir().unwrap();
    let server = Server::new();
    for (state, path, host, message) in [
        (Some("wrong-state"), "/callback", None, "state mismatch"),
        (None, "/wrong", None, "path mismatch"),
        (None, "/callback", Some("localhost:9999"), "host mismatch"),
    ] {
        let (child, stdout, url) = start_login(home.path(), &server);
        assert!(callback(&url, state, path, host).contains("400 Bad Request"));
        let (ok, text, error) = finish(child, stdout);
        assert!(!ok);
        assert!(error.contains(message), "{error}");
        assert!(!text.contains("Authentication successful."));
        assert!(!credential_path(home.path()).exists());
    }
    let output = cli(home.path(), &server.origin, "login")
        .env("SIFTTEXT_AUTH_TIMEOUT_SECONDS", "1")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("timed out"));
    assert_eq!(server.observed.lock().unwrap().account_calls, 0);
}

#[test]
fn unpaid_permissions_revocation_and_offline_logout_preserve_knowledge() {
    let home = tempfile::tempdir().unwrap();
    let server = Server::new();
    let knowledge = home.path().join("sifttext/nodes/fixture/node.md");
    fs::create_dir_all(knowledge.parent().unwrap()).unwrap();
    fs::write(&knowledge, "offline knowledge").unwrap();
    server.paid.store(false, Ordering::SeqCst);
    let (child, stdout, url) = start_login(home.path(), &server);
    callback(&url, None, "/callback", None);
    let (ok, text, _) = finish(child, stdout);
    assert!(!ok);
    assert!(!text.contains("Authentication successful."));
    assert!(text.contains("Payment pending"));
    assert!(text.contains(&server.origin));
    let path = credential_path(home.path());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let output = cli(home.path(), &server.origin, "account")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("0600"));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let mut credentials: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    credentials["expires_at"] = json!(0);
    fs::write(&path, credentials.to_string()).unwrap();
    server.observed.lock().unwrap().reject_refresh = true;
    let output = cli(home.path(), &server.origin, "account")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("revoked"));
    let origin = server.origin.clone();
    drop(server);
    let output = cli(home.path(), &origin, "logout").output().unwrap();
    assert!(output.status.success());
    assert!(!path.exists());
    assert!(String::from_utf8_lossy(&output.stdout).contains("could not be confirmed"));
    assert_eq!(fs::read_to_string(&knowledge).unwrap(), "offline knowledge");
}

#[test]
fn browser_open_failure_still_prints_usable_link_and_git_store_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let server = Server::new();
    let bin = home.path().join("bin");
    fs::create_dir(&bin).unwrap();
    for opener in ["open", "xdg-open"] {
        let path = bin.join(opener);
        fs::write(
            &path,
            "#!/bin/sh\nprintf '%s\\n' \"$1\" > \"$HOME/browser-url\"\nexit 1\n",
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut cmd = cli(home.path(), &server.origin, "login");
    cmd.env_remove("SIFTTEXT_NO_BROWSER").env(
        "PATH",
        format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
    );
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let mut url = String::new();
    stdout.read_line(&mut url).unwrap();
    server.observed.lock().unwrap().challenge =
        params(url.trim().split_once('?').unwrap().1)["code_challenge"].clone();
    callback(url.trim(), None, "/callback", None);
    assert!(finish(child, stdout).0);
    assert_eq!(
        fs::read_to_string(home.path().join("browser-url"))
            .unwrap()
            .trim(),
        url.trim()
    );
    fs::create_dir(home.path().join(".git")).unwrap();
    let output = cli(home.path(), &server.origin, "login").output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Git repository"));
}
