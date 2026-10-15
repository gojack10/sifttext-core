// Split the existing bounded request at launch, retaining concurrent/barrier controls.
pub(super) struct ObserverJob {
    pending: Option<PendingRequest>,
    evidence: Option<TempDir>,
}
impl ObserverJob {
    pub(super) fn start(command: Command, input: Vec<u8>) -> Self {
        let evidence = match std::env::var_os("SHARED_OBSERVER_EVIDENCE") {
            Some(root) => tempfile::Builder::new()
                .prefix("lifetime-")
                .tempdir_in(root)
                .unwrap(),
            None => TempDir::new().unwrap(),
        };
        let pending = start_request(
            command,
            input,
            &evidence.path().join("request"),
            Duration::from_secs(900),
        );
        Self {
            pending: Some(pending),
            evidence: Some(evidence),
        }
    }
    pub(super) fn check_active(&self) {
        assert_eq!(
            STOP.load(Ordering::Relaxed),
            0,
            "runner stopped; no next action"
        );
        assert!(
            self.pending.as_ref().unwrap().start.elapsed() < self.pending.as_ref().unwrap().bound,
            "observer lifetime deadline"
        );
    }
    pub(super) fn running(&mut self) -> bool {
        self.check_active();
        self.pending
            .as_mut()
            .unwrap()
            .group
            .child
            .try_wait()
            .unwrap()
            .is_none()
    }
    pub(super) fn kill(&mut self) {
        self.check_active();
        // Intentional killed-publication controls retain their direct-child SIGKILL.
        self.pending.as_mut().unwrap().group.child.kill().unwrap();
    }
    pub(super) fn finish(mut self) -> Output {
        let pending = self.pending.take().unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            finish_request(pending, None, Duration::from_millis(50)).0
        }));
        match result {
            Ok(out) => out,
            Err(error) => {
                eprintln!(
                    "failed observer evidence: {}",
                    self.evidence.take().unwrap().keep().display()
                );
                std::panic::resume_unwind(error)
            }
        }
    }
}
impl Drop for ObserverJob {
    fn drop(&mut self) {
        if let Some(mut pending) = self.pending.take() {
            // Unwinding a readiness/action assertion must also drain other live jobs.
            let already_done = pending.group.child.try_wait().ok().flatten();
            if already_done.is_some() && !pending.group.alive() {
                pending.group.cancelled = true;
            } else {
                pending.group.cancel();
            }
            let mut status = already_done;
            let began = Instant::now();
            loop {
                status = status.or_else(|| pending.group.child.try_wait().ok().flatten());
                if (status.is_some() && !pending.group.alive())
                    || began.elapsed() >= Duration::from_millis(500)
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let drained = status.is_some() && !pending.group.alive();
            let receipt = json!({"pgid":pending.group.child.id(),"drained":drained,"abandoned":true,
                "cancel_errno":pending.group.cancel_errno,"child_kill_errno":pending.group.child_kill_errno,
                "probe_errno":pending.group.probe_errno,
                "cancelled_signal":STOP.load(Ordering::Relaxed),"ms":pending.start.elapsed().as_secs_f64()*1000.});
            let _ = fs::write(
                pending.prefix.with_extension("receipt.json"),
                receipt.to_string(),
            );
            if pending.writer.is_finished() {
                let _ = pending.writer.join();
            }
            eprintln!(
                "abandoned observer {receipt}; evidence: {}",
                self.evidence.take().unwrap().keep().display()
            );
        }
    }
}
