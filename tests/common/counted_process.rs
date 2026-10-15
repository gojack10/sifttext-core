//! Bounded fresh-process observations: sticky Stop, one cancellation, verified drain.
use std::{
    fs,
    os::unix::process::{CommandExt, ExitStatusExt},
    path::Path,
    process::{Child, Command, ExitStatus},
    sync::{
        Once,
        atomic::{AtomicI32, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
static STOP: AtomicI32 = AtomicI32::new(0);
static SIGNALS: Once = Once::new();
extern "C" fn stop(signal: libc::c_int) {
    let _ = STOP.compare_exchange(0, signal, Ordering::Relaxed, Ordering::Relaxed);
}
fn alive(pid: u32) -> bool {
    if unsafe { libc::kill(-(pid as i32), 0) } == 0 {
        return true;
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => false,
        Some(libc::EPERM) => true,
        error => panic!("group probe failed: {error:?}"),
    }
}
struct Group {
    child: Child,
    cancelled: bool,
    forced: bool,
    drained: bool,
}
impl Group {
    fn signal(&mut self, signal: i32) {
        if !self.drained {
            let result = unsafe { libc::kill(-(self.child.id() as i32), signal) };
            assert!(
                result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
            );
        }
    }
    fn cancel(&mut self) {
        if !self.cancelled {
            self.signal(libc::SIGTERM);
            self.cancelled = true;
        }
    }
    fn force(&mut self) {
        if !self.forced {
            self.signal(libc::SIGKILL);
            self.forced = true;
        }
    }
}
impl Drop for Group {
    fn drop(&mut self) {
        self.force();
        let _ = self.child.wait();
    }
}
pub fn run(
    command: &mut Command,
    bound: Duration,
    prefix: &Path,
) -> (ExitStatus, serde_json::Value) {
    SIGNALS.call_once(|| unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = stop as *const () as usize;
        libc::sigemptyset(&mut action.sa_mask);
        for signal in [libc::SIGTERM, libc::SIGINT] {
            assert_eq!(libc::sigaction(signal, &action, std::ptr::null_mut()), 0);
        }
    });
    assert_eq!(
        STOP.load(Ordering::Relaxed),
        0,
        "runner stopped; no next request"
    );
    let started = Instant::now();
    let mut group = Group {
        child: command.process_group(0).spawn().unwrap(),
        cancelled: false,
        forced: false,
        drained: false,
    };
    let pid = group.child.id();
    fs::write(
        prefix.with_extension("start.json"),
        serde_json::json!({"pid":pid,"pgid":pid}).to_string(),
    )
    .unwrap();
    let mut status = None;
    let mut cancel_at = None;
    let mut signal = 0;
    let mut timeout = false;
    loop {
        let observed = STOP.load(Ordering::Relaxed);
        if cancel_at.is_none() && (observed != 0 || started.elapsed() >= bound) {
            signal = observed;
            timeout = observed == 0;
            group.cancel();
            cancel_at = Some(Instant::now());
        }
        if status.is_none() {
            status = group.child.try_wait().unwrap();
        }
        group.drained = status.is_some() && !alive(pid);
        if group.drained {
            break;
        }
        // Give a nested supervisor time to drain before the enclosing one-second Stop ceiling.
        if cancel_at.is_some_and(|t| t.elapsed() >= Duration::from_millis(250)) {
            group.force();
        }
        if cancel_at.is_some_and(|t| t.elapsed() >= Duration::from_millis(750)) {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let receipt = serde_json::json!({"pid":pid,"pgid":pid,"drained":group.drained,
        "exit":status.and_then(|s|s.code()),"signal":status.and_then(|s|s.signal()),
        "cancelled_signal":signal,"runner_timeout":timeout,"runner_bound_ms":bound.as_millis(),
        "ms":started.elapsed().as_secs_f64()*1000.});
    fs::write(
        prefix.with_extension("process.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    assert!(group.drained, "request group did not drain: {receipt}");
    assert_eq!(signal, 0, "runner stopped: {receipt}");
    assert!(!timeout, "request timed out: {receipt}");
    (status.unwrap(), receipt)
}
