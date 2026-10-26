//! Publication-only Unix helper custody. No callbacks may reenter mutation of
//! an already leased path. Stable path gates are never unlinked.
use crate::{Result, require};
use std::{
    cell::RefCell,
    fs,
    io::Read,
    os::{
        fd::{AsRawFd, RawFd},
        unix::{fs::OpenOptionsExt, process::CommandExt},
    },
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

thread_local! { static CUSTODY: RefCell<Vec<RawFd>> = const { RefCell::new(Vec::new()) }; }
#[cfg(test)]
thread_local! { static PIPE_ERROR: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
// Keep a duplicate alive; clear CLOEXEC only in the forked child, never in the
// multithreaded parent. Rc marker makes this thread-local scope non-Send.
pub(crate) struct Custody {
    files: Vec<fs::File>,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl Custody {
    pub(crate) fn new(files: &[&fs::File]) -> Result<Self> {
        let files = files
            .iter()
            .map(|f| f.try_clone())
            .collect::<std::io::Result<Vec<_>>>()?;
        CUSTODY.with_borrow_mut(|fds| fds.extend(files.iter().map(AsRawFd::as_raw_fd)));
        Ok(Self {
            files,
            _thread: std::marker::PhantomData,
        })
    }
}
impl Drop for Custody {
    fn drop(&mut self) {
        CUSTODY.with_borrow_mut(|fds| {
            fds.retain(|fd| !self.files.iter().any(|f| f.as_raw_fd() == *fd))
        });
    }
}

pub(super) fn run(cmd: Command) -> Result<Output> {
    run_bounded(cmd, Duration::from_secs(30))
}

pub(super) fn run_stdout_file(cmd: Command, file: &fs::File) -> Result<Output> {
    run_bounded_stdout(cmd, Duration::from_secs(30), Some(file))
}

fn run_bounded(cmd: Command, budget: Duration) -> Result<Output> {
    run_bounded_stdout(cmd, budget, None)
}

fn run_bounded_stdout(mut cmd: Command, budget: Duration, file: Option<&fs::File>) -> Result<Output> {
    const CAP: usize = 16 * 1024 * 1024;
    // A separate open description lets us prove inherited handles have closed,
    // including descendants that close their output pipes before exiting.
    let lease = tempfile::NamedTempFile::new()?;
    lease.as_file().lock()?;
    let witness = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(lease.path())?;
    let mut fds = CUSTODY.with_borrow(Clone::clone);
    fds.push(lease.as_file().as_raw_fd());
    cmd.process_group(0);
    unsafe {
        cmd.pre_exec(move || {
            for fd in &fds {
                let flags = libc::fcntl(*fd, libc::F_GETFD);
                if flags < 0 || libc::fcntl(*fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(match file { Some(file) => Stdio::from(file.try_clone()?), None => Stdio::piped() })
        .stderr(Stdio::piped())
        .spawn()?;
    let group = child.id() as libc::pid_t;
    // Close the parent's description after spawn; inherited copies alone keep
    // the witness busy. Unlinking this private witness is not a path gate.
    drop(lease);
    let pipes: [Option<Box<dyn Read + Send>>; 2] = [
        child.stdout.take().map(|pipe| Box::new(pipe) as _),
        Some(Box::new(child.stderr.take().ok_or("Git pipe unavailable")?)),
    ];
    let expected = if file.is_some() { 1 } else { 2 };
    let read_bytes = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let (tx, rx) = std::sync::mpsc::channel();
    #[cfg(test)]
    let pipe_error = PIPE_ERROR.with(|v| v.replace(false));
    #[cfg(not(test))]
    let pipe_error = false;
    let readers: Vec<_> = pipes
        .into_iter()
        .enumerate()
        .filter_map(|(tag, pipe)| pipe.map(|pipe| (tag, pipe)))
        .map(|(tag, mut pipe)| {
            let tx = tx.clone();
            let read_bytes = read_bytes.clone();
            thread::spawn(move || {
                let mut bytes = Vec::new();
                let result = if pipe_error && tag == 0 {
                    Err(std::io::Error::other("injected pipe failure"))
                } else {
                    (|| {
                        let mut chunk = [0; 8192];
                        while bytes.len() <= CAP {
                            let n = pipe.read(&mut chunk)?;
                            if n == 0 { break; }
                            read_bytes.fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
                            bytes.extend_from_slice(&chunk[..n]);
                        }
                        Ok(bytes)
                    })()
                };
                let _ = tx.send((tag, result));
            })
        })
        .collect();
    drop(tx);
    let start = Instant::now();
    let mut bytes = [Vec::new(), Vec::new()];
    let mut received = 0;
    let result = (|| -> Result<Output> {
        loop {
            while let Ok((tag, result)) = rx.try_recv() {
                received += 1;
                bytes[tag] = result.map_err(|_| "Git pipe read failed")?;
                require(
                    bytes.iter().map(Vec::len).sum::<usize>() <= CAP,
                    "Git output resource limit",
                )?;
            }
            require(
                file.map_or(Ok(0), |file| file.metadata().map(|m| m.len()))?
                    .saturating_add(read_bytes.load(std::sync::atomic::Ordering::Relaxed)) <= CAP as u64,
                "Git output resource limit",
            )?;
            // Do not reap the leader while descendants may retain custody:
            // its live/zombie PID pins the process-group identity until any
            // required group kill, avoiding a kill against a recycled PID.
            if received == expected && witness.try_lock().is_ok() {
                require(file.map_or(Ok(0), |file| file.metadata().map(|m| m.len()))?
                    .saturating_add(read_bytes.load(std::sync::atomic::Ordering::Relaxed)) <= CAP as u64,
                    "Git output resource limit")?;
                if let Some(status) = child.try_wait().map_err(|_| "Git wait failed")? {
                    let [stdout, stderr] = std::mem::take(&mut bytes);
                    return Ok(Output {
                        status,
                        stdout,
                        stderr,
                    });
                }
            }
            require(start.elapsed() < budget, "Git helper timeout")?;
            thread::sleep(Duration::from_millis(2));
        }
    })();
    if result.is_err() {
        // Kill the entire owned group, not just Git's leader. SIGKILL has no
        // unbounded grace period. Reap the leader and drain ordinary descendants.
        unsafe {
            libc::kill(-group, libc::SIGKILL);
        }
        let _ = child.wait();
    }
    let drain = Instant::now();
    while received < expected && drain.elapsed() < Duration::from_secs(1) {
        if rx.recv_timeout(Duration::from_millis(10)).is_ok() {
            received += 1;
        }
    }
    let stopped = loop {
        if witness.try_lock().is_ok() {
            break true;
        }
        if drain.elapsed() >= Duration::from_secs(1) {
            break false;
        }
        thread::sleep(Duration::from_millis(2));
    };
    // Ordinary supported descendants are drained; hostile session/FD escape is
    // not a sandbox guarantee. Do not block indefinitely on such a pipe reader.
    if received == expected {
        for reader in readers {
            let _ = reader.join();
        }
    }
    if !stopped || received != expected {
        return Err(format!(
            "{}; Git termination unconfirmed; preserve custody",
            result
                .err()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Git helper completion unavailable".into())
        )
        .into());
    }
    result
}

#[cfg(test)]
#[path = "../../tests/successor/publication_lifetime.rs"]
mod tests;
