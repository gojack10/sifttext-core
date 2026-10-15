use crate::{
    sha256,
    token_metadata::{ENCODING, PauseBeforeAccept, Refresher, TOKENIZER_VERSION},
};
use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

const TEXT: &str = "---\r\nmetadata: \"include all Ω\"\r\n---\r\n# Body\r\nUnicode 漢字 é 👩🏽‍💻\t ordinary file text.\r\n";

// Independent fixed expectations: Python tiktoken 0.14.0 o200k_base,
// vocabulary SHA256 446a9538cb6c348e3516120d7c08b09f57c36495e2acfffe59a5bf8b0cfb1a2d.
// Counts cover complete UTF-8 files, not parsed bodies or product-derived output.
#[test]
fn independent_exact_counts_include_metadata_unicode_and_special_looking_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.md");
    for (text, tokens) in [
        ("", 0),
        (" ", 1),
        ("\r\n", 1),
        ("Ω漢字 é 👩🏽‍💻", 12),
        ("ordinary file text", 3),
        ("x\t\ny\\z\r", 6),
        (" metadata: \"all\"\n", 5),
        ("<|endoftext|>", 7),
    ] {
        fs::write(&path, text).unwrap();
        let got = Refresher::default().refresh(&path, None).unwrap();
        assert_eq!(got.status, "current");
        assert_eq!(got.measurement.tokens, tokens, "{text:?}");
        assert_eq!(fs::read(&path).unwrap(), text.as_bytes());
    }
}

fn ready(path: &Path) {
    let start = Instant::now();
    while !path.exists() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "native barrier not reached"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn exact_file_identity_reuse_edits_and_untrusted_sidecars() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.md");
    let meta = dir.path().join("node.meta.json");
    fs::write(&path, TEXT).unwrap();
    let mut engine = Refresher::default();
    let start = Instant::now();
    let first = engine.refresh(&path, None).unwrap();
    let cold = start.elapsed();
    assert_eq!(first.status, "current");
    assert_eq!(first.measurement.tokens, 32);
    assert!(!first.reused);
    assert_eq!(first.measurement.hash, sha256(TEXT.as_bytes()));
    assert_eq!(first.measurement.encoding, ENCODING);
    assert_eq!(first.measurement.tokenizer_version, TOKENIZER_VERSION);
    let sidecar = fs::read(&meta).unwrap();
    let start = Instant::now();
    for _ in 0..16 {
        let reused = engine.refresh(&path, None).unwrap();
        assert!(reused.reused);
        assert_eq!(reused.measurement, first.measurement);
        assert_eq!(fs::read(&meta).unwrap(), sidecar);
    }
    println!(
        "TOKEN REUSE: source_bytes={} hash={} tokens={} version={} cold_us={} warm_16_us={}",
        TEXT.len(),
        first.measurement.hash,
        first.measurement.tokens,
        TOKENIZER_VERSION,
        cold.as_micros(),
        start.elapsed().as_micros()
    );
    // A plausible forged count with matching source hash and tokenizer identity is
    // still untrusted. A new consumer must compute, the live consumer uses its own cache.
    let mut forged = serde_json::to_value(&first.measurement).unwrap();
    forged["tokens"] = serde_json::json!(0);
    fs::write(&meta, serde_json::to_vec(&forged).unwrap()).unwrap();
    assert_eq!(
        engine.refresh(&path, None).unwrap().measurement,
        first.measurement
    );
    fs::write(&meta, serde_json::to_vec(&forged).unwrap()).unwrap();
    let new_consumer = Refresher::default().refresh(&path, None).unwrap();
    assert!(!new_consumer.reused);
    assert_eq!(new_consumer.measurement, first.measurement);
    fs::remove_file(&meta).unwrap();
    assert!(engine.refresh(&path, None).unwrap().reused);
    assert_eq!(fs::read(&meta).unwrap(), sidecar);
    for text in [
        TEXT.replace("include all", "metadata-only edit"),
        format!("{TEXT} \n"),
    ] {
        fs::write(&path, &text).unwrap();
        let result = engine.refresh(&path, None).unwrap();
        assert!(!result.reused);
        assert_eq!(result.measurement.tokens, 33);
        assert_eq!(result.measurement.hash, sha256(text.as_bytes()));
        assert_eq!(fs::read(&path).unwrap(), text.as_bytes());
    }
    fs::write(&path, TEXT).unwrap();
    assert_eq!(
        engine.refresh(&path, None).unwrap().measurement,
        first.measurement
    );
    assert_eq!(fs::read(&path).unwrap(), TEXT.as_bytes());
}

#[test]
fn old_candidate_cannot_overwrite_newer_sidecar_or_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.md");
    let meta = dir.path().join("node.meta.json");
    fs::write(&path, TEXT).unwrap();
    let prior = Refresher::default().refresh(&path, None).unwrap();
    let prior_bytes = fs::read(&meta).unwrap();
    let barrier_dir = tempfile::tempdir().unwrap();
    let r = barrier_dir.path().join("ready");
    let release = barrier_dir.path().join("release");
    let p = path.clone();
    let pause = PauseBeforeAccept {
        ready: r.clone(),
        release: release.clone(),
    };
    let old = thread::spawn(move || Refresher::default().refresh(&p, Some(&pause)).unwrap());
    ready(&r);
    fs::write(&path, "newer Ω version").unwrap();
    let p = path.clone();
    let new = thread::spawn(move || Refresher::default().refresh(&p, None).unwrap());
    fs::write(&release, "release").unwrap();
    let stale = old.join().unwrap();
    assert_eq!(stale.status, "stale");
    assert_eq!(stale.measurement, prior.measurement);
    let current = new.join().unwrap();
    assert_eq!(current.status, "current");
    assert_eq!(current.measurement.tokens, 4);
    assert_eq!(current.measurement.hash, sha256(b"newer \xce\xa9 version"));
    assert_ne!(fs::read(&meta).unwrap(), prior_bytes);
    assert_eq!(fs::read(&path).unwrap(), "newer Ω version".as_bytes());
}

