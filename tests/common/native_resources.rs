//! Observational process-tree resources, not continuous peak or capacity guarantees.
use serde_json::{Value, json};
#[cfg(target_os = "linux")]
use std::fs;
use std::os::unix::process::CommandExt;
use std::{
    collections::BTreeSet,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

// Sampling must never park the capacity watchdog indefinitely.
fn bounded_output(command: &mut Command, bound: Duration, stopped: fn() -> bool) -> Output {
    bounded_output_with(command, bound, stopped, |pid| {
        if unsafe { libc::kill(-(pid as i32), libc::SIGKILL) } == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    })
}
fn bounded_output_with(
    command: &mut Command,
    bound: Duration,
    stopped: fn() -> bool,
    mut kill: impl FnMut(u32) -> std::io::Result<()>,
) -> Output {
    let stdout = tempfile::tempfile().unwrap();
    let stderr = tempfile::tempfile().unwrap();
    let mut child = command
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(stdout.try_clone().unwrap())
        .stderr(stderr.try_clone().unwrap())
        .spawn()
        .unwrap();
    if let Some(root) = std::env::var_os("CAPACITY_PROCESS_EVIDENCE") {
        std::fs::write(
            std::path::Path::new(&root).join("sampling-pgid"),
            child.id().to_string(),
        )
        .unwrap();
        std::fs::write(std::path::Path::new(&root).join("sampling-ready"), b"ready").unwrap();
    }
    let start = Instant::now();
    let (mut timed_out, mut was_stopped, mut kill_errno) = (false, false, None);
    let mut cancel_at: Option<Instant> = None;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if cancel_at.is_some_and(|at| at.elapsed() >= Duration::from_millis(500)) {
            panic!(
                "sampling helper did not reap: timeout={timed_out}, stopped={was_stopped}, kill_errno={kill_errno:?}, bound_ms={}, elapsed_ms={}",
                bound.as_millis(),
                start.elapsed().as_millis()
            );
        }
        if cancel_at.is_none() {
            was_stopped = stopped();
            timed_out = start.elapsed() >= bound;
        }
        if cancel_at.is_none() && (was_stopped || timed_out) {
            cancel_at = Some(Instant::now());
            if let Err(error) = kill(child.id()) {
                kill_errno = error.raw_os_error();
                // Preserve the group refusal even when direct-child cleanup succeeds.
                child.kill().unwrap_or_else(|error| panic!(
                    "sampling cancellation: group_errno={kill_errno:?}, child={error}, timeout={timed_out}, stopped={was_stopped}"
                ));
            }
            // Reap through the same finite poll; a successful kill is not a wait deadline.
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    use std::io::{Read, Seek};
    let read = |mut file: std::fs::File| {
        file.rewind().unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        bytes
    };
    let mut stderr = read(stderr);
    if !status.success() || timed_out || was_stopped || kill_errno.is_some() {
        stderr.extend_from_slice(format!(
            "\nsampling helper: status={status}, timeout={timed_out}, stopped={was_stopped}, kill_errno={kill_errno:?}, bound_ms={}, elapsed_ms={}\n",
            bound.as_millis(), start.elapsed().as_millis()
        ).as_bytes());
    }
    assert!(
        !status.success() || (!timed_out && !was_stopped && kill_errno.is_none()),
        "sampling helper succeeded after cancellation: {}",
        String::from_utf8_lossy(&stderr)
    );
    Output {
        status,
        stdout: read(stdout),
        stderr,
    }
}
#[test]
fn sampling_helper_deadline_control() {
    use std::os::unix::process::ExitStatusExt;
    let start = Instant::now();
    let result = bounded_output(
        Command::new("/bin/sh").args(["-c", "printf partial; exec sleep 30"]),
        Duration::from_millis(100),
        || false,
    );
    assert_eq!(result.status.signal(), Some(libc::SIGKILL));
    assert_eq!(result.stdout, b"partial");
    assert!(String::from_utf8_lossy(&result.stderr).contains("timeout=true, stopped=false"));
    assert!(start.elapsed() < Duration::from_secs(2));
    for (script, stopped, expected) in [
        ("exit 7", false, "status=exit status: 7"),
        ("kill -TERM $$", false, "status=signal: 15"),
        ("exec sleep 30", true, "timeout=false, stopped=true"),
    ] {
        let result = bounded_output(
            Command::new("/bin/sh").args(["-c", script]),
            Duration::from_secs(2),
            if stopped { || true } else { || false },
        );
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(expected),
            "{result:?}"
        );
    }
    let start = Instant::now();
    let denied = bounded_output_with(
        Command::new("/bin/sh").args(["-c", "exec sleep 30"]),
        Duration::from_secs(2),
        || true,
        |_| Err(std::io::Error::from_raw_os_error(libc::EPERM)),
    );
    assert_eq!(denied.status.signal(), Some(libc::SIGKILL));
    assert!(String::from_utf8_lossy(&denied.stderr).contains("kill_errno=Some(1)"));
    assert!(start.elapsed() < Duration::from_secs(1));
    // A concurrent exit0 after cancellation is not a successful sample.
    let late_success = std::panic::catch_unwind(|| {
        bounded_output_with(
            Command::new("/bin/sh").args(["-c", "sleep 0.05; exit 0"]),
            Duration::from_secs(2),
            || true,
            |_| Ok(()),
        )
    });
    assert!(late_success.is_err());
    let mut pid = 0;
    let start = Instant::now();
    let no_reap = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        bounded_output_with(
            Command::new("/bin/sh").args(["-c", "exec sleep 30"]),
            Duration::from_secs(2),
            || true,
            |child| {
                pid = child;
                Ok(())
            },
        )
    }));
    let elapsed = start.elapsed();
    // The injected no-op intentionally leaves a live child; independently drain it.
    assert!(pid > 0);
    assert_eq!(unsafe { libc::kill(-(pid as i32), libc::SIGKILL) }, 0);
    let began = Instant::now();
    let mut status = 0;
    loop {
        let reaped = unsafe { libc::waitpid(pid as i32, &mut status, libc::WNOHANG) };
        if reaped == pid as i32 {
            break;
        }
        assert_eq!(reaped, 0);
        assert!(began.elapsed() < Duration::from_millis(500));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(unsafe { libc::kill(-(pid as i32), 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
    assert!(elapsed >= Duration::from_millis(500) && elapsed < Duration::from_secs(1));
    let error = no_reap.unwrap_err();
    let message = error.downcast_ref::<String>().unwrap();
    assert!(message.contains("did not reap: timeout=false, stopped=true, kill_errno=None"));
    let (rss, _, _) = sample_bounded(std::process::id());
    assert!(rss > 0);
}

pub fn resource_tree(rows: &[(u32, u32, u64)], root: u32) -> BTreeSet<u32> {
    let mut chosen = BTreeSet::from([root]);
    loop {
        let before = chosen.len();
        for &(pid, parent, _) in rows {
            if chosen.contains(&parent) {
                chosen.insert(pid);
            }
        }
        if chosen.len() == before {
            return chosen;
        }
    }
}
pub fn sample(root: u32) -> (u64, usize, Value) {
    sample_with(root, false, || false)
}
pub fn sample_bounded(root: u32) -> (u64, usize, Value) {
    sample_cancellable(root, || false)
}
pub fn sample_cancellable(root: u32, stopped: fn() -> bool) -> (u64, usize, Value) {
    sample_with(root, true, stopped)
}
fn sample_with(root: u32, bounded: bool, stopped: fn() -> bool) -> (u64, usize, Value) {
    if bounded && std::env::var_os("CAPACITY_SAMPLE_CONTROL").is_some() {
        let allowance = if std::env::var("CAPACITY_SAMPLE_CONTROL").as_deref() == Ok("stop") {
            Duration::from_secs(2)
        } else {
            Duration::from_millis(100)
        };
        let out = bounded_output(
            Command::new("/bin/sh").args(["-c", "exec sleep 30"]),
            allowance,
            stopped,
        );
        assert!(
            out.status.success(),
            "sampling helper deadline exceeded: status={}, {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let mut command = Command::new("/bin/ps");
    command.args(["-axo", "pid=,ppid=,rss="]);
    let out = if bounded {
        bounded_output(&mut command, Duration::from_secs(2), stopped)
    } else {
        command.output().unwrap()
    };
    assert!(
        out.status.success(),
        "ps: status={}, {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<_> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            let fields: Vec<u64> = line
                .split_whitespace()
                .map(|s| s.parse().unwrap())
                .collect();
            assert_eq!(fields.len(), 3);
            (
                u32::try_from(fields[0]).unwrap(),
                u32::try_from(fields[1]).unwrap(),
                fields[2] * 1024,
            )
        })
        .collect();
    let chosen = resource_tree(&rows, root);
    let rows: Vec<_> = rows
        .into_iter()
        .filter(|(pid, _, _)| chosen.contains(pid))
        .collect();
    let rss = rows.iter().map(|(_, _, rss)| rss).sum();
    #[cfg(target_os = "macos")]
    let (fds, descriptor_log) = {
        let observations: Vec<_> = chosen
            .iter()
            .map(|pid| match darwin_descriptors(*pid, stopped) {
                Ok(fds) => json!({"pid":pid,"fds":fds}),
                Err(e)
                    if matches!(
                        e.raw_os_error(),
                        Some(libc::ESRCH | libc::EACCES | libc::EPERM)
                    ) =>
                {
                    json!({"pid":pid,"fds":[],"error":e.to_string(),"errno":e.raw_os_error()})
                }
                Err(e) => panic!("descriptor sample for {pid}: {e}"),
            })
            .collect();
        (
            observations
                .iter()
                .map(|row| row["fds"].as_array().unwrap().len())
                .sum(),
            json!({"source":"proc_pidinfo/PROC_PIDLISTFDS","processes":observations}),
        )
    };
    #[cfg(target_os = "linux")]
    let (fds, descriptor_log) = {
        let counts: Vec<_> = chosen
            .iter()
            .map(|pid| {
                // A process listed by ps can disappear (ENOENT) or briefly deny
                // /proc reads during execve or PID reuse (EACCES) before this
                // sample; the macOS lsof branch tolerates the same race with
                // exit 1. A persistently unreadable tree still yields
                // fd_peak == 0 and fails the caller's missing-observation
                // assertion, and the denial kind stays recorded in the sample.
                let (count, error) = match fs::read_dir(format!("/proc/{pid}/fd")) {
                    Ok(entries) => descriptor_entries(entries)
                        .unwrap_or_else(|e| panic!("descriptor sample for {pid}: {e}")),
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                        ) =>
                    {
                        (0, Some(format!("{:?}", e.kind())))
                    }
                    Err(e) => panic!("descriptor sample for {pid}: {e}"),
                };
                (*pid, count, error)
            })
            .collect();
        (
            counts.iter().map(|(_, count, _)| count).sum(),
            json!(counts),
        )
    };
    (
        rss,
        fds,
        json!({"pids":chosen,"rows_pid_ppid_rss_bytes":rows,"rss_bytes":rss,"numeric_fds":fds,"descriptors":descriptor_log}),
    )
}
// PROC_PIDLISTFDS can fill its buffer while descriptors change. Never credit
// a saturated or malformed result as complete; retries and allocation are finite.
#[cfg(target_os = "macos")]
pub(crate) fn darwin_descriptors(pid: u32, stopped: fn() -> bool) -> std::io::Result<Vec<i32>> {
    use std::io::{Error, ErrorKind};
    let pid = i32::try_from(pid)
        .ok()
        .filter(|p| *p > 0)
        .ok_or_else(|| Error::from(ErrorKind::InvalidInput))?;
    darwin_descriptors_with(stopped, |entries| {
        let bytes = unsafe {
            // Zero descriptors is valid; distinguish it from refusal, not stale errno.
            *libc::__error() = 0;
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDLISTFDS,
                0,
                if entries.is_empty() {
                    std::ptr::null_mut()
                } else {
                    entries.as_mut_ptr().cast()
                },
                i32::try_from(std::mem::size_of_val(entries)).unwrap(),
            )
        };
        let errno = Error::last_os_error();
        if bytes < 0 || (bytes == 0 && errno.raw_os_error() != Some(0)) {
            return Err(if errno.raw_os_error() == Some(0) {
                Error::from(ErrorKind::InvalidData)
            } else {
                errno
            });
        }
        Ok(bytes as usize)
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn darwin_descriptors_with(
    stopped: impl Fn() -> bool,
    mut read: impl FnMut(&mut [libc::proc_fdinfo]) -> std::io::Result<usize>,
) -> std::io::Result<Vec<i32>> {
    use std::io::{Error, ErrorKind};
    let start = Instant::now();
    let width = std::mem::size_of::<libc::proc_fdinfo>();
    let mut room = 32usize;
    for _ in 0..4 {
        if stopped() || start.elapsed() >= Duration::from_secs(2) {
            return Err(Error::from(ErrorKind::Interrupted));
        }
        let needed = read(&mut [])?;
        if !needed.is_multiple_of(width) {
            return Err(Error::from(ErrorKind::InvalidData));
        }
        if stopped() || start.elapsed() >= Duration::from_secs(2) {
            return Err(Error::from(ErrorKind::Interrupted));
        }
        // ponytail: 8 MiB observer ceiling; raise only with measured descriptor demand.
        room = room.max(needed / width + 32);
        if room > 1024 * 1024 {
            return Err(Error::from(ErrorKind::OutOfMemory));
        }
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(room)
            .map_err(|_| Error::from(ErrorKind::OutOfMemory))?;
        entries.resize(
            room,
            libc::proc_fdinfo {
                proc_fd: 0,
                proc_fdtype: 0,
            },
        );
        let bytes = read(&mut entries)?;
        if stopped() || start.elapsed() >= Duration::from_secs(2) {
            return Err(Error::from(ErrorKind::Interrupted));
        }
        if bytes > room * width || !bytes.is_multiple_of(width) {
            return Err(Error::from(ErrorKind::InvalidData));
        }
        if bytes == room * width {
            room *= 2;
            continue;
        }
        let mut fds: Vec<_> = entries[..bytes / width].iter().map(|e| e.proc_fd).collect();
        fds.sort_unstable();
        if fds.iter().any(|fd| *fd < 0) || fds.windows(2).any(|p| p[0] == p[1]) {
            return Err(Error::from(ErrorKind::InvalidData));
        }
        return Ok(fds);
    }
    Err(Error::new(
        ErrorKind::WouldBlock,
        "descriptor table kept filling the sampling buffer",
    ))
}

// Keep vanished/denied fd entries observable; other iterator failures are fatal.
#[cfg(any(target_os = "linux", test))]
fn descriptor_entries<T>(
    entries: impl IntoIterator<Item = std::io::Result<T>>,
) -> std::io::Result<(usize, Option<String>)> {
    use std::io::ErrorKind::{NotFound, PermissionDenied};
    let mut count = 0;
    let mut errors = Vec::new();
    for entry in entries {
        match entry {
            Ok(_) => count += 1,
            Err(e) if matches!(e.kind(), NotFound | PermissionDenied) => {
                errors.push(format!("{:?}", e.kind()));
            }
            Err(e) => return Err(e),
        }
    }
    Ok((count, (!errors.is_empty()).then(|| errors.join(","))))
}
#[test]
fn descriptor_entry_errors_remain_observable() {
    use std::io::{Error, ErrorKind};
    let entries = [
        Ok(()),
        Err(Error::from(ErrorKind::NotFound)),
        Err(Error::from(ErrorKind::PermissionDenied)),
        Ok(()),
    ];
    assert_eq!(
        descriptor_entries(entries).unwrap(),
        (2, Some("NotFound,PermissionDenied".into()))
    );
    assert_eq!(descriptor_entries([Ok(())]).unwrap(), (1, None));
    let denied = [Err::<(), _>(Error::from(ErrorKind::PermissionDenied))];
    assert_eq!(
        descriptor_entries(denied).unwrap(),
        (0, Some("PermissionDenied".into()))
    );
    let failed = [Ok(()), Err(Error::from(ErrorKind::Other))];
    assert_eq!(
        descriptor_entries(failed).unwrap_err().kind(),
        ErrorKind::Other
    );
}
pub fn native_rss(stderr: &str) -> u64 {
    #[cfg(target_os = "macos")]
    let value = stderr
        .lines()
        .find_map(|l| l.trim().strip_suffix(" maximum resident set size"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    #[cfg(target_os = "linux")]
    let value = stderr
        .lines()
        .find_map(|l| l.trim().strip_prefix("Maximum resident set size (kbytes):"))
        .unwrap()
        .trim()
        .parse::<u64>()
        .unwrap()
        * 1024;
    value
}
