//! Native parity for coherent_capture.mjs; unit capture-work checks stay independent.
//! Sampling observes synthetic process families, not continuous peaks or capacity limits.
#[path = "common/native_resources.rs"]
mod resources;
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn id(n: usize) -> String {
    format!("62000000-0000-4000-8000-{n:012}")
}
fn node(store: &Path, n: usize) -> PathBuf {
    store.join("nodes").join(id(n))
}
fn fixture(store: &Path, count: usize, shape: &str) {
    fs::create_dir_all(store.join("trees")).unwrap();
    for n in 1..=count {
        let p = node(store, n);
        fs::create_dir_all(p.join("children")).unwrap();
        fs::write(p.join("children/.keep"), "").unwrap();
        let refs: Vec<_> = if shape == "links" {
            (0..3)
                .map(|i| n.wrapping_add(i) % count + 1)
                .filter(|t| *t != n)
                .map(|t| json!({"target":id(t),"reason":format!("Source {n} target {t} Ω")}))
                .collect()
        } else {
            vec![]
        };
        let fields = json!({"id":id(n),"name":format!("Node {n}"),"micro_summary":format!("Summary {n} Ω"),
            "scope":"Synthetic capture scope","status":"in_progress","see_also":refs});
        let metadata: String = fields
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| format!("{k}: {v}\n"))
            .collect();
        fs::write(
            p.join("node.md"),
            format!("---\n{metadata}---\nExact body\r\n"),
        )
        .unwrap();
        let children: Vec<_> = if shape == "deep" {
            if n < count { vec![n + 1] } else { vec![] }
        } else if n == 1 {
            (2..=count).rev().collect()
        } else {
            vec![]
        };
        fs::write(
            p.join("children.order"),
            children.iter().map(|n| id(*n) + "\n").collect::<String>(),
        )
        .unwrap();
        for c in children {
            symlink(format!("../../{}", id(c)), p.join("children").join(id(c))).unwrap();
        }
    }
    symlink(
        format!("../nodes/{}", id(1)),
        store.join("trees").join(id(1)),
    )
    .unwrap();
}
// Even an assertion failure must reap the directly owned native process.
struct Process(Option<Child>);
impl Drop for Process {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
struct Native {
    bin: PathBuf,
    evidence: PathBuf,
    calls: Vec<Value>,
}
impl Native {
    fn call(&mut self, request: Value, mutate: Option<Box<dyn FnOnce()>>, pass: bool) -> Value {
        let label = format!("{:03}", self.calls.len());
        let stdout = self.evidence.join(format!("{label}.stdout.json"));
        let stderr = self.evidence.join(format!("{label}.stderr.txt"));
        let mut command = Command::new("/usr/bin/time");
        #[cfg(target_os = "macos")]
        command.arg("-l");
        #[cfg(target_os = "linux")]
        command.arg("-v");
        let child = command
            .arg(&self.bin)
            .stdin(Stdio::piped())
            .stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap();
        let mut guard = Process(Some(child));
        let child = guard.0.as_mut().unwrap();
        let pid = child.id();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(request.to_string().as_bytes())
            .unwrap();
        let start = Instant::now();
        if let Some(mutate) = mutate {
            let ready = Path::new(request["pause_before_accept"]["ready"].as_str().unwrap());
            while !ready.exists() {
                assert!(
                    start.elapsed() < Duration::from_secs(30),
                    "barrier unavailable: {request}"
                );
                assert!(
                    child.try_wait().unwrap().is_none(),
                    "native exited before barrier: {request}"
                );
                thread::sleep(Duration::from_millis(5));
            }
            mutate();
            fs::write(
                request["pause_before_accept"]["release"].as_str().unwrap(),
                "release",
            )
            .unwrap();
        }
        let (mut peak, mut fds, mut samples) = (0, 0, vec![]);
        let status = loop {
            let (rss, descriptors, raw) = resources::sample(pid);
            peak = peak.max(rss);
            fds = fds.max(descriptors);
            samples.push(raw);
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            thread::sleep(Duration::from_millis(25));
        };
        guard.0.take();
        let err = fs::read_to_string(stderr).unwrap();
        let bytes = fs::read(stdout).unwrap();
        let result: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        let receipt = json!({"request":request,"exit":status.code(),"result":result,"stderr":err,
            "elapsed_ms":start.elapsed().as_secs_f64()*1000.,"native_rss_bytes":resources::native_rss(&err),
            "sampled_family_peak_bytes":peak,"sampled_fds":fds,"samples":samples});
        fs::write(
            self.evidence.join(format!("{label}.json")),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
        self.calls.push(receipt.clone());
        assert_eq!(status.success(), pass, "{receipt:#}");
        result
    }
    fn index(&mut self, store: &Path, mode: &str) -> Value {
        self.call(json!({"op":"index","store":store,"mode":mode}), None, true)
    }
}
#[test]
fn native_shapes_sparse_equivalence_and_freshness_races() {
    let temp = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("COHERENT_CAPTURE_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("evidence"));
    assert!(evidence.is_absolute());
    fs::create_dir(&evidence).unwrap();
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .map(|p| p.join("core-observe"))
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_core-observe")));
    let mut native = Native {
        bin,
        evidence: evidence.clone(),
        calls: vec![],
    };
    for (shape, count) in [("wide", 64), ("deep", 48), ("links", 48)] {
        let store = evidence.join(shape);
        fixture(&store, count, shape);
        let readable: Vec<_> = (1..=count).map(id).collect();
        let authored: Vec<_> = (1..=count)
            .map(|n| fs::read(node(&store, n).join("node.md")).unwrap())
            .collect();
        let full = native.index(&store, "full");
        assert_eq!(full["status"], "current");
        assert_eq!(full["nodes"].as_array().unwrap().len(), count);
        assert_eq!(
            full["links"].as_array().unwrap().len(),
            if shape == "links" { count * 3 } else { 0 }
        );
        assert_eq!(native.index(&store, "incremental"), full);
        for n in [1, 2, count] {
            let p = native.call(
                json!({"op":"neighborhood","store":store,"id":id(n),"readable_ids":readable}),
                None,
                true,
            );
            assert_eq!(p["neighborhood"]["parent"].is_null(), n == 1);
            assert_eq!(
                p["neighborhood"]["children"].as_array().unwrap().len(),
                if shape == "deep" {
                    usize::from(n < count)
                } else if n == 1 {
                    count - 1
                } else {
                    0
                }
            );
        }
        let restricted = native.call(
            json!({"op":"neighborhood","store":store,"id":id(2),"readable_ids":[id(2)]}),
            None,
            true,
        );
        assert_eq!(
            restricted["neighborhood"],
            json!({"parent":null,"children":[],"outgoing":[],"incoming":[]})
        );
        let search = native.call(
            json!({"op":"search","store":store,"query":"capture","under_node_id":null,
            "limit":10,"offset":0,"readable_ids":readable}),
            None,
            true,
        );
        assert_eq!(search["status"], "current");
        assert_eq!(search["total_matches"], count);
        for n in 1..=count {
            assert_eq!(
                fs::read(node(&store, n).join("node.md")).unwrap(),
                authored[n - 1]
            );
        }
        let file = node(&store, 2).join("node.md");
        fs::write(
            &file,
            String::from_utf8(authored[1].clone())
                .unwrap()
                .replace("Node 2", "Edited 2"),
        )
        .unwrap();
        let edit = native.index(&store, "incremental");
        assert_ne!(edit["version"], full["version"]);
        assert_eq!(edit["nodes"][1]["name"], "Edited 2");
        fs::write(file, &authored[1]).unwrap();
        assert_eq!(native.index(&store, "incremental"), full);
        let overlay = evidence.join(format!("{shape}-overlay"));
        fs::create_dir_all(overlay.join("nodes")).unwrap();
        fs::create_dir(overlay.join("trees")).unwrap();
        symlink(
            format!("../nodes/{}", id(1)),
            overlay.join("trees").join(id(1)),
        )
        .unwrap();
        let sparse = native.call(
            json!({"op":"index","store":overlay,"base_store":store,"mode":"incremental"}),
            None,
            true,
        );
        assert_eq!(sparse, full);
        assert!(!node(&overlay, 2).join("node.md").exists());
        let disk = Command::new("du")
            .args(["-sk"])
            .arg(&store)
            .output()
            .unwrap();
        assert!(disk.status.success());
        fs::write(evidence.join(format!("{shape}-disk.txt")), disk.stdout).unwrap();
    }
    let before = evidence.join("race-before");
    fixture(&before, 4, "wide");
    for (op, kind) in [
        ("index", "body"),
        ("index", "order"),
        ("index", "link"),
        ("index", "root"),
        ("index", "same_bytes"),
        ("neighborhood", "same_bytes"),
        ("status", "same_bytes"),
    ] {
        let store = evidence.join(format!("race-{}", native.calls.len()));
        fixture(&store, 4, "wide");
        let ready = evidence.join(format!("ready-{}", native.calls.len()));
        let release = ready.with_extension("release");
        let pause = json!({"ready":ready,"release":release});
        let request = match op {
            "index" => {
                json!({"op":op,"store":store,"mode":"incremental","pause_before_accept":pause})
            }
            "status" => {
                json!({"op":op,"before":before,"after":store,"explicit":[],"pause_before_accept":pause})
            }
            _ => {
                json!({"op":op,"store":store,"id":id(2),"readable_ids":(1..=4).map(id).collect::<Vec<_>>(),"pause_before_accept":pause})
            }
        };
        let mutate = move || {
            let root = node(&store, 1);
            let file = node(&store, 2).join("node.md");
            match kind {
                "body" => fs::write(&file, fs::read_to_string(&file).unwrap() + "new").unwrap(),
                "same_bytes" => {
                    let replacement = file.with_extension("new");
                    fs::write(&replacement, fs::read(&file).unwrap()).unwrap();
                    fs::rename(replacement, file).unwrap();
                }
                "order" => fs::write(
                    root.join("children.order"),
                    (2..=4).map(|n| id(n) + "\n").collect::<String>(),
                )
                .unwrap(),
                "link" => {
                    let p = root.join("children").join(id(2));
                    fs::remove_file(&p).unwrap();
                    symlink(format!("../../{}", id(2)), p).unwrap();
                }
                "root" => {
                    let p = store.join("trees").join(id(1));
                    fs::remove_file(&p).unwrap();
                    symlink(format!("../nodes/{}", id(1)), p).unwrap();
                }
                _ => unreachable!(),
            }
        };
        let result = native.call(request, Some(Box::new(mutate)), false);
        if op == "index" {
            assert_eq!(result["status"], "stale");
        }
    }
    assert_eq!(native.calls.len(), 37);
    fs::write(evidence.join("coherent-capture-passed.json"),serde_json::to_vec_pretty(&json!({"native_calls":native.calls.len(),
        "limits":["Synthetic wide64/deep48/link48 only; no full Product capacity qualification",
            "25ms sleep plus ps/lsof cost; family samples can miss short helpers and inflate elapsed time",
            "Native time RSS and raw family PID/RSS/FD receipts retained; no continuous peak claim",
            "No atomic unmanaged-multiwriter snapshot or publication guarantee"]})).unwrap()).unwrap();
}
