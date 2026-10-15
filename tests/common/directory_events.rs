use std::{
    ffi::{CStr, CString},
    path::{Path, PathBuf},
};

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::{
        ffi::{c_char, c_void},
        sync::Mutex,
    };
    type Handle = *mut c_void;
    type Callback = unsafe extern "C" fn(Handle, Handle, usize, Handle, *const u32, *const u64);
    #[repr(C)]
    struct Context {
        version: isize,
        info: Handle,
        retain: Handle,
        release: Handle,
        describe: Handle,
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithCString(
            allocator: Handle,
            text: *const c_char,
            encoding: u32,
        ) -> Handle;
        fn CFArrayCreate(
            allocator: Handle,
            values: *const Handle,
            count: isize,
            callbacks: Handle,
        ) -> Handle;
        fn CFRelease(object: Handle);
    }
    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn FSEventStreamCreate(
            allocator: Handle,
            callback: Callback,
            context: *mut Context,
            paths: Handle,
            since: u64,
            latency: f64,
            flags: u32,
        ) -> Handle;
        fn FSEventStreamSetDispatchQueue(stream: Handle, queue: Handle);
        fn FSEventStreamStart(stream: Handle) -> u8;
        fn FSEventStreamFlushSync(stream: Handle);
        fn FSEventStreamStop(stream: Handle);
        fn FSEventStreamInvalidate(stream: Handle);
        fn FSEventStreamRelease(stream: Handle);
        fn dispatch_queue_create(label: *const c_char, attr: Handle) -> Handle;
        fn dispatch_release(queue: Handle);
        fn dispatch_sync_f(queue: Handle, context: Handle, work: unsafe extern "C" fn(Handle));
    }
    unsafe extern "C" fn collect(
        _: Handle,
        context: Handle,
        count: usize,
        paths: Handle,
        flags: *const u32,
        _: *const u64,
    ) {
        let events = unsafe { &*(context as *const Mutex<Vec<(PathBuf, u32)>>) };
        // No UseCFTypes flag: eventPaths is an array of NUL-terminated paths.
        for i in 0..count {
            let path = unsafe { CStr::from_ptr(*(paths as *const *const c_char).add(i)) };
            events
                .lock()
                .unwrap()
                .push((PathBuf::from(path.to_str().unwrap()), unsafe {
                    *flags.add(i)
                }));
        }
    }
    pub struct Watch {
        stream: Handle,
        queue: Handle,
        events: Box<Mutex<Vec<(PathBuf, u32)>>>,
        root: PathBuf,
    }
    impl Watch {
        pub fn new(path: &Path) -> Self {
            let mut events = Box::new(Mutex::new(Vec::new()));
            let root = path.to_owned();
            let path = CString::new(path.to_str().unwrap()).unwrap();
            let null = std::ptr::null_mut();
            unsafe {
                let string = CFStringCreateWithCString(null, path.as_ptr(), 0x08000100);
                assert!(!string.is_null());
                let paths = CFArrayCreate(null, &string, 1, null);
                assert!(!paths.is_null());
                let mut context = Context {
                    version: 0,
                    info: (&mut *events as *mut Mutex<_>).cast(),
                    retain: null,
                    release: null,
                    describe: null,
                };
                // SinceNow; NoDefer | FileEvents. Watch before the native operation.
                let stream =
                    FSEventStreamCreate(null, collect, &mut context, paths, u64::MAX, 0.01, 0x12);
                CFRelease(paths);
                CFRelease(string);
                assert!(!stream.is_null());
                let queue = dispatch_queue_create(c"outline-test-events".as_ptr(), null);
                assert!(!queue.is_null());
                FSEventStreamSetDispatchQueue(stream, queue);
                let watch = Self {
                    stream,
                    queue,
                    events,
                    root,
                };
                assert_ne!(FSEventStreamStart(stream), 0);
                watch
            }
        }
        pub fn finish(self) -> Vec<PathBuf> {
            // FlushSync drains stream buffers, not events still in the kernel.
            // Wait for a later filesystem marker to establish actual delivery.
            // Darwin delivers resolved paths, even when callers watch an alias.
            let root = self.root.canonicalize().unwrap();
            let marker = tempfile::NamedTempFile::new_in(&root).unwrap();
            let start = std::time::Instant::now();
            while !self
                .events
                .lock()
                .unwrap()
                .iter()
                .any(|(path, _)| path == marker.path())
            {
                assert!(
                    start.elapsed() < std::time::Duration::from_secs(10),
                    "missing filesystem delivery marker"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            unsafe {
                FSEventStreamFlushSync(self.stream);
            }
            let events = self.events.lock().unwrap().clone();
            // MustScanSubDirs | UserDropped | KernelDropped: observation is inconclusive.
            assert!(
                events.iter().all(|(_, flags)| flags & 7 == 0),
                "filesystem event delivery lost: {events:?}"
            );
            events
                .into_iter()
                .map(|(path, _)| self.root.join(path.strip_prefix(&root).unwrap()))
                .collect()
        }
    }
    unsafe extern "C" fn drained(_: Handle) {}
    impl Drop for Watch {
        fn drop(&mut self) {
            unsafe {
                FSEventStreamStop(self.stream);
                FSEventStreamInvalidate(self.stream);
                // Drain callbacks before releasing their boxed context.
                dispatch_sync_f(self.queue, std::ptr::null_mut(), drained);
                FSEventStreamRelease(self.stream);
                dispatch_release(self.queue);
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::{
        io,
        os::fd::{AsRawFd, FromRawFd, OwnedFd},
    };
    pub struct Watch {
        fd: OwnedFd,
        root: PathBuf,
    }
    impl Watch {
        pub fn new(path: &Path) -> Self {
            let raw = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
            assert!(raw >= 0, "{}", io::Error::last_os_error());
            let fd = unsafe { OwnedFd::from_raw_fd(raw) };
            let cpath = CString::new(path.to_str().unwrap()).unwrap();
            let wd = unsafe {
                libc::inotify_add_watch(
                    raw,
                    cpath.as_ptr(),
                    libc::IN_CREATE | libc::IN_DELETE | libc::IN_MOVED_FROM | libc::IN_MOVED_TO,
                )
            };
            assert!(wd >= 0, "{}", io::Error::last_os_error());
            Self {
                fd,
                root: path.to_owned(),
            }
        }
        pub fn finish(self) -> Vec<PathBuf> {
            let mut paths = Vec::new();
            let mut buffer = [0u8; 65536];
            loop {
                let count = unsafe {
                    libc::read(
                        self.fd.as_raw_fd(),
                        buffer.as_mut_ptr().cast(),
                        buffer.len(),
                    )
                };
                if count < 0 {
                    let error = io::Error::last_os_error();
                    if error.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    assert_eq!(error.kind(), io::ErrorKind::WouldBlock, "{error}");
                    break;
                }
                assert!(count > 0, "unexpected end of event stream");
                let mut offset = 0;
                while offset < count as usize {
                    let header = size_of::<libc::inotify_event>();
                    assert!(offset + header <= count as usize);
                    let event = unsafe {
                        buffer
                            .as_ptr()
                            .add(offset)
                            .cast::<libc::inotify_event>()
                            .read_unaligned()
                    };
                    assert_eq!(
                        event.mask & (libc::IN_Q_OVERFLOW | libc::IN_IGNORED | libc::IN_UNMOUNT),
                        0,
                        "lost filesystem events"
                    );
                    let end = offset + header + event.len as usize;
                    assert!(end <= count as usize);
                    if event.len != 0 {
                        let name =
                            CStr::from_bytes_until_nul(&buffer[offset + header..end]).unwrap();
                        paths.push(self.root.join(name.to_str().unwrap()));
                    }
                    offset = end;
                }
            }
            paths
        }
    }
}
pub use platform::Watch;

#[test]
fn directory_alias_preserves_transient_event_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("real");
    std::fs::create_dir(&root).unwrap();
    let alias = temp.path().join("alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let stage = alias.join("outline-stage");
    let watch = Watch::new(&alias);
    std::fs::create_dir(&stage).unwrap();
    std::fs::remove_dir(&stage).unwrap();
    assert!(watch.finish().contains(&stage));
}