#[test]
fn replacement_inode_and_unsafe_destinations_reject_without_following() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.md");
    let meta = dir.path().join("node.meta.json");
    let foreign = dir.path().join("foreign");
    fs::write(&foreign, b"do not overwrite").unwrap();
    symlink(&foreign, &path).unwrap();
    assert!(Refresher::default().refresh(&path, None).is_err());
    fs::remove_file(&path).unwrap();
    fs::hard_link(&foreign, &path).unwrap();
    assert!(Refresher::default().refresh(&path, None).is_err());
    fs::remove_file(&path).unwrap();
    fs::write(&path, TEXT).unwrap();
    symlink(&foreign, &meta).unwrap();
    assert!(Refresher::default().refresh(&path, None).is_err());
    fs::remove_file(&meta).unwrap();
    fs::hard_link(&foreign, &meta).unwrap();
    assert!(Refresher::default().refresh(&path, None).is_err());
    fs::remove_file(&meta).unwrap();
    assert_eq!(fs::read(&foreign).unwrap(), b"do not overwrite");
    let barrier_dir = tempfile::tempdir().unwrap();
    let r = barrier_dir.path().join("ready");
    let release = barrier_dir.path().join("release");
    let p = path.clone();
    let pause = PauseBeforeAccept {
        ready: r.clone(),
        release: release.clone(),
    };
    let old = thread::spawn(move || Refresher::default().refresh(&p, Some(&pause)).unwrap());
    ready(&r);
    let replacement = dir.path().join("replacement");
    fs::write(&replacement, TEXT).unwrap();
    fs::rename(&replacement, &path).unwrap();
    fs::write(&release, "release").unwrap();
    assert_eq!(old.join().unwrap().status, "stale");
    assert!(!meta.exists());
    assert_eq!(fs::read(&path).unwrap(), TEXT.as_bytes());
}

#[test]
fn simultaneous_refresh_and_logically_separate_worktrees() {
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let a = dir.path().join("node.md");
    let b = other.path().join("node.md");
    fs::write(&a, TEXT).unwrap();
    fs::write(&b, "different working version").unwrap();
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let p = a.clone();
            thread::spawn(move || Refresher::default().refresh(&p, None).unwrap())
        })
        .collect();
    for w in workers {
        let result = w.join().unwrap();
        assert_eq!(result.status, "current");
        assert_eq!(result.measurement.hash, sha256(TEXT.as_bytes()));
    }
    let mut engine = Refresher::default();
    assert!(!engine.refresh(&a, None).unwrap().reused);
    let different = engine.refresh(&b, None).unwrap();
    assert!(!different.reused);
    assert_eq!(different.measurement.tokens, 3);
    assert_eq!(
        different.measurement.hash,
        sha256(b"different working version")
    );
    assert_eq!(fs::read(&a).unwrap(), TEXT.as_bytes());
    assert_eq!(fs::read(&b).unwrap(), b"different working version");
}

#[test]
fn unreleased_barrier_expires_without_publication_and_releases_lock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.md");
    fs::write(&path, TEXT).unwrap();
    let meta = dir.path().join("node.meta.json");
    let mut engine = Refresher::default();
    engine.refresh(&path, None).unwrap();
    let previous = fs::read(&meta).unwrap();
    fs::write(&path, "newer Ω version").unwrap();
    let barrier = tempfile::tempdir().unwrap();
    let pause = PauseBeforeAccept {
        ready: barrier.path().join("ready"),
        release: barrier.path().join("release"),
    };
    // Rescue a broken deadline so the regression fails instead of hanging Cargo.
    let (done, waiting) = std::sync::mpsc::channel();
    let release = pause.release.clone();
    thread::scope(|scope| {
        scope.spawn(move || {
            if waiting.recv_timeout(Duration::from_secs(20)).is_err() {
                fs::write(release, "rescue").unwrap();
            }
        });
        let result = engine.refresh(&path, Some(&pause));
        let _ = done.send(());
        assert_eq!(result.unwrap_err().to_string(), "barrier expired");
    });
    assert!(pause.ready.exists());
    assert!(!pause.release.exists());
    assert_eq!(fs::read(&meta).unwrap(), previous);
    assert_eq!(fs::read(&path).unwrap(), "newer Ω version".as_bytes());
    assert_eq!(
        Refresher::default().refresh(&path, None).unwrap().status,
        "current"
    );
}

#[test]
fn invalid_utf8_oversize_and_barrier_cannot_modify_authored_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.md");
    for bytes in [vec![0xff], vec![b'x'; crate::LIMIT + 1]] {
        fs::write(&path, &bytes).unwrap();
        assert!(Refresher::default().refresh(&path, None).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    fs::write(&path, TEXT).unwrap();
    let pause = PauseBeforeAccept {
        ready: path.clone(),
        release: dir.path().join("release"),
    };
    assert!(Refresher::default().refresh(&path, Some(&pause)).is_err());
    assert_eq!(fs::read(&path).unwrap(), TEXT.as_bytes());
}
