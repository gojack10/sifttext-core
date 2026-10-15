use super::{Fixture, commit, git, id, node, sparse::call};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
#[path = "../common/native_resources.rs"]
mod native_resources;

struct Running(Option<Child>);
impl Drop for Running {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
#[test]
fn native_prepared_publication_helper_inclusive_resources() {
    let f = Fixture::with_nodes(4);
    node(&f.a, 2, "Prepared X");
    node(&f.b, 3, "Published Y");
    let x = commit(&f.a);
    let y = commit(&f.b);
    f.prepare(&f.a);
    f.prepare(&f.b);
    f.submit(&f.b);
    let accepted = f.submit(&f.a);
    let view = accepted.view.as_ref().unwrap();
    let donor = view.join(".sifttext/index.sqlite");
    let main_bytes = fs::read(&donor).unwrap();
    let draft = f.a.join(".sifttext/index.sqlite");
    let draft_bytes = fs::read(&draft).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("PREPARED_PUBLICATION_RESOURCE_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("resources"));
    fs::create_dir(&evidence).unwrap();
    let bin_dir = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug"));
    let mut command = Command::new("/usr/bin/time");
    #[cfg(target_os = "macos")]
    command.arg("-l");
    #[cfg(target_os = "linux")]
    command.arg("-v");
    let stdout = evidence.join("stdout.json");
    let stderr = evidence.join("stderr.txt");
    let start = Instant::now();
    let child = command
        .arg(bin_dir.join("git-submit-observe"))
        .stdin(Stdio::piped())
        .stdout(fs::File::create(&stdout).unwrap())
        .stderr(fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let mut guard = Running(Some(child));
    let pid = guard.0.as_ref().unwrap().id();
    let request = f.request(&f.a);
    guard
        .0
        .as_mut()
        .unwrap()
        .stdin
        .take()
        .unwrap()
        .write_all(request.to_string().as_bytes())
        .unwrap();
    let (mut rss_peak, mut fd_peak) = (0, 0);
    let mut samples = vec![];
    let status = loop {
        let (rss, fds, mut raw) = native_resources::sample(pid);
        raw["elapsed_ms"] = json!(start.elapsed().as_secs_f64() * 1000.0);
        samples.push(raw);
        rss_peak = rss_peak.max(rss);
        fd_peak = fd_peak.max(fds);
        if let Some(status) = guard.0.as_mut().unwrap().try_wait().unwrap() {
            break status;
        }
        thread::sleep(Duration::from_millis(20));
    };
    guard.0.take();
    let err = fs::read_to_string(&stderr).unwrap();
    assert!(
        status.success(),
        "{}\n{err}",
        fs::read_to_string(&stdout).unwrap()
    );
    let result: Value = serde_json::from_slice(&fs::read(&stdout).unwrap()).unwrap();
    let native_rss = native_resources::native_rss(&err);
    let du = Command::new("/usr/bin/du")
        .arg("-sk")
        .arg(f._dir.path())
        .output()
        .unwrap();
    assert!(du.status.success());
    let disk_kib: u64 = String::from_utf8(du.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let receipt = json!({"request":request,"result":result,"exit":status.code(),"ms":start.elapsed().as_secs_f64()*1000.0,
        "native_rss_bytes":native_rss,"sampled_helper_inclusive_rss_bytes":rss_peak,"sampled_tree_fds":fd_peak,"samples":samples,"disk_kib":disk_kib,
        "limits":["Four synthetic nodes, debug binary; not private/large-store, aggregate speed or universal capacity proof.","Warm fresh process; OS page cache not dropped. Native time RSS excludes simultaneous helpers.","Process-tree ps/FD samples sleep20ms plus acquisition overhead, can miss peaks and inflate latency. Disk is du allocated KiB for the complete fixture, not the legacy evidence directory.","Record/search reuse is not Markdown parse reuse; compiled graph validation and source checks remain active."]});
    fs::write(
        evidence.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    eprintln!(
        "submission resources: native_rss={native_rss}, helper_rss={rss_peak}, descriptors={fd_peak}, disk_kib={disk_kib}"
    );
    assert!(
        native_rss > 0 && rss_peak > 0 && fd_peak > 0 && disk_kib > 0,
        "missing observation"
    );
    assert_eq!(result["status"], "current");
    assert_eq!(result["candidate"].as_str(), accepted.candidate.as_deref());
    assert_eq!(result["version"].as_str(), accepted.version.as_deref());
    assert_eq!(result["index_reuse"]["accepted_main"], 4);
    assert_eq!(result["index_reuse"]["computed"], 0);
    let measured_view = Path::new(result["view"].as_str().unwrap());
    assert_eq!(
        git(measured_view, &["rev-parse", "HEAD"]),
        result["candidate"].as_str().unwrap()
    );
    assert_eq!(
        git(measured_view, &["rev-parse", "HEAD^{tree}"]),
        result["tree"].as_str().unwrap()
    );
    assert_eq!(
        git(&f.repo, &["rev-parse", "main"]),
        result["candidate"].as_str().unwrap()
    );
    for tip in [&x, &y] {
        git(measured_view, &["merge-base", "--is-ancestor", tip, "HEAD"]);
    }
    // Observe the measured receipt's view through the real CLI, outside sampling.
    let indexed = call(
        "core-observe",
        json!({"op":"index","store":measured_view,"mode":"incremental"}),
    );
    assert_eq!(indexed["status"], "current");
    assert_eq!(indexed["version"], result["version"]);
    let rows = indexed["nodes"].as_array().unwrap();
    assert_eq!(
        rows.iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![id(1), id(2), id(3), id(4)]
    );
    for (n, body) in [(2, "Prepared X"), (3, "Published Y")] {
        assert_eq!(
            rows.iter().find(|r| r["id"] == id(n)).unwrap()["content"],
            body
        );
    }
    let outline = call(
        "core-observe",
        json!({"op":"outline_inspect","store":measured_view}),
    );
    assert_eq!(outline["status"], "current");
    assert_eq!(outline["version"], result["version"]);
    assert_eq!(fs::read(donor).unwrap(), main_bytes);
    assert_eq!(fs::read(draft).unwrap(), draft_bytes);
    assert_eq!(git(&f.a, &["rev-parse", "HEAD"]), x);
    assert_eq!(git(&f.b, &["rev-parse", "HEAD"]), y);
    assert_eq!(git(&f.a, &["status", "--porcelain"]), "");
    assert_eq!(git(&f.b, &["status", "--porcelain"]), "");
}
