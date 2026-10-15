//! One registration for the Darwin parity fixture, not one per sampler consumer.
#![cfg(target_os = "macos")]
#[path = "common/native_resources.rs"]
#[allow(dead_code)]
mod resources;
use resources::{darwin_descriptors, darwin_descriptors_with};
use std::{
    io::{BufRead, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn descriptor_refusal_growth_and_inflight_stop() {
    use std::{
        cell::Cell,
        io::{Error, ErrorKind},
    };
    let width = std::mem::size_of::<libc::proc_fdinfo>();
    for errno in [libc::EACCES, libc::EPERM, libc::ESRCH, libc::EIO] {
        for fail_fetch in [false, true] {
            let error = darwin_descriptors_with(
                || false,
                |entries| {
                    if entries.is_empty() && fail_fetch {
                        Ok(width)
                    } else {
                        Err(Error::from_raw_os_error(errno))
                    }
                },
            )
            .unwrap_err();
            assert_eq!(error.raw_os_error(), Some(errno));
        }
    }
    for malformed_query in [true, false] {
        assert_eq!(
            darwin_descriptors_with(
                || false,
                |entries| {
                    Ok(if entries.is_empty() && !malformed_query {
                        width
                    } else {
                        1
                    })
                }
            )
            .unwrap_err()
            .kind(),
            ErrorKind::InvalidData
        );
    }
    for fds in [[-1, 2], [2, 2]] {
        assert_eq!(
            darwin_descriptors_with(
                || false,
                |entries| {
                    if !entries.is_empty() {
                        for (entry, fd) in entries.iter_mut().zip(fds) {
                            entry.proc_fd = fd;
                        }
                    }
                    Ok(2 * width)
                }
            )
            .unwrap_err()
            .kind(),
            ErrorKind::InvalidData
        );
    }
    assert_eq!(
        darwin_descriptors_with(
            || false,
            |entries| {
                Ok(if entries.is_empty() {
                    width
                } else {
                    std::mem::size_of_val(entries) + width
                })
            }
        )
        .unwrap_err()
        .kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(
        darwin_descriptors_with(|| false, |_| Ok(1024 * 1024 * width))
            .unwrap_err()
            .kind(),
        ErrorKind::OutOfMemory
    );
    let calls = Cell::new(0);
    assert_eq!(
        darwin_descriptors_with(
            || false,
            |entries| {
                calls.set(calls.get() + 1);
                Ok(if entries.is_empty() {
                    width
                } else {
                    std::mem::size_of_val(entries)
                })
            }
        )
        .unwrap_err()
        .kind(),
        ErrorKind::WouldBlock
    );
    assert_eq!(calls.get(), 8);
    let calls = Cell::new(0);
    assert_eq!(
        darwin_descriptors_with(
            || false,
            |entries| {
                calls.set(calls.get() + 1);
                if entries.is_empty() {
                    return Ok(width);
                }
                if calls.get() == 2 {
                    return Ok(std::mem::size_of_val(entries));
                }
                entries[0].proc_fd = 9;
                entries[1].proc_fd = 3;
                Ok(2 * width)
            }
        )
        .unwrap(),
        [3, 9]
    );
    assert_eq!(calls.get(), 4);
    for stop_on_call in [1, 2] {
        let calls = Cell::new(0);
        assert_eq!(
            darwin_descriptors_with(
                || calls.get() == stop_on_call,
                |_| {
                    calls.set(calls.get() + 1);
                    Ok(width)
                }
            )
            .unwrap_err()
            .kind(),
            ErrorKind::Interrupted
        );
        assert_eq!(calls.get(), stop_on_call);
    }
}

#[test]
fn descriptors_remain_complete_during_concurrent_growth() {
    use std::{fs::File, os::fd::AsRawFd, sync::mpsc};
    let (tx, rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(move || {
            let mut files = Vec::new();
            for _ in 0..16 {
                files.extend((0..32).map(|_| File::open("/dev/null").unwrap()));
                tx.send(files.iter().map(AsRawFd::as_raw_fd).collect::<Vec<_>>())
                    .unwrap();
                std::thread::sleep(Duration::from_millis(2));
            }
            let _ = done_rx.recv_timeout(Duration::from_secs(2));
        });
        let mut observations = Vec::new();
        for _ in 0..16 {
            let held = rx.recv_timeout(Duration::from_secs(2)).unwrap();
            let observed = darwin_descriptors(std::process::id(), || false).unwrap();
            assert!(held.iter().all(|fd| observed.contains(fd)));
            assert!(observed.windows(2).all(|pair| pair[0] < pair[1]));
            observations.push(observed.len());
        }
        assert!(
            observations[0] < *observations.last().unwrap(),
            "{observations:?}"
        );
        done_tx.send(()).unwrap();
        eprintln!("concurrent descriptor counts: {observations:?}");
    });
}

#[test]
fn descriptor_parity_growth_exit_and_stop() {
    struct Fixture(std::process::Child);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut child = Fixture(Command::new("/bin/bash")
        .args(["--noprofile", "--norc", "-c",
            "exec 3</dev/null; exec 9>/dev/null; echo ready; read -r x; for ((i=20;i<532;i++)); do eval \"exec $i</dev/null\"; done; echo grown; read -r x"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().unwrap());
    let pid = child.0.id();
    let mut output = std::io::BufReader::new(child.0.stdout.take().unwrap());
    for phase in ["ready", "grown"] {
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), phase);
        for _ in 0..5 {
            let start = Instant::now();
            let baseline = Command::new("/usr/sbin/lsof")
                .args(["-nP", "-a", "-p", &pid.to_string(), "-Fpf"])
                .output()
                .unwrap();
            let baseline_us = start.elapsed().as_micros();
            assert!(baseline.status.success());
            let mut expected: Vec<i32> = String::from_utf8(baseline.stdout)
                .unwrap()
                .lines()
                .filter_map(|s| s.strip_prefix('f')?.parse().ok())
                .collect();
            expected.sort_unstable();
            let start = Instant::now();
            let observed = darwin_descriptors(pid, || false).unwrap();
            eprintln!(
                "descriptor parity {phase}: fds={} lsof_us={baseline_us} libproc_us={}",
                observed.len(),
                start.elapsed().as_micros()
            );
            assert_eq!(observed, expected);
            assert!(observed.contains(&3) && observed.contains(&9));
            if phase == "grown" {
                assert!((20..532).all(|fd| observed.contains(&fd)));
            }
        }
        if phase == "ready" {
            child
                .0
                .stdin
                .as_mut()
                .unwrap()
                .write_all(b"grow\n")
                .unwrap();
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let ready = dir.path().join("empty-ready");
    let mut empty = Fixture(
        Command::new("/bin/sh")
            .args([
                "-c",
                "exec 0<&-; exec 1>&-; exec 2>&-; printf ready >\"$1\"; exec sleep 30",
                "empty",
            ])
            .arg(&ready)
            .spawn()
            .unwrap(),
    );
    let start = Instant::now();
    while !ready.exists()
        || !darwin_descriptors(empty.0.id(), || false)
            .unwrap()
            .is_empty()
    {
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(empty.0.try_wait().unwrap().is_none());
        std::thread::sleep(Duration::from_millis(5));
    }
    // A previous refused call must not taint the valid zero-descriptor reply.
    unsafe {
        *libc::__error() = libc::EACCES;
    }
    assert!(
        darwin_descriptors(empty.0.id(), || false)
            .unwrap()
            .is_empty()
    );
    let start = Instant::now();
    assert_eq!(
        darwin_descriptors(pid, || true).unwrap_err().kind(),
        std::io::ErrorKind::Interrupted
    );
    assert!(start.elapsed() < Duration::from_millis(100));
    assert_eq!(
        darwin_descriptors(0, || false).unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    assert_eq!(
        darwin_descriptors(pid, || false)
            .unwrap_err()
            .raw_os_error(),
        Some(libc::ESRCH)
    );
}
