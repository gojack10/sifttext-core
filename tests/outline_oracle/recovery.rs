// Deterministic pre-accept races and killed recovery from incremental_outlines.mjs.
// Native results establish freshness only; authored records own the row/byte/cost oracle.
use std::os::unix::process::ExitStatusExt;
use std::{
    process::{Child, Output},
    time::{Duration, Instant},
};

// Always reap the paused child, including when a barrier or mutation assertion fails.
struct PausedIndex(Option<Child>);
impl Drop for PausedIndex {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
fn paused_index(store: &Path, label: &str, act: impl FnOnce(), kill: bool) -> Output {
    let barrier = TempDir::new().unwrap();
    let ready = barrier.path().join("ready");
    let release = barrier.path().join("release");
    let bin = std::env::var_os("CORE_OBSERVE_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/core-observe"));
    assert!(bin.is_file(), "build core-observe first: {}", bin.display());
    let mut command = Command::new(&bin);
    command.current_dir(bin.parent().unwrap().parent().unwrap().parent().unwrap());
    let mut guard = capacity::ObserverJob::start(
        command,
        json!({"op":"index","store":store,
        "mode":if label == "killed-full" { "full" } else { "incremental" },
        "pause_before_accept":{"ready":ready,"release":release}})
        .to_string()
        .into_bytes(),
    );
    let start = Instant::now();
    while !ready.exists() {
        assert!(guard.running(), "{label}: exited before barrier");
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{label}: missing barrier"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    guard.check_active();
    act();
    guard.check_active();
    if kill {
        guard.kill();
    } else {
        fs::write(release, "go").unwrap();
    }
    let result = guard.finish();
    eprintln!(
        "{label}: exit={} signal={:?} stdout={} stderr={}",
        result.status,
        result.status.signal(),
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    if kill {
        assert_eq!(result.status.signal(), Some(9), "{label}");
        assert!(
            result.stdout.is_empty(),
            "{label}: killed computation claimed a result"
        );
    } else {
        assert!(
            !result.status.success(),
            "{label}: raced computation accepted"
        );
        if label == "cache-replacement" {
            assert!(result.stdout.is_empty(), "missing cache advertised a view");
            let error: Value = serde_json::from_slice(&result.stderr).unwrap();
            assert_eq!(error, json!({"kind":"not_found","status":"error"}));
            return result;
        }
        let view: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(view["status"], "stale", "{label}");
        assert!(
            view["version"].is_null(),
            "{label}: refused version advertised"
        );
        assert_eq!(view["nodes"], json!([]));
        assert_eq!(view["links"], json!([]));
    }
    result
}
fn replace_identical(p: &Path) {
    let replacement = p.with_file_name(format!(
        "{}.replacement",
        p.file_name().unwrap().to_string_lossy()
    ));
    fs::write(&replacement, fs::read(p).unwrap()).unwrap();
    fs::rename(replacement, p).unwrap();
}
#[test]
fn incremental_races_and_killed_recovery_preserve_complete_authored_views() {
    let bpe = ORACLE_BPE.clone();
    let dir = TempDir::new().unwrap();
    let store = fs::canonicalize(dir.path()).unwrap().join("store");
    materialize(&store, &dependency_specimen());
    let (baseline, _) = incremental_call(&store);
    assert_dependency_equality(&store, &baseline, &bpe);
    for label in [
        "source-replacement",
        "artifact-replacement",
        "sidecar-replacement",
        "staged-tamper",
        "cache-replacement",
        "killed-warm",
        "killed-full",
        "killed-changed",
    ] {
        if label == "killed-changed" {
            upsert_dependency_field(&store, 3, "warnings", json!("Before killed change"));
        }
        let authored = authored_snapshot(&store);
        let generated = generated_snapshot(&store);
        let source = store.join("nodes").join(incremental_id(3));
        let stage = store.join(".sifttext/outline-stage");
        paused_index(
            &store,
            label,
            || match label {
                "source-replacement" => replace_identical(&source.join("node.md")),
                "artifact-replacement" => replace_identical(&source.join("outline.full.lean")),
                "sidecar-replacement" => replace_identical(&source.join("outline.lean.meta.json")),
                "staged-tamper" => {
                    let p = fs::read_dir(&stage)
                        .unwrap()
                        .next()
                        .expect("missing staged file")
                        .unwrap()
                        .path();
                    fs::write(p, "changed staged candidate").unwrap();
                }
                "cache-replacement" => fs::rename(
                    store.join(".sifttext/index.sqlite"),
                    store.join(".sifttext/old-cache.sqlite"),
                )
                .unwrap(),
                _ => (),
            },
            label.starts_with("killed"),
        );
        assert_eq!(
            authored_snapshot(&store),
            authored,
            "{label}: authored drift"
        );
        assert_eq!(
            generated_snapshot(&store),
            generated,
            "{label}: partial publication"
        );
        let (current, _) = incremental_call(&store);
        assert_eq!(
            authored_snapshot(&store),
            authored,
            "{label}: recovery edited authored data"
        );
        assert!(!stage.exists(), "{label}: abandoned stage after recovery");
        if label != "killed-changed" {
            assert_eq!(current, baseline, "{label}: restored logical view drift");
        } else {
            assert_ne!(current["version"], baseline["version"]);
        }
        assert_dependency_equality(&store, &current, &bpe);
    }
}
