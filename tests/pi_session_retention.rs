//! Opt-in lifecycle qualification against the installed Pi CLI and a local mock provider.
//! Run with `cargo test --test pi_session_retention -- --ignored`.
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Output},
    thread,
};

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn request(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut data = Vec::new();
    let mut buf = [0; 4096];
    let end = loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "headers",
            ));
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&data[..end]).into_owned();
    let length = head
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .and_then(|n| n.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    while data.len() < end + length {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
    }
    Ok(head)
}

fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(body).unwrap();
}

fn serve(listener: TcpListener) -> Result<(), String> {
    for incoming in listener.incoming().take(4) {
        let mut stream = incoming.map_err(|e| e.to_string())?;
        let head = request(&mut stream).map_err(|e| e.to_string())?;
        let line = head.lines().next().unwrap_or("");
        if line.contains("/chat/completions") {
            let body = concat!(
                "data: {\"id\":\"local\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"CAPTURE-ME\"},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"local\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                "data: [DONE]\n\n"
            );
            respond(&mut stream, "200 OK", "text/event-stream", body.as_bytes());
            return Ok(());
        }
        if line.contains("/models") {
            respond(
                &mut stream,
                "200 OK",
                "application/json",
                br#"{"data":[{"id":"test","object":"model"}]}"#,
            );
        } else {
            respond(&mut stream, "404 Not Found", "application/json", b"{}");
        }
    }
    Err("Pi did not request a chat completion".into())
}

fn run_pi(pi: &str, extension: &Path, sessions: &Path, agent: &Path, home: &Path) -> Output {
    Command::new(pi)
        .args(["--print", "--model", "mock/test", "--extension"])
        .arg(extension)
        .arg("--session-dir")
        .arg(sessions)
        .args([
            "--session-id",
            "rust-lifecycle-test",
            "--no-tools",
            "Reply with exactly: CAPTURE-ME",
        ])
        .env("PI_CODING_AGENT_DIR", agent)
        .env("PI_OFFLINE", "1")
        .env("SIFTTEXT_HOME", home)
        .env("SIFTTEXT_BIN", env!("CARGO_BIN_EXE_sifttext"))
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .output()
        .unwrap()
}

fn run_lifecycle(retention_enabled: bool) {
    let temp = tempfile::tempdir().unwrap();
    let agent = temp.path().join("pi-agent");
    let sessions = temp.path().join("pi-sessions");
    let home = temp.path().join("home");
    let archive = temp.path().join("archive");
    for dir in [&agent, &sessions, &home] {
        fs::create_dir_all(dir).unwrap();
    }
    if retention_enabled {
        fs::create_dir_all(archive.join("sessions")).unwrap();
        fs::create_dir_all(home.join(".sifttext")).unwrap();
        fs::write(
            home.join(".sifttext/config.json"),
            serde_json::to_vec(&serde_json::json!({
                "session_retention": {"enabled": true, "archive_root": archive}
            }))
            .unwrap(),
        )
        .unwrap();
        git(&archive, &["init", "-q"]);
        git(&archive, &["config", "user.name", "Test"]);
        git(&archive, &["config", "user.email", "test@example.invalid"]);
    }

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || serve(listener));
    fs::write(
        agent.join("models.json"),
        format!(
            r#"{{"providers":{{"mock":{{"baseUrl":"http://{address}/v1","api":"openai-completions","apiKey":"local-only","models":[{{"id":"test","name":"Local test","contextWindow":4096,"maxTokens":128}}]}}}}}}"#
        ),
    )
    .unwrap();

    let extension =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("integrations/pi/session-retention.ts");
    let output = run_pi(
        &std::env::var("PI_BIN").unwrap_or_else(|_| "pi".into()),
        &extension,
        &sessions,
        &agent,
        &home,
    );
    server.join().unwrap().unwrap();
    assert!(
        output.status.success(),
        "Pi failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("CAPTURE-ME"));

    let retained = archive.join("sessions/rust-lifecycle-test.jsonl");
    if retention_enabled {
        let bytes = fs::read(&retained).unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("CAPTURE-ME"));
        let commit = Command::new("git")
            .arg("-C")
            .arg(&archive)
            .args(["rev-parse", "--verify", "HEAD"])
            .output()
            .unwrap();
        assert!(commit.status.success(), "session was not Git-checkpointed");
    } else {
        assert!(!retained.exists(), "disabled retention wrote an archive");
    }
}

#[test]
#[ignore = "requires installed Pi CLI; run explicitly with --ignored"]
fn agent_end_retains_final_response_using_local_mock_provider() {
    run_lifecycle(true);
}

#[test]
#[ignore = "requires installed Pi CLI; run explicitly with --ignored"]
fn agent_end_does_not_retain_when_config_is_absent() {
    run_lifecycle(false);
}
