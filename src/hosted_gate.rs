//! Optional parent-owned acceptance channel. No credentials or remote policy live here.
//! The hosting parent, not a guest-writable receipt, owns the other socket endpoint.
use crate::{Result, git_snapshot::{GitReader, GitView}, require};
use serde_json::{Value, json};
use std::io::{Read, Write};

pub(crate) fn configured() -> bool {
    std::env::var_os("SIFTTEXT_HOSTED_GATE_FD").is_some()
}

#[cfg(unix)]
pub(crate) fn check(
    reader: &mut GitReader,
    before: Option<&GitView>,
    after: &GitView,
    commit: &str,
    recovery: bool,
) -> Result<Value> {
    use std::os::fd::FromRawFd;
    use std::os::unix::net::UnixStream;
    let fd: i32 = std::env::var("SIFTTEXT_HOSTED_GATE_FD")?.parse()?;
    require(fd >= 3, "hosting parent socket required")?;
    require(unsafe { libc::fcntl(fd, libc::F_GETFD) } >= 0, "hosting descriptor is not open")?;
    // Ownership is transferred exactly once by the executable's hosting parent.
    let mut socket = unsafe { UnixStream::from_raw_fd(fd) };
    require(unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } == 0,
        "could not isolate hosting socket from subprocesses")?;
    socket.set_read_timeout(Some(std::time::Duration::from_secs(1000)))?;
    socket.set_write_timeout(Some(std::time::Duration::from_secs(30)))?;
    let binding = json!({"protocol":1,"commit":commit,"tree":after.tree,
        "version":after.version,"baseline_tree":before.map(|v| &v.tree),"recovery":recovery});
    let mut nodes = Vec::new();
    for (id, node) in &after.nodes {
        if before.and_then(|v| v.nodes.get(id)).is_some_and(|old| old.raw_hash == node.raw_hash) {
            continue;
        }
        nodes.push(json!({"id":id,"text":String::from_utf8(reader.bytes(after, id)?)?}));
    }
    let bytes = serde_json::to_vec(&json!({"binding":binding,"nodes":nodes}))?;
    socket.write_all(&(bytes.len() as u64).to_be_bytes())?;
    socket.write_all(&bytes)?;
    let mut length = [0; 8];
    socket.read_exact(&mut length)?;
    let length = u64::from_be_bytes(length);
    require(length <= 8 * 1024 * 1024, "hosting response too large")?;
    let mut bytes = vec![0; length as usize];
    socket.read_exact(&mut bytes)?;
    let response: Value = serde_json::from_slice(&bytes)?;
    require(response["binding"] == binding, "hosting response candidate mismatch")?;
    let result = response["check"].clone();
    require(matches!(result["status"].as_str(), Some("passed" | "findings" | "unchecked" | "not_required")),
        "invalid hosting check outcome")?;
    Ok(result)
}

#[cfg(not(unix))]
pub(crate) fn check(_: &mut GitReader, _: Option<&GitView>, _: &GitView, _: &str, _: bool) -> Result<Value> {
    Err("hosting parent sockets unavailable on this platform".into())
}
