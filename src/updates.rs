//! Official release acquisition and checked KB merges. Installed packages are immutable.
use crate::{Policy, Result, local, require};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const MANIFEST: &str =
    "https://github.com/gojack10/sifttext-core/releases/latest/download/manifest.json";
const RELEASES: &str = "https://github.com/gojack10/sifttext-core/releases/download/";
const INTERVAL: u64 = 4 * 60 * 60;
const RETRY: u64 = 5 * 60;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub url: String,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Application {
    pub version: String,
    pub source: String,
    pub format: u32,
    pub requires_knowledge: String,
    pub assets: std::collections::BTreeMap<String, Asset>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knowledge {
    pub revision: String,
    pub format: u32,
    pub requires_application: String,
    pub asset: Asset,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub application: Application,
    pub knowledge: Knowledge,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub version: String,
    pub source: String,
    pub knowledge: String,
    pub format: u32,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedApplication {
    version: String,
    source: String,
    format: u32,
    asset_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum UpdatePlan {
    Release(Manifest),
    Rollback(Recovery),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    plan: UpdatePlan,
    application: PathBuf,
    draft: PathBuf,
    base: String,
    upstream: String,
    commit: Option<String>,
    changed: Vec<String>,
    #[serde(default)]
    repair_node: Option<String>,
    // Actual publication receipt, retained even when normal-home finishing fails.
    #[serde(default)]
    accepted_delta: Option<(String, String)>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Recovery {
    application: PathBuf,
    application_sha256: String,
    knowledge: String,
    baseline: Option<String>,
    update_commit: Option<String>,
    #[serde(default)]
    update_base: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    home: PathBuf,
    application: PathBuf,
    application_sha256: String,
    knowledge: String,
    baseline: Option<String>,
    success: CheckTime,
    attempt: Option<CheckTime>,
    pending: Option<Pending>,
    previous: Option<Recovery>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckTime {
    wall: u64,
    uptime: u64,
    boot: String,
}
fn now() -> CheckTime {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // Persistent wall time spans reboots. Within a boot, use the monotonic clock.
    #[cfg(target_os = "linux")]
    let clock_id = libc::CLOCK_BOOTTIME;
    #[cfg(target_os = "macos")]
    let clock_id = libc::CLOCK_MONOTONIC;
    unsafe {
        libc::clock_gettime(clock_id, &mut ts);
    }
    #[cfg(target_os = "linux")]
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap_or_default();
    #[cfg(target_os = "macos")]
    let boot = {
        let mut value = libc::timeval {
            tv_sec: 0,
            tv_usec: 0,
        };
        let mut size = std::mem::size_of_val(&value);
        let ok = unsafe {
            libc::sysctlbyname(
                c"kern.boottime".as_ptr(),
                (&mut value as *mut libc::timeval).cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if ok == 0 {
            format!("{}-{}", value.tv_sec, value.tv_usec)
        } else {
            String::new()
        }
    };
    CheckTime {
        wall: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        uptime: ts.tv_sec.max(0) as u64,
        boot,
    }
}
fn elapsed(before: &CheckTime, time: &CheckTime) -> u64 {
    if !time.boot.is_empty() && time.boot == before.boot {
        time.uptime.checked_sub(before.uptime).unwrap_or(u64::MAX)
    } else {
        time.wall.checked_sub(before.wall).unwrap_or(u64::MAX)
    }
}
fn due(state: &State, time: &CheckTime) -> bool {
    elapsed(&state.success, time) >= INTERVAL
        && state
            .attempt
            .as_ref()
            .is_none_or(|attempt| elapsed(attempt, time) >= RETRY)
}
fn persist<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().ok_or("missing state parent")?)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    fs::File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}
fn load<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&crate::read_regular(path)?)?)
}
fn lock(root: &Path, shared: bool) -> Result<fs::File> {
    crate::real_dir(root)?;
    let f = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(root.join("update.lock"))?;
    use std::os::unix::fs::MetadataExt;
    require(
        f.metadata()?.is_file() && f.metadata()?.nlink() == 1,
        "unsafe update lock",
    )?;
    if shared {
        f.lock_shared()?;
    } else {
        f.try_lock().map_err(|e| format!("update busy: {e}"))?;
    }
    Ok(f)
}
/// Hold for the entire operation, including native publication. A raced launcher retries.
pub fn operation_lock() -> Result<Option<fs::File>> {
    let Some(root) = std::env::var_os("SIFTTEXT_INSTALL_ROOT") else {
        return Ok(None);
    };
    let root = PathBuf::from(root);
    let mut file = lock(&root, true)?;
    if recovery_needed(&root)? {
        drop(file);
        update(&root, "--recover")?;
        file = lock(&root, true)?;
    }
    require(
        fs::canonicalize(root.join("current"))? == local::runtime()?,
        "release changed before operation; retry the command",
    )?;
    Ok(Some(file))
}
fn recovery_needed(root: &Path) -> Result<bool> {
    if root.join("activation.json").exists() {
        let marker: Value = load(&root.join("activation.json"))?;
        if let Some(publication) = marker.get("publication") {
            let request: crate::checked_publication::Request =
                serde_json::from_value(publication.clone())?;
            return Ok(request.commit.as_ref().is_some_and(|commit| {
                git(
                    &request.repo,
                    &["merge-base", "--is-ancestor", commit, "refs/heads/main"],
                )
                .is_ok()
            }));
        }
        return Ok(true);
    }
    let state: State = load(&root.join("state.json"))?;
    Ok(state
        .pending
        .as_ref()
        .and_then(|p| p.commit.as_ref())
        .is_some_and(|commit| {
            git(
                &state.home,
                &["merge-base", "--is-ancestor", commit, "refs/heads/main"],
            )
            .is_ok()
        }))
}
fn activate(root: &Path, path: &Path) -> Result<()> {
    require(
        path.starts_with(root.join("releases")),
        "candidate outside release directory",
    )?;
    let temp = root.join("current.next");
    if temp.symlink_metadata().is_ok() {
        require(
            fs::read_link(&temp)? == path,
            "different interrupted activation found; retain current.next for inspection",
        )?;
    } else {
        symlink(path, &temp)?;
    }
    fs::rename(temp, root.join("current"))?;
    fs::File::open(root)?.sync_all()?;
    Ok(())
}
fn platform() -> Result<String> {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        "linux" => "linux",
        _ => return Err("unsupported OS".into()),
    };
    let cpu = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        _ => return Err("unsupported architecture".into()),
    };
    Ok(format!("{os}-{cpu}"))
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.len() < 100
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn official(asset: &Asset) -> Result<()> {
    let tail = asset
        .url
        .strip_prefix(RELEASES)
        .ok_or("unofficial release URL")?;
    let (tag, file) = tail.split_once('/').ok_or("missing release artifact")?;
    require(
        identifier(tag) && identifier(file) && digest(&asset.sha256),
        "invalid official artifact identity",
    )
}
fn validate_manifest(manifest: &Manifest) -> Result<()> {
    require(
        manifest.schema == 1 && manifest.application.format == 1 && manifest.knowledge.format == 1,
        "unsupported release compatibility; keep the active release",
    )?;
    require(
        identifier(&manifest.application.version) && digest(&manifest.knowledge.revision),
        "invalid release version",
    )?;
    require(
        manifest.application.requires_knowledge == manifest.knowledge.revision
            && manifest.knowledge.requires_application == manifest.application.version,
        "release components are not a compatible set",
    )?;
    official(&manifest.knowledge.asset)?;
    for asset in manifest.application.assets.values() {
        official(asset)?;
    }
    Ok(())
}
fn download(url: &str, path: &Path, limit: u64) -> Result<()> {
    let output = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "3",
            "--max-time",
            "60",
            "--max-filesize",
            &limit.to_string(),
            "--output",
        ])
        .arg(path)
        .arg(url)
        .output()?;
    require(
        output.status.success(),
        "Could not download the official release. Check your internet connection, then retry `sifttext update`. Your current installation and local work remain available.",
    )?;
    require(
        fs::metadata(path)?.len() <= limit,
        "oversized release download",
    )
}
fn fetch_manifest(root: &Path) -> Result<Manifest> {
    let file = tempfile::NamedTempFile::new_in(root)?;
    download(MANIFEST, file.path(), 1_048_576)?;
    let manifest: Manifest = load(file.path())?;
    validate_manifest(&manifest)?;
    persist(&root.join("last_manifest.json"), &manifest)?;
    Ok(manifest)
}
/// Stage one official package into `destination`.
///
/// The asset URL must sit under the official release path and the archive must match the
/// digest published beside it. That proves the bytes are the ones the publisher's GitHub
/// release serves over HTTPS. It does not prove which build produced them; that would need
/// a signed-provenance verifier with its own established trust root, which this product
/// does not currently require or ship.
fn unpack(asset: &Asset, destination: &Path) -> Result<()> {
    official(asset)?;
    let stage = tempfile::tempdir_in(destination.parent().ok_or("missing stage parent")?)?;
    let archive = stage.path().join("download.tar.gz");
    download(&asset.url, &archive, 268_435_456)?;
    require(
        crate::sha256(&fs::read(&archive)?) == asset.sha256,
        "release checksum mismatch",
    )?;
    let listing = Command::new("tar").arg("-tzf").arg(&archive).output()?;
    require(listing.status.success(), "invalid release archive")?;
    let listing = String::from_utf8(listing.stdout)?;
    require(
        listing.lines().all(|p| {
            (p == "package/" || p.starts_with("package/"))
                && !p.split('/').any(|part| part == ".." || part == ".")
        }),
        "unsafe archive path",
    )?;
    fs::create_dir(destination)?;
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("--strip-components=1")
        .arg("-C")
        .arg(destination)
        .status()?;
    if !status.success() {
        fs::remove_dir_all(destination)?;
        return Err("release extraction failed".into());
    }
    Ok(())
}
fn package(path: &Path) -> Result<Package> {
    load(&path.join("release.json"))
}
fn verify_cached_application(path: &Path, manifest: &Manifest, asset: &Asset) -> Result<()> {
    let info = package(path)?;
    let receipt: CachedApplication = load(&path.join(".sifttext-cache.json"))?;
    require(
        info.version == manifest.application.version
            && info.source == manifest.application.source
            && info.format == manifest.application.format
            && receipt.version == info.version
            && receipt.source == info.source
            && receipt.format == info.format
            && receipt.asset_sha256 == asset.sha256,
        "cached application identity differs from release; retain it and stage a fresh candidate",
    )
}

/// Build or reuse a checked source package, leaving the source and working release intact.
fn build_candidate(root: &Path, source: &Path) -> Result<PathBuf> {
    let info = package(source)?;
    let inputs = crate::patch_build::fingerprint(source)?;
    let releases = root.join("releases");
    let candidate = releases.join(format!("{}-patched-{inputs}", info.version));
    match fs::symlink_metadata(&candidate) {
        Ok(metadata) => {
            require(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "unsafe cached patched candidate",
            )?;
            let cached = package(&candidate)?;
            let receipt: crate::patch_build::BuildReceipt =
                load(&candidate.join(".sifttext-build.json"))?;
            require(
                cached.version == info.version
                    && cached.source == info.source
                    && cached.format == info.format,
                "cached patched package identity differs from source",
            )?;
            crate::patch_build::verify_receipt(
                &candidate,
                &receipt,
                &cached.version,
                &cached.source,
                cached.format,
                &inputs,
            )?;
            require(
                crate::patch_build::fingerprint(&candidate)? == inputs,
                "cached patched source inputs changed; preserve it and rebuild",
            )?;
            checked_runtime(&candidate)?;
            return Ok(candidate);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let cache = root.join("build-cache");
    match fs::symlink_metadata(&cache) {
        Ok(metadata) => require(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "unsafe source-build cache",
        )?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&cache)?,
        Err(error) => return Err(error.into()),
    }
    let target = cache.join("target");
    let built = Command::new("sh")
        .arg("scripts/build.sh")
        .arg("--release")
        .current_dir(source)
        .env("CARGO_TARGET_DIR", &target)
        .env("CARGO_NET_OFFLINE", "true")
        .output()?;
    require(
        built.status.success(),
        &format!(
            "patched source build failed; retain the working release: {}",
            String::from_utf8_lossy(&built.stderr)
        ),
    )?;
    fs::copy(target.join("release/sifttext"), source.join("sifttext"))?;
    let receipt = crate::patch_build::write_receipt(
        source,
        &info.version,
        &info.source,
        info.format,
        &inputs,
    )?;
    persist(&source.join(".sifttext-build.json"), &receipt)?;
    checked_runtime(source)?;
    fs::rename(source, &candidate)?;
    Ok(candidate)
}

/// Build a patched copy of an official source package; leave the verified official cache intact.
fn patched_candidate(
    root: &Path,
    home: &Path,
    official: &Path,
    manifest: &Manifest,
) -> Result<Option<PathBuf>> {
    let temp = tempfile::tempdir_in(root.join("releases"))?;
    let source = temp.path().join("source");
    copy_tree(official, &source)?;
    let patches = crate::patch_build::apply_recorded(&source, home)?;
    if patches.is_empty() {
        return Ok(None);
    }
    let info = package(&source)?;
    require(
        info.version == manifest.application.version
            && info.source == manifest.application.source
            && info.format == manifest.application.format,
        "patched source package identity differs from release",
    )?;
    build_candidate(root, &source).map(Some)
}

fn rebuild_active(root: &Path, state: &mut State) -> Result<Value> {
    require(
        state.pending.is_none(),
        "finish the pending update before rebuilding",
    )?;
    let active = root.join(&state.application);
    let temp = tempfile::tempdir_in(root.join("releases"))?;
    let source = temp.path().join("source");
    copy_tree(&active, &source)?;
    crate::patch_build::apply_recorded(&source, &state.home)?;
    let candidate = build_candidate(root, &source)?;
    let relative = candidate.strip_prefix(root)?.to_owned();
    let info = package(&candidate)?;
    let receipt: crate::patch_build::BuildReceipt = load(&candidate.join(".sifttext-build.json"))?;
    let pinned = format!("local-{}", &receipt.inputs[..40]);
    let previous = Recovery {
        application: state.application.clone(),
        application_sha256: state.application_sha256.clone(),
        knowledge: state.knowledge.clone(),
        baseline: state.baseline.clone(),
        update_commit: None,
        update_base: None,
    };
    persist(
        &root.join("activation.json"),
        &json!({
            "application":relative,
            "application_sha256":receipt.executable.clone(),
            "knowledge":state.knowledge,
            "baseline":state.baseline,
            "previous":Some(previous.clone()),
            "pinned":pinned,
        }),
    )?;
    set_update_config(&state.home, "pinned", &pinned)?;
    activate(root, &candidate)?;
    state.application = relative;
    state.application_sha256 = receipt.executable;
    state.previous = Some(previous);
    persist(&root.join("state.json"), state)?;
    fs::remove_file(root.join("activation.json"))?;
    Ok(json!({
        "status":"rebuilt",
        "application":info.version,
        "source":info.source,
        "pinned":pinned,
        "path":candidate,
    }))
}

fn checked_runtime(path: &Path) -> Result<Policy> {
    let output = Command::new(path.join("sifttext"))
        .arg("runtime-check")
        .env_remove("SIFTTEXT_INSTALL_ROOT")
        .output()?;
    require(
        output.status.success(),
        &format!(
            "candidate runtime check failed; retain previous release: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    Policy::checked(path)
}
fn git(home: &Path, args: &[&str]) -> Result<String> {
    local::reader_git(home, args)
}
fn text(path: &Path) -> Result<&str> {
    path.to_str().ok_or_else(|| "non-UTF-8 path".into())
}

fn repair_paths(policy: &Policy, home: &Path) -> Result<Value> {
    let tree = local::ensure_maintenance_tree(policy, home)?;
    Ok(json!({
        "guide": home.join("nodes/8b5361e1-20d7-42a9-9dca-26ba3bac9f25/node.md"),
        "maintenance_tree": tree["path"],
        "maintenance_tree_id": tree["id"],
    }))
}

/// Called only by the verified bootstrap, before the final install-directory rename.
pub fn initialize(policy: &Policy, root: &Path, home: &Path) -> Result<Value> {
    let actual_root = fs::canonicalize(root)?;
    let actual_home = if home.exists() {
        fs::canonicalize(home)?
    } else {
        fs::canonicalize(home.parent().ok_or("missing home parent")?)?
            .join(home.file_name().ok_or("missing home name")?)
    };
    require(
        !actual_root.starts_with(&actual_home) && !actual_home.starts_with(&actual_root),
        "application and knowledge directories must be separate",
    )?;
    let _lock = lock(root, false)?;
    let state_path = root.join("state.json");
    if state_path.exists() {
        let state: State = load(&state_path)?;
        require(
            state.home == home,
            "installation already belongs to another knowledge home",
        )?;
        return Ok(json!({"home":home,"created":false,"application":state.application}));
    }
    let app = fs::canonicalize(root.join("current"))?;
    let info = package(&app)?;
    require(
        info.format == 1 && identifier(&info.version) && digest(&info.knowledge),
        "invalid packaged release metadata",
    )?;
    let initialized = crate::native_kb::init(policy, home)?;
    let baseline = if initialized["created"] == true {
        Some(git(home, &["rev-parse", "HEAD"])?)
    } else {
        None
    };
    if let Some(base) = &baseline {
        git(home, &["update-ref", "refs/sifttext/upstream", base])?;
    }
    // Store relative package locations so the bootstrap's final rename is harmless.
    let application_sha256 = String::from_utf8(crate::read_regular(&root.join("package.sha256"))?)?
        .trim()
        .to_owned();
    require(
        digest(&application_sha256),
        "missing verified package checksum",
    )?;
    let state = State {
        home: home.to_owned(),
        application: PathBuf::from("releases").join(&info.version),
        application_sha256,
        knowledge: if baseline.is_some() {
            info.knowledge
        } else {
            "unknown".into()
        },
        baseline,
        success: now(),
        attempt: None,
        pending: None,
        previous: None,
    };
    persist(&state_path, &state)?;
    Ok(initialized)
}

// Both the internal recovery API and the customer receipt spelling are supported.
// Acceptance alone is insufficient: activation also requires ready_home below.
fn accepted_submission(receipt: &Value, home: &Path) -> Result<String> {
    require(
        matches!(receipt["status"].as_str(), Some("current" | "saved" | "recovered"))
            && receipt["publication"] != "accepted_incomplete"
            && receipt["home"].as_str() == home.to_str(),
        &format!("knowledge publication remains pending; retain the original draft/base and retry: {receipt}"),
    )?;
    receipt["candidate"]
        .as_str()
        .or_else(|| receipt["accepted"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| "missing accepted knowledge commit; retain update journal".into())
}

fn ready_home(policy: &Policy, home: &Path, accepted: Option<&str>) -> Result<()> {
    // This is clean, ff-only reader recovery, not another graph acceptance gate.
    let ready = local::advance(policy, home)?;
    require(
        matches!(ready["status"].as_str(), Some("current" | "advanced"))
            && ready["accepted"].as_str() == Some(git(home, &["rev-parse", "HEAD"])?.as_str())
            && ready["accepted"].as_str() == Some(git(home, &["rev-parse", "refs/heads/main"])?.as_str()),
        "knowledge home is not ready; retain the update journal and retry",
    )?;
    if let Some(accepted) = accepted {
        git(home, &["merge-base", "--is-ancestor", accepted, "refs/heads/main"])?;
    }
    Ok(())
}

fn finish(
    root: &Path,
    state: &mut State,
    manifest: &Manifest,
    app: &Path,
    commit: Option<String>,
    update_base: Option<String>,
    baseline: Option<String>,
) -> Result<Value> {
    let policy = checked_runtime(&root.join(app))?;
    ready_home(&policy, &state.home, commit.as_deref())?;
    let previous = Recovery {
        application: state.application.clone(),
        application_sha256: state.application_sha256.clone(),
        knowledge: state.knowledge.clone(),
        baseline: state.baseline.clone(),
        update_commit: commit.clone(),
        update_base,
    };
    let application_sha256 = manifest
        .application
        .assets
        .get(&platform()?)
        .ok_or("missing platform checksum")?
        .sha256
        .clone();
    // The journal survives a crash between knowledge publication and launcher activation.
    persist(
        &root.join("activation.json"),
        &json!({"application":app,"application_sha256":application_sha256,"knowledge":manifest.knowledge.revision,"baseline":baseline,"previous":previous}),
    )?;
    activate(root, &root.join(app))?;
    if let Some(base) = &baseline {
        git(&state.home, &["update-ref", "refs/sifttext/upstream", base])?;
    }
    let changed = state
        .pending
        .as_ref()
        .map(|p| p.changed.clone())
        .unwrap_or_default();
    state.application = app.to_owned();
    state.application_sha256 = application_sha256;
    state.knowledge = manifest.knowledge.revision.clone();
    state.baseline = baseline;
    state.previous = Some(previous);
    state.pending = None;
    persist(&root.join("state.json"), state)?;
    fs::remove_file(root.join("activation.json"))?;
    Ok(
        json!({"status":"updated","application":manifest.application.version,"knowledge":state.knowledge,"refresh_instruction_paths":changed}),
    )
}
fn resume(
    root: &Path,
    state: &mut State,
    preflight: &mut impl FnMut(&Value) -> Result<()>,
) -> Result<Value> {
    let mut pending = state.pending.clone().ok_or("no update is pending")?;
    if let UpdatePlan::Rollback(previous) = &pending.plan {
        return resume_rollback(root, state, previous.clone());
    }
    let policy = checked_runtime(&root.join(&pending.application))?;
    if pending.commit.is_none() {
        require(
            git(&pending.draft, &["diff", "--name-only", "--diff-filter=U"])?.is_empty(),
            "resolve the reported KB conflicts in the retained worktree",
        )?;
        let commit = git(&pending.draft, &["rev-parse", "HEAD"])?;
        git(
            &pending.draft,
            &["merge-base", "--is-ancestor", &pending.upstream, &commit],
        )?;
        require(
            commit != pending.base,
            "commit the resolved KB merge before resuming",
        )?;
        pending.commit = Some(commit);
        state.pending = Some(pending.clone());
        persist(&root.join("state.json"), state)?;
    }
    let projected_receipt = json!({
        "status":"updated",
        "application": match &pending.plan {
            UpdatePlan::Release(manifest) => &manifest.application.version,
            _ => unreachable!(),
        },
        "knowledge": match &pending.plan {
            UpdatePlan::Release(manifest) => &manifest.knowledge.revision,
            _ => unreachable!(),
        },
        "refresh_instruction_paths": pending.changed,
    });
    preflight(&projected_receipt)?;
    let receipt = local::submit(
        &policy,
        &crate::checked_publication::Request {
            repo: state.home.clone(),
            accepted_ref: "refs/heads/main".into(),
            draft: Some(pending.draft.clone()),
            commit: pending.commit.clone(),
            base: Some(pending.base.clone()),
            explicit: BTreeSet::new(),
            pause_before_publish: None,
            pause_after_publish: None,
        },
    )?;
    if receipt["published"] == true {
        let before = receipt["accepted_before"].as_str().ok_or("missing publication base; retain update")?;
        let accepted = receipt["candidate"].as_str().ok_or("missing publication candidate; retain update")?;
        require(pending.accepted_delta.is_none(),
            "update published more than once; preserve receipts and resolve its exact delta before activation")?;
        pending.accepted_delta = Some((before.to_owned(), accepted.to_owned()));
        state.pending = Some(pending.clone());
        persist(&root.join("state.json"), state)?;
    }
    let accepted = accepted_submission(&receipt, &state.home)?;
    let (before, update_commit) = pending.accepted_delta.as_ref().ok_or(
        "missing original successful publication receipt; preserve accepted knowledge and update journal; do not infer an update base from history",
    )?;
    git(&state.home, &["merge-base", "--is-ancestor", update_commit, &accepted])?;
    finish(
        root,
        state,
        match &pending.plan {
            UpdatePlan::Release(manifest) => manifest,
            _ => unreachable!(),
        },
        &pending.application,
        Some(update_commit.clone()),
        Some(before.clone()),
        Some(pending.upstream.clone()),
    )
}
/// Agent-facing next step in a release notice. A human decides whether to update.
const UPDATE_NOTICE_ASK: &str =
    "Ask the user to update SiftText for latest speed and stability fixes.";
/// With a standing grant no confirmation is needed, so the next step is urgent.
const UPDATE_NOTICE_STANDING: &str =
    "Update SiftText immediately for latest speed and stability fixes.";
const STANDING_GRANT_COMMAND: &str = "sifttext update --grant standing";

fn opportunistic_notice(current: &Package, state: &State, manifest: &Manifest) -> Result<Value> {
    let config = update_config(&state.home)?;
    let app_changed = current.version != manifest.application.version;
    if !app_changed {
        // A pinned build is expected to differ from the official release of the same version; that
        // is what the pin records. Its application stays and its knowledge still updates, and the
        // runtime of the active build is verified before anything is activated.
        let official_match = current.source == manifest.application.source
            && manifest
                .application
                .assets
                .get(&platform()?)
                .is_some_and(|asset| asset.sha256 == state.application_sha256);
        require(
            official_match || config["pinned"].is_string(),
            "official application identity changed without a new version; retain active bytes",
        )?;
    }
    if !app_changed && state.knowledge == manifest.knowledge.revision {
        return Ok(json!({"status":"current","application":current.version}));
    }
    let permission = config["permission"].as_str().unwrap_or("ask");
    let standing = permission == "standing";
    let mut notice = json!({
        "status":"update_available",
        "application":manifest.application.version,
        "next_action":if standing { UPDATE_NOTICE_STANDING } else { UPDATE_NOTICE_ASK },
        "guide":state.home.join("nodes/8b5361e1-20d7-42a9-9dca-26ba3bac9f25/node.md"),
        "command":"sifttext update",
    });
    if standing {
        // An automatic grant needs no confirmation, so the notice omits it and
        // does not suggest the grant the user already recorded.
    } else {
        notice["permission"] = json!(permission);
        notice["grant_command"] = json!(STANDING_GRANT_COMMAND);
    }
    if let Some(pinned) = config["pinned"].as_str() {
        notice["pinned"] = json!(pinned);
        if app_changed {
            notice["command"] = json!("sifttext update --unpin");
        }
    }
    Ok(notice)
}
fn apply(
    root: &Path,
    state: &mut State,
    manifest: &Manifest,
    preflight: &mut impl FnMut(&Value) -> Result<()>,
) -> Result<Value> {
    validate_manifest(manifest)?;
    let current = package(&root.join(&state.application))?;
    let config = update_config(&state.home)?;
    let app_changed = current.version != manifest.application.version;
    if !app_changed {
        let official_match = current.source == manifest.application.source
            && manifest
                .application
                .assets
                .get(&platform()?)
                .is_some_and(|asset| asset.sha256 == state.application_sha256);
        require(
            official_match || config["pinned"].is_string(),
            "official application identity changed without a new version; retain active bytes",
        )?;
    }
    let kb_changed = state.knowledge != manifest.knowledge.revision;
    if !app_changed && !kb_changed {
        return Ok(json!({"status":"current","application":current.version}));
    }
    let app = if app_changed {
        // A customer who pinned their own build keeps it: the candidate is reported, never staged
        // or activated, so a local fix survives an update without anyone noticing and rolling back.
        // A knowledge-only release still applies, because it reuses the pinned executable.
        if let Some(pinned) = config["pinned"].as_str() {
            return Ok(json!({
                "status":"update_pinned",
                "pinned":pinned,
                "application":manifest.application.version,
                "knowledge":manifest.knowledge.revision,
                "unpin":"sifttext update --unpin",
            }));
        }
        let asset = manifest
            .application
            .assets
            .get(&platform()?)
            .ok_or("no official package for this platform")?;
        let path = PathBuf::from("releases").join(format!(
            "{}-{}",
            manifest.application.version, &asset.sha256
        ));
        let destination = root.join(&path);
        if destination.exists() {
            verify_cached_application(&destination, manifest, asset)?;
        } else {
            unpack(asset, &destination)?;
            let info = package(&destination)?;
            require(
                info.version == manifest.application.version
                    && info.source == manifest.application.source
                    && info.format == manifest.application.format,
                "staged package identity differs from manifest",
            )?;
            persist(
                &destination.join(".sifttext-cache.json"),
                &CachedApplication {
                    version: info.version,
                    source: info.source,
                    format: info.format,
                    asset_sha256: asset.sha256.clone(),
                },
            )?;
        }
        match patched_candidate(root, &state.home, &destination, manifest) {
            Ok(Some(candidate)) => candidate.strip_prefix(root)?.to_owned(),
            Ok(None) => path,
            Err(error) => {
                return Ok(json!({
                    "status":"needs_agent",
                    "reason":"patched_build_failed",
                    "error":error.to_string(),
                    "application":manifest.application.version,
                    "active_application":state.application,
                    "guide":state.home.join("nodes/8b5361e1-20d7-42a9-9dca-26ba3bac9f25/node.md"),
                }));
            }
        }
    } else {
        state.application.clone()
    };
    let policy = checked_runtime(&root.join(&app))?;
    if !kb_changed {
        return finish(root, state, manifest, &app, None, None, state.baseline.clone());
    }
    require(
        state.baseline.is_some(),
        "existing home has no verified upstream baseline; keep current knowledge and ask the agent to establish its source revision",
    )?;
    let kb = root.join(format!("knowledge-{}", manifest.knowledge.revision));
    require(
        !kb.exists(),
        "unactivated knowledge package already exists; inspect it before retrying",
    )?;
    unpack(&manifest.knowledge.asset, &kb)?;
    merge_knowledge(root, state, manifest, app, &policy, &kb, preflight)
}
fn merge_knowledge(
    root: &Path,
    state: &mut State,
    manifest: &Manifest,
    app: PathBuf,
    policy: &Policy,
    kb: &Path,
    preflight: &mut impl FnMut(&Value) -> Result<()>,
) -> Result<Value> {
    let baseline = state
        .baseline
        .clone()
        .ok_or("no verified upstream baseline")?;
    local::check(policy, kb, None, BTreeSet::new())?;
    // Form an upstream branch from the last incorporated snapshot, not the customer's HEAD.
    let upstream_dir = root.join("upstream-worktree");
    git(
        &state.home,
        &[
            "worktree",
            "add",
            "--detach",
            text(&upstream_dir)?,
            &baseline,
        ],
    )?;
    for name in ["nodes", "trees"] {
        let dest = upstream_dir.join(name);
        if dest.exists() {
            fs::remove_dir_all(&dest)?;
        }
        copy_tree(&kb.join(name), &dest)?;
    }
    git(&upstream_dir, &["add", "--", "nodes", "trees"])?;
    git(
        &upstream_dir,
        &[
            "commit",
            "--allow-empty",
            "-m",
            &format!("knowledge: import official {}", manifest.knowledge.revision),
        ],
    )?;
    let upstream = git(&upstream_dir, &["rev-parse", "HEAD"])?;
    git(
        &state.home,
        &["update-ref", "refs/sifttext/upstream-candidate", &upstream],
    )?;
    git(&state.home, &["worktree", "remove", text(&upstream_dir)?])?;
    let allocation = local::worktree(&policy, &state.home)?;
    let draft = PathBuf::from(allocation["worktree"].as_str().ok_or("missing draft")?);
    let base = allocation["base"]
        .as_str()
        .ok_or("missing draft base")?
        .to_owned();
    let changed = git(
        &state.home,
        &[
            "diff",
            "--name-only",
            &baseline,
            &upstream,
            "--",
            "nodes",
            "trees",
        ],
    )?
    .lines()
    .map(str::to_owned)
    .collect();
    state.pending = Some(Pending {
        plan: UpdatePlan::Release(manifest.clone()),
        application: app,
        draft: draft.clone(),
        base,
        upstream: upstream.clone(),
        commit: None,
        changed,
        repair_node: None,
        accepted_delta: None,
    });
    persist(&root.join("state.json"), state)?;
    if let Err(error) = git(&draft, &["merge", "--no-ff", "--no-commit", &upstream]) {
        let paths = repair_paths(policy, &state.home)?;
        return Ok(
            json!({"status":"needs_agent","worktree":draft,"conflict_paths":git(&draft, &["diff", "--name-only", "--diff-filter=U"])?,"error":error.to_string(),"resume":"sifttext update --resume","repair":paths}),
        );
    }
    // Record the authored merge; submit normalizes the exact combined Git candidate.
    git(&draft, &["add", "--", "nodes", "trees"])?;
    git(
        &draft,
        &[
            "commit",
            "-m",
            &format!("knowledge: merge official {}", manifest.knowledge.revision),
        ],
    )?;
    resume(root, state, preflight)
}
fn copy_tree(source: &Path, dest: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(source)?;
    if meta.file_type().is_symlink() {
        symlink(fs::read_link(source)?, dest)?;
    } else if meta.is_dir() {
        fs::create_dir(dest)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(&entry.path(), &dest.join(entry.file_name()))?;
        }
    } else {
        require(meta.is_file(), "unexpected package entry")?;
        fs::copy(source, dest)?;
    }
    Ok(())
}
const CONFIG_FILE: &str = ".sifttext/config.json";

/// User-owned update intent, kept in the knowledge home's own configuration file and never in
/// `state.json`, which holds only machine observations. The knowledge home's Git index ignores
/// `.sifttext/` directory, so an official knowledge package cannot add or rewrite this file.
/// Defaults preserve shipped behavior: release checks still notify, and no standing grant is
/// recorded until the user records one.
fn update_config(home: &Path) -> Result<Value> {
    let path = home.join(CONFIG_FILE);
    let file: Value = match fs::symlink_metadata(&path) {
        Ok(_) => serde_json::from_slice(&crate::read_regular(&path)?)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(json!({"mode":"notify","permission":"ask","source":"default","path":path}));
        }
        Err(error) => return Err(error.into()),
    };
    let empty = json!({});
    let updates = file.get("updates").unwrap_or(&empty);
    let updates = updates.as_object().ok_or("updates must be an object")?;
    for key in updates.keys() {
        require(
            key == "mode" || key == "permission" || key == "pinned",
            "updates accepts only mode, permission and pinned",
        )?;
    }
    let pinned = match updates.get("pinned") {
        None | Some(Value::Null) => Value::Null,
        Some(Value::String(pinned)) => {
            require(
                identifier(pinned),
                "updates.pinned must be a release identity",
            )?;
            json!(pinned)
        }
        Some(_) => return Err("updates.pinned must be a release identity or null".into()),
    };
    let choice = |name: &str, allowed: &[&str], fallback: &str| -> Result<String> {
        let Some(value) = updates.get(name) else {
            return Ok(fallback.to_owned());
        };
        let Some(text) = value.as_str() else {
            return Err(format!("updates.{name} must be a string").into());
        };
        let message = format!("updates.{name} must be one of {}", allowed.join(" or "));
        require(allowed.contains(&text), &message)?;
        Ok(text.to_owned())
    };
    Ok(json!({
        "mode": choice("mode", &["notify", "disabled"], "notify")?,
        "permission": choice("permission", &["standing", "ask"], "ask")?,
        "pinned": pinned,
        "source": "file",
        "path": path,
    }))
}

/// Record one user-owned update choice without touching any other setting in the shared local
/// configuration. An agent storing a consent answer must not be able to drop session retention
/// or another sibling key by rewriting the file, and an unsupported value is refused before any
/// write.
fn set_update_config(home: &Path, name: &str, value: &str) -> Result<Value> {
    let allowed: &[&str] = if name == "mode" {
        &["notify", "disabled"]
    } else if name == "pinned" {
        &["", "current"]
    } else {
        &["standing", "ask"]
    };
    let message = format!("updates.{name} must be one of {}", allowed.join(" or "));
    if name != "pinned" {
        require(allowed.contains(&value), &message)?;
    }
    let pinned_identity = name == "pinned" && value != "" && value != "current";
    if name == "pinned" && !pinned_identity {
        require(value == "current", &message)?;
    }
    if pinned_identity {
        require(
            identifier(value),
            "updates.pinned must be a release identity",
        )?;
    }
    let path = home.join(CONFIG_FILE);
    let mut file: Value = match fs::symlink_metadata(&path) {
        Ok(_) => serde_json::from_slice(&crate::read_regular(&path)?)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(error) => return Err(error.into()),
    };
    let root = file
        .as_object_mut()
        .ok_or("local configuration must be an object")?;
    let updates = root
        .entry("updates".to_owned())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("updates must be an object")?;
    if name == "pinned" && value == "current" {
        updates.remove(name);
    } else {
        updates.insert(name.to_owned(), value.into());
    }
    fs::create_dir_all(path.parent().ok_or("missing configuration parent")?)?;
    persist(&path, &file)?;
    update_config(home)
}

/// Inspect only the last recorded update journal. Never take/create the update lock, recover,
/// fetch releases, inspect the knowledge home, or infer publication/activation readiness.
fn local_status(root: &Path) -> Result<Value> {
    use std::io::Read;
    use std::os::unix::fs::MetadataExt;

    crate::real_dir(root)?;
    require(root.is_absolute() && fs::canonicalize(root)? == root, "use the canonical installation path")?;
    // Open the one allowed state file without following a raced link or blocking on a FIFO.
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(root.join("state.json"))?;
    let meta = file.metadata()?;
    require(meta.is_file() && meta.nlink() == 1 && meta.len() <= 1_048_576, "unsafe update state file")?;
    let mut bytes = Vec::new();
    file.take(1_048_577).read_to_end(&mut bytes)?;
    require(bytes.len() <= 1_048_576, "oversized update state file")?;
    let state: State = serde_json::from_slice(&bytes)
        .map_err(|_| "invalid update state.json; local inspection unavailable")?;
    // State is atomically replaced by the updater, but these separate observations cannot be
    // a transaction with a concurrent writer. Marker presence is not validated recovery input.
    let activation_marker_present = match fs::symlink_metadata(root.join("activation.json")) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    let mut status = json!({
        "status":"recorded_only",
        "inspection":"local_read_only",
        "consistency":"not_atomic_snapshot",
        "update_check":"not_performed",
        "publication_state":"not_inspected",
        "note":"Recorded update journal only; launcher, knowledge HEAD, settings and lock ownership are not checked. This is not permission to publish, resume or remove locks.",
        "home":state.home,
        "application":state.application,
        "knowledge":state.knowledge,
        "activation_marker_present":activation_marker_present,
    });
    if let Some(previous) = state.previous {
        status["rollback_application"] = json!(previous.application);
        status["rollback_knowledge"] = json!(previous.knowledge);
    }
    if let Some(pending) = state.pending {
        status["worktree"] = json!(pending.draft);
        status["pending_operation"] = json!(match pending.plan {
            UpdatePlan::Release(_) => "update",
            UpdatePlan::Rollback(_) => "rollback",
        });
        status["pending_commit"] = json!(pending.commit);
        status["pending_repair_node"] = json!(pending.repair_node);
    }
    Ok(status)
}

pub fn update(root: &Path, mode: &str) -> Result<Value> {
    update_with_receipt_preflight(root, mode, |_| Ok(()))
}

pub fn update_with_receipt_preflight(
    root: &Path,
    mode: &str,
    mut preflight: impl FnMut(&Value) -> Result<()>,
) -> Result<Value> {
    if mode == "--local-status" {
        return local_status(root);
    }
    let _lock = lock(root, false)?;
    let mut state: State = load(&root.join("state.json"))?;
    if mode == "--config" {
        return update_config(&state.home);
    }
    if let Some(value) = mode.strip_prefix("--grant=") {
        return set_update_config(&state.home, "permission", value);
    }
    if let Some(value) = mode.strip_prefix("--mode=") {
        return set_update_config(&state.home, "mode", value);
    }
    if let Some(value) = mode.strip_prefix("--pin=") {
        return set_update_config(&state.home, "pinned", value);
    }
    if let Some(id) = mode.strip_prefix("--attach-node=") {
        let pending = state.pending.as_mut().ok_or("no update is pending")?;
        let path = local::maintenance_node_path(&state.home, id)?;
        pending.repair_node = Some(id.to_owned());
        persist(&root.join("state.json"), &state)?;
        return Ok(json!({"status":"repair_attached","repair_node":id,"path":path}));
    }
    // A requested status check bypasses the opportunistic interval but never installs a release.
    if mode == "--status" {
        state.attempt = Some(now());
        persist(&root.join("state.json"), &state)?;
        let check = (|| -> Result<Value> {
            let manifest = fetch_manifest(root)?;
            let current = package(&root.join(&state.application))?;
            opportunistic_notice(&current, &state, &manifest)
        })();
        if check.is_ok() {
            state.success = state.attempt.clone().unwrap();
        }
        persist(&root.join("state.json"), &state)?;
        let mut status = serde_json::to_value(&state)?;
        status["activation_pending"] = json!(root.join("activation.json").exists());
        match check {
            Ok(notice) => {
                status["update_check"] = notice["status"].clone();
                for field in [
                    "application",
                    "next_action",
                    "command",
                    "permission",
                    "grant_command",
                    "config",
                    "guide",
                ] {
                    if let Some(value) = notice.get(field) {
                        status[format!("available_{field}")] = value.clone();
                    }
                }
            }
            Err(error) => {
                status["update_check"] = json!("error");
                status["update_error"] = json!(error.to_string());
            }
        }
        return Ok(status);
    }
    let recovering = root.join("activation.json").exists();
    if recovering {
        let activation: Value = load(&root.join("activation.json"))?;
        let app: PathBuf = serde_json::from_value(activation["application"].clone())?;
        let policy = checked_runtime(&root.join(&app))?;
        if let Some(publication) = activation.get("publication") {
            let request: crate::checked_publication::Request =
                serde_json::from_value(publication.clone())?;
            require(
                request.repo == state.home && request.accepted_ref == "refs/heads/main",
                "rollback publication does not name the installed knowledge home",
            )?;
            // Retry the original request: submit's journal retains normalized HEAD
            // and cleanup custody even after main has accepted the candidate.
            let receipt = local::submit(&policy, &request)?;
            let accepted = accepted_submission(&receipt, &state.home)?;
            ready_home(&policy, &state.home, Some(&accepted))?;
        } else {
            ready_home(&policy, &state.home, None)?;
        }
        activate(root, &root.join(&app))?;
        if let Some(pinned) = activation.get("pinned").and_then(Value::as_str) {
            set_update_config(&state.home, "pinned", pinned)?;
        }
        state.application = app;
        state.application_sha256 = activation["application_sha256"]
            .as_str()
            .ok_or("missing recovery digest")?
            .into();
        state.knowledge = activation["knowledge"]
            .as_str()
            .ok_or("missing recovery KB")?
            .into();
        state.baseline = serde_json::from_value(activation["baseline"].clone())?;
        state.previous = serde_json::from_value(activation["previous"].clone())?;
        if let Some(base) = &state.baseline {
            git(&state.home, &["update-ref", "refs/sifttext/upstream", base])?;
        }
        state.pending = None;
        persist(&root.join("state.json"), &state)?;
        fs::remove_file(root.join("activation.json"))?;
    }
    if mode == "--recover" {
        if state
            .pending
            .as_ref()
            .and_then(|p| p.commit.as_ref())
            .is_some_and(|commit| {
                git(
                    &state.home,
                    &["merge-base", "--is-ancestor", commit, "refs/heads/main"],
                )
                .is_ok()
            })
        {
            return resume(root, &mut state, &mut preflight);
        }
        return Ok(json!({"status":"recovered"}));
    }
    if mode == "--rebuild" {
        return rebuild_active(root, &mut state);
    }
    if mode == "--resume" {
        if recovering {
            return Ok(
                json!({"status":"recovered","application":state.application,"knowledge":state.knowledge}),
            );
        }
        return resume(root, &mut state, &mut preflight);
    }
    if mode == "--rollback" {
        return rollback(root, &mut state);
    }
    if let Some(pending) = &state.pending {
        let mut result = json!({
            "status":"needs_agent",
            "worktree":pending.draft,
            "resume":"sifttext update --resume",
        });
        // Ordinary commands only report a pending update. Resolving the repair undertaking reads
        // the whole knowledge store and can publish the maintenance tree, so it belongs to the
        // explicit update the agent runs after reading the guide, not to every release check.
        if mode != "--opportunistic" {
            let policy = checked_runtime(&root.join(&state.application))?;
            let mut repair = repair_paths(&policy, &state.home)?;
            if let Some(id) = &pending.repair_node {
                repair["repair_node"] = json!(id);
                repair["repair_node_path"] = json!(local::maintenance_node_path(&state.home, id)?);
            }
            result["repair"] = repair;
        }
        return Ok(result);
    }
    if mode == "--opportunistic" {
        if !due(&state, &now()) {
            // The interval throttles network checks, not repeat notices for the cached release.
            let cached = root.join("last_manifest.json");
            if cached.exists() {
                if let Ok(config) = update_config(&state.home) {
                    if config["mode"] == "disabled" {
                        return Ok(json!({"status":"update_disabled"}));
                    }
                    if let Ok(manifest) = load::<Manifest>(&cached) {
                        if validate_manifest(&manifest).is_ok() {
                            let current = package(&root.join(&state.application))?;
                            return opportunistic_notice(&current, &state, &manifest);
                        }
                    }
                }
            }
            return Ok(json!({"status":"not_due"}));
        }
        let config = match update_config(&state.home) {
            Ok(config) => config,
            // A background release check reports a broken settings file at most once per retry
            // window, the way a failed download does, rather than on every command. The explicit
            // update and --config paths report the same refusal immediately.
            Err(error) => {
                state.attempt = Some(now());
                persist(&root.join("state.json"), &state)?;
                return Ok(json!({
                    "status":"update_config_invalid",
                    "error":error.to_string(),
                }));
            }
        };
        if config["mode"] == "disabled" {
            return Ok(json!({"status":"update_disabled"}));
        }
    }
    state.attempt = Some(now());
    persist(&root.join("state.json"), &state)?;
    let manifest = fetch_manifest(root)?;
    let result = if mode == "--opportunistic" {
        let current = package(&root.join(&state.application))?;
        opportunistic_notice(&current, &state, &manifest)?
    } else {
        apply(root, &mut state, &manifest, &mut preflight)?
    };
    state.success = now();
    persist(&root.join("state.json"), &state)?;
    Ok(result)
}
fn status_changes(before: &Path, after: &Path) -> Result<BTreeSet<String>> {
    use std::io::{BufRead, BufReader, Read};
    use std::os::unix::fs::MetadataExt;

    let status = |root: &Path, id: &str| -> Result<Option<String>> {
        crate::real_dir(&root.join("nodes"))?;
        let dir = root.join("nodes").join(id);
        match fs::symlink_metadata(&dir) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
            Ok(_) => crate::real_dir(&dir)?,
        }
        let file = match fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(dir.join("node.md"))
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let meta = file.metadata()?;
        require(meta.is_file() && meta.nlink() == 1 && meta.len() <= crate::LIMIT as u64,
            "unsafe or oversized rollback node")?;
        let mut reader = BufReader::new(file.take(crate::LIMIT as u64 + 1));
        let mut header = String::new();
        loop {
            let start = header.len();
            require(reader.read_line(&mut header)? != 0 && header.len() <= crate::LIMIT,
                "missing or oversized rollback node header")?;
            if start != 0 && header[start..].trim_end_matches(['\r', '\n']) == "---" {
                break;
            }
        }
        let parsed = crate::markdown(&header)?;
        require(parsed.authored.id == id, "rollback node identity differs from its path")?;
        Ok(Some(parsed.fields.get("status").ok_or("missing status")?.to_string()))
    };
    let base = git(before, &["rev-parse", "HEAD"])?;
    // NUL-delimited native paths avoid quoting/newline ambiguity and inspect only
    // changed node headers, not a pair of whole Store inventories or node bodies.
    let paths = git(after, &["diff", "--no-ext-diff", "--no-renames", "--name-only", "-z", &base, "--", "nodes"])?;
    require(paths.is_empty() || paths.ends_with('\0'), "ambiguous rollback paths")?;
    let mut explicit = BTreeSet::new();
    for path in paths.split_terminator('\0') {
        if !path.ends_with("/node.md") {
            continue;
        }
        let parts: Vec<_> = path.split('/').collect();
        require(parts.len() == 3 && parts[0] == "nodes" && crate::uuid(parts[1]),
            "unsafe rollback node path")?;
        let id = parts[1];
        if let (Some(old), Some(new)) = (status(before, id)?, status(after, id)?)
            && old != new
        {
            explicit.insert(id.to_owned());
        }
    }
    Ok(explicit)
}
fn rollback(root: &Path, state: &mut State) -> Result<Value> {
    require(
        state.pending.is_none(),
        "finish or inspect the retained update before rollback",
    )?;
    let previous = state
        .previous
        .clone()
        .ok_or("no previous release retained")?;
    let policy = checked_runtime(&root.join(&previous.application))?;
    if let Some(commit) = &previous.update_commit {
        crate::checked_publication::commit(&state.home, commit)?;
        git(&state.home, &["merge-base", "--is-ancestor", commit, "refs/heads/main"])?;
        let base = match &previous.update_base {
            Some(base) => {
                crate::checked_publication::commit(&state.home, base)?;
                git(&state.home, &["merge-base", "--is-ancestor", base, commit])?;
                base.clone()
            }
            None => {
                // Only the former single official merge contract has an
                // unambiguous first-parent delta. A normalization child does not.
                let parents = git(&state.home, &["rev-list", "--parents", "-n", "1", commit])?;
                let parents: Vec<_> = parents.split_whitespace().collect();
                require(parents.len() == 3
                    && Some(parents[2]) == state.baseline.as_deref()
                    && git(&state.home, &["show", "-s", "--format=%s", commit])?
                        == format!("knowledge: merge official {}", state.knowledge),
                    "legacy rollback has no exact update base or single official merge proof; preserve knowledge and recovery state for explicit repair")?;
                parents[1].to_owned()
            }
        };
        // Native binary diff preserves byte identity and three-way conflicts.
        // The shared runner refuses >16MiB output (never truncates a patch).
        let diff = crate::checked_publication::run_command(local::reader_command(&state.home,
            &["diff", "--binary", "--full-index", "--no-ext-diff", "--no-textconv", "--no-renames", &base, commit, "--", "nodes", "trees"]))?;
        require(diff.status.success() && !diff.stdout.is_empty(),
            "rollback delta unavailable or empty; preserve recovery state")?;
        let mut patch = tempfile::NamedTempFile::new_in(root)?;
        patch.write_all(&diff.stdout)?;
        patch.as_file().sync_all()?;
        drop(diff);
        let draft = local::worktree(&policy, &state.home)?;
        let path = PathBuf::from(draft["worktree"].as_str().ok_or("missing rollback draft")?);
        state.pending = Some(Pending {
            plan: UpdatePlan::Rollback(previous.clone()),
            application: previous.application.clone(),
            draft: path.clone(),
            base: draft["base"].as_str().unwrap().into(),
            upstream: previous.baseline.clone().unwrap_or_default(),
            commit: None,
            changed: Vec::new(),
            repair_node: None,
            accepted_delta: None,
        });
        persist(&root.join("state.json"), state)?;
        if let Err(error) = git(&path, &["apply", "--reverse", "--3way", "--index", "--", text(patch.path())?]) {
            return Ok(
                json!({"status":"needs_agent","worktree":path,"error":error.to_string(),"resume":"sifttext update --resume","reason":"resolve rollback in this retained worktree; active release unchanged"}),
            );
        }
    }
    resume_rollback(root, state, previous)
}
fn resume_rollback(root: &Path, state: &mut State, previous: Recovery) -> Result<Value> {
    let policy = checked_runtime(&root.join(&previous.application))?;
    let mut activation = json!({"application":previous.application,"application_sha256":previous.application_sha256,"knowledge":previous.knowledge,"baseline":previous.baseline,"previous":null});
    if let Some(mut pending) = state.pending.clone() {
        let path = &pending.draft;
        let explicit = if path.exists() {
            status_changes(&state.home, path)?
        } else {
            BTreeSet::new()
        };
        if pending.commit.is_none() {
            require(
                git(path, &["diff", "--name-only", "--diff-filter=U"])?.is_empty(),
                "resolve rollback conflicts in the retained worktree",
            )?;
            if !git(path, &["status", "--porcelain"])?.is_empty() {
                // --rollback selected these status intents; submit normalizes them
                // together with the exact combined candidate, not this filesystem.
                git(path, &["add", "--", "nodes", "trees"])?;
                git(
                    path,
                    &[
                        "commit",
                        "-m",
                        "knowledge: restore previous official revision",
                    ],
                )?;
            }
            let head = git(path, &["rev-parse", "HEAD"])?;
            require(
                head != pending.base,
                "rollback has no committed change; retain its worktree for inspection",
            )?;
            git(path, &["merge-base", "--is-ancestor", &pending.base, &head])?;
            pending.commit = Some(head);
            state.pending = Some(pending.clone());
            persist(&root.join("state.json"), state)?;
        }
        activation["publication"] = json!({"repo":state.home,"accepted_ref":"refs/heads/main","draft":path,"commit":pending.commit,"base":pending.base,"explicit":explicit});
        persist(&root.join("activation.json"), &activation)?;
        let request = serde_json::from_value(activation["publication"].clone())?;
        let receipt = local::submit(&policy, &request)?;
        let accepted = accepted_submission(&receipt, &state.home)?;
        ready_home(&policy, &state.home, Some(&accepted))?;
    } else {
        ready_home(&policy, &state.home, None)?;
    }
    persist(&root.join("activation.json"), &activation)?;
    activate(root, &root.join(&previous.application))?;
    state.application = previous.application;
    state.application_sha256 = previous.application_sha256;
    state.knowledge = previous.knowledge;
    state.baseline = previous.baseline;
    state.previous = None;
    state.pending = None;
    if let Some(base) = &state.baseline {
        git(&state.home, &["update-ref", "refs/sifttext/upstream", base])?;
    }
    persist(&root.join("state.json"), state)?;
    fs::remove_file(root.join("activation.json"))?;
    Ok(json!({"status":"rolled_back","application":state.application,"knowledge":state.knowledge}))
}
pub fn opportunistic() {
    if std::env::var_os("SIFTTEXT_NO_UPDATE").is_some_and(|value| value == "1") {
        return;
    }
    if let Some(root) = std::env::var_os("SIFTTEXT_INSTALL_ROOT") {
        match update(Path::new(&root), "--opportunistic") {
            Ok(value)
                if value["status"] != "not_due"
                    && value["status"] != "current"
                    && value["status"] != "update_disabled" =>
            {
                match crate::lean_render::render(&value) {
                    Ok(notice) => eprintln!("{notice}"),
                    Err(error) => eprintln!("SiftText update deferred: invalid notice: {error}"),
                }
            }
            Err(error) => eprintln!("SiftText update deferred: {error}"),
            _ => (),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn clock(n: u64) -> CheckTime {
        CheckTime {
            wall: n,
            uptime: n,
            boot: "fixture".into(),
        }
    }
    fn state() -> State {
        State {
            home: "/home/fixture".into(),
            application: "releases/v1".into(),
            application_sha256: "c".repeat(64),
            knowledge: "a".repeat(64),
            baseline: None,
            success: clock(100_000),
            attempt: Some(clock(100_000)),
            pending: None,
            previous: None,
        }
    }
    #[test]
    fn opportunistic_notice_reports_updates_without_applying_them() {
        let state = state();
        let current = Package {
            version: "v1".into(),
            source: "a".repeat(40),
            knowledge: "a".repeat(64),
            format: 1,
        };
        let asset = Asset {
            url: format!("{RELEASES}v1/fixture.tar.gz"),
            sha256: state.application_sha256.clone(),
        };
        let mut manifest = Manifest {
            schema: 1,
            application: Application {
                version: "v1".into(),
                source: current.source.clone(),
                format: 1,
                requires_knowledge: "b".repeat(64),
                assets: [(platform().unwrap(), asset.clone())].into(),
            },
            knowledge: Knowledge {
                revision: "b".repeat(64),
                format: 1,
                requires_application: "v1".into(),
                asset,
            },
        };
        let notice = opportunistic_notice(&current, &state, &manifest).unwrap();
        assert_eq!(notice["status"], "update_available");
        assert_eq!(notice["command"], "sifttext update");
        assert_eq!(notice["permission"], "ask");
        assert_eq!(
            crate::lean_render::render(&notice).unwrap(),
            format!(
                "application:v1\ncommand:sifttext update\ngrant_command:{}\nguide:/home/fixture/nodes/8b5361e1-20d7-42a9-9dca-26ba3bac9f25/node.md\nnext_action:{}\npermission:ask\nstatus:update_available",
                STANDING_GRANT_COMMAND, UPDATE_NOTICE_ASK
            )
        );
        assert_eq!(
            notice["guide"],
            state
                .home
                .join("nodes/8b5361e1-20d7-42a9-9dca-26ba3bac9f25/node.md")
                .to_string_lossy()
                .as_ref()
        );

        manifest.knowledge.revision = state.knowledge.clone();
        let current_notice = opportunistic_notice(&current, &state, &manifest).unwrap();
        assert_eq!(current_notice["status"], "current");
        assert_eq!(current_notice["application"], "v1");
        assert!(current_notice.get("knowledge").is_none());
        manifest.application.version = "v2".into();
        assert_eq!(
            opportunistic_notice(&current, &state, &manifest).unwrap()["status"],
            "update_available"
        );
    }

    #[test]
    fn opportunistic_notice_preserves_standing_permission_when_update_is_available() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state();
        state.home = dir.path().to_owned();
        set_update_config(&state.home, "permission", "standing").unwrap();
        let current = Package {
            version: "v1".into(),
            source: "a".repeat(40),
            knowledge: "a".repeat(64),
            format: 1,
        };
        let asset = Asset {
            url: format!("{RELEASES}v2/fixture.tar.gz"),
            sha256: state.application_sha256.clone(),
        };
        let manifest = Manifest {
            schema: 1,
            application: Application {
                version: "v2".into(),
                source: "b".repeat(40),
                format: 1,
                requires_knowledge: "b".repeat(64),
                assets: [(platform().unwrap(), asset.clone())].into(),
            },
            knowledge: Knowledge {
                revision: "b".repeat(64),
                format: 1,
                requires_application: "v2".into(),
                asset,
            },
        };

        let notice = opportunistic_notice(&current, &state, &manifest).unwrap();
        assert_eq!(notice["status"], "update_available");
        // A recorded standing grant needs no confirmation and no grant suggestion.
        assert!(notice.get("permission").is_none());
        assert!(notice.get("grant_command").is_none());
        assert_eq!(notice["next_action"], UPDATE_NOTICE_STANDING);
        assert_eq!(notice["command"], "sifttext update");
        assert!(notice.get("config").is_none());
        assert_eq!(
            update_config(&state.home).unwrap()["permission"],
            "standing"
        );
    }

    #[test]
    fn update_config_reads_the_users_grant_and_refuses_invented_values() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let config = home.join(".sifttext").join("config.json");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        let defaults = update_config(home).unwrap();
        assert_eq!(defaults["mode"], "notify");
        assert_eq!(defaults["permission"], "ask");
        assert_eq!(defaults["source"], "default");
        fs::write(&config, r#"{"updates":{"permission":"standing"}}"#).unwrap();
        let granted = update_config(home).unwrap();
        assert_eq!(granted["permission"], "standing");
        assert_eq!(granted["mode"], "notify");
        assert_eq!(granted["source"], "file");
        fs::write(&config, r#"{"updates":{"mode":"disabled"}}"#).unwrap();
        assert_eq!(update_config(home).unwrap()["mode"], "disabled");
        fs::write(&config, r#"{"updates":{"pinned":"v1"}}"#).unwrap();
        assert_eq!(update_config(home).unwrap()["pinned"], "v1");
        let cleared = set_update_config(home, "pinned", "current").unwrap();
        assert_eq!(cleared["pinned"], Value::Null);
        assert_eq!(
            set_update_config(home, "pinned", "v9").unwrap()["pinned"],
            "v9"
        );
        for body in [
            r#"{"updates":{"mode":"silent"}}"#,
            r#"{"updates":{"permission":"always"}}"#,
            r#"{"updates":{"track":"beta"}}"#,
            r#"{"updates":true}"#,
            r#"{"updates":{"pinned":"../elsewhere"}}"#,
            r#"{"updates":{"pinned":7}}"#,
        ] {
            fs::write(&config, body).unwrap();
            assert!(update_config(home).is_err(), "accepted {body}");
        }
        fs::write(
            &config,
            r#"{"session_retention":{"enabled":true,"archive_root":"/archive"}}"#,
        )
        .unwrap();
        let granted = set_update_config(home, "permission", "standing").unwrap();
        assert_eq!(granted["permission"], "standing");
        assert_eq!(granted["mode"], "notify");
        assert!(
            fs::read_to_string(&config)
                .unwrap()
                .contains("archive_root"),
            "recording a grant dropped a sibling setting"
        );
        assert!(set_update_config(home, "permission", "always").is_err());
        assert_eq!(update_config(home).unwrap()["permission"], "standing");
    }

    #[test]
    fn a_broken_settings_file_is_reported_once_not_on_every_command() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("install");
        let home = dir.path().join("home");
        fs::create_dir(&root).unwrap();
        fs::create_dir_all(home.join(".sifttext")).unwrap();
        fs::write(
            home.join(".sifttext/config.json"),
            r#"{"updates":{"mode":"silent"}}"#,
        )
        .unwrap();
        persist(
            &root.join("state.json"),
            &State {
                home: home.clone(),
                application: "releases/v1".into(),
                application_sha256: "c".repeat(64),
                knowledge: "a".repeat(64),
                baseline: None,
                success: clock(0),
                attempt: None,
                pending: None,
                previous: None,
            },
        )
        .unwrap();
        let first = update(&root, "--opportunistic").unwrap();
        assert_eq!(first["status"], "update_config_invalid");
        assert!(
            first["error"]
                .as_str()
                .is_some_and(|error| error.contains("updates.mode")),
            "{first}"
        );
        assert_eq!(
            update(&root, "--opportunistic").unwrap()["status"],
            "not_due",
            "a broken settings file reported again before the retry window"
        );
        assert!(
            update(&root, "--config")
                .unwrap_err()
                .to_string()
                .contains("updates.mode")
        );
    }

    #[test]
    fn repeats_cached_update_notice_without_another_network_check() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let (_policy, mut state, _, _, manifest) = fixture(&root);
        state.success = now();
        state.attempt = Some(state.success.clone());
        persist(&root.join("state.json"), &state).unwrap();
        persist(&root.join("last_manifest.json"), &manifest).unwrap();

        for _ in 0..2 {
            assert_eq!(
                update(&root, "--opportunistic").unwrap()["status"],
                "update_available"
            );
        }
    }

    #[test]
    fn check_interval_retry_and_clock_reset() {
        let mut s = state();
        assert!(!due(&s, &clock(100_001)));
        assert!(!due(&s, &clock(114_399)));
        assert!(due(&s, &clock(114_400)));
        assert!(due(&s, &clock(200_000)));
        s.attempt = Some(clock(114_400));
        assert!(!due(&s, &clock(114_401)));
        assert!(due(&s, &clock(114_700)));
        let mut shifted = clock(114_700);
        shifted.wall = 1;
        assert!(due(&s, &shifted));
        let mut reboot = clock(10);
        reboot.boot = "next boot".into();
        assert!(due(&s, &reboot));
    }
    #[test]
    fn cached_application_requires_exact_release_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        let asset = Asset {
            url: format!("{RELEASES}v2/package.tar.gz"),
            sha256: "a".repeat(64),
        };
        let manifest = Manifest {
            schema: 1,
            application: Application {
                version: "v2".into(),
                source: "b".repeat(40),
                format: 1,
                requires_knowledge: "c".repeat(64),
                assets: [(platform().unwrap(), asset.clone())].into(),
            },
            knowledge: Knowledge {
                revision: "c".repeat(64),
                format: 1,
                requires_application: "v2".into(),
                asset: asset.clone(),
            },
        };
        persist(
            &path.join("release.json"),
            &Package {
                version: "v2".into(),
                source: "b".repeat(40),
                knowledge: "c".repeat(64),
                format: 1,
            },
        )
        .unwrap();
        persist(
            &path.join(".sifttext-cache.json"),
            &CachedApplication {
                version: "v2".into(),
                source: "b".repeat(40),
                format: 1,
                asset_sha256: "a".repeat(64),
            },
        )
        .unwrap();
        assert!(verify_cached_application(path, &manifest, &asset).is_ok());
        let mut changed = asset;
        changed.sha256 = "d".repeat(64);
        assert!(verify_cached_application(path, &manifest, &changed).is_err());
    }

    #[test]
    fn a_pinned_build_keeps_its_application_but_still_offers_knowledge_updates() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let (_policy, mut state, _a, _b, manifest) = fixture(&root);
        fs::create_dir_all(state.home.join(".sifttext")).unwrap();
        persist(
            &state.home.join(".sifttext/config.json"),
            &json!({"updates":{"pinned":"v1"}}),
        )
        .unwrap();
        let current = package(&root.join(&state.application)).unwrap();

        // A release that changes only first-party knowledge reuses the pinned executable, so the
        // notice must still tell the agent to apply it.
        let kb_only = opportunistic_notice(&current, &state, &manifest).unwrap();
        assert_eq!(kb_only["status"], "update_available");
        assert_eq!(kb_only["pinned"], "v1");
        assert_eq!(kb_only["command"], "sifttext update");

        let mut newer = manifest.clone();
        newer.application.version = "v2".into();
        newer.knowledge.requires_application = "v2".into();
        let pinned = apply(&root, &mut state, &newer, &mut |_| Ok(())).unwrap();
        assert_eq!(pinned["status"], "update_pinned", "{pinned}");
        assert_eq!(pinned["pinned"], "v1");
        assert_eq!(pinned["application"], "v2");
        assert!(
            !root.join("releases").join("v2").exists(),
            "a pinned installation staged an official candidate"
        );
        assert_eq!(state.application, PathBuf::from("releases/v1"));
        let notice = opportunistic_notice(&current, &state, &newer).unwrap();
        assert_eq!(notice["pinned"], "v1");
        assert_eq!(notice["command"], "sifttext update --unpin");

        // A patched build usually keeps the official version string, so a knowledge-only release
        // reaches it under the same version with a different source identity. Without a pin that
        // still fails closed; with one, the local application stays and its knowledge can update.
        let mut patched = manifest.clone();
        patched.application.source = "d".repeat(40);
        fs::remove_file(state.home.join(".sifttext/config.json")).unwrap();
        assert!(
            opportunistic_notice(&current, &state, &patched)
                .unwrap_err()
                .to_string()
                .contains("identity changed"),
            "an unverified same-version application identity was accepted"
        );
        persist(
            &state.home.join(".sifttext/config.json"),
            &json!({"updates":{"pinned":"v1"}}),
        )
        .unwrap();
        let patched_notice = opportunistic_notice(&current, &state, &patched).unwrap();
        assert_eq!(patched_notice["status"], "update_available");
        assert_eq!(patched_notice["command"], "sifttext update");
        assert_eq!(patched_notice["pinned"], "v1");
    }

    #[test]
    fn trust_boundary_and_component_coupling() {
        let good = Asset {
            url: format!("{RELEASES}v0.1.0/package.tar.gz"),
            sha256: "a".repeat(64),
        };
        assert!(official(&good).is_ok());
        for bad in [
            "http://github.com/x",
            "https://example.invalid/package",
            "https://github.com/gojack10/sifttext-core/releases/download/../x",
            "https://github.com/gojack10/sifttext-core/releases/download/v1/../../x",
        ] {
            let mut a = good.clone();
            a.url = bad.into();
            assert!(official(&a).is_err(), "{bad}");
        }
        let mut m = Manifest {
            schema: 1,
            application: Application {
                version: "v0.1.0".into(),
                source: "source".into(),
                format: 1,
                requires_knowledge: "b".repeat(64),
                assets: [("darwin-arm64".into(), good.clone())].into(),
            },
            knowledge: Knowledge {
                revision: "b".repeat(64),
                format: 1,
                requires_application: "v0.1.0".into(),
                asset: good,
            },
        };
        assert!(validate_manifest(&m).is_ok());
        m.knowledge.requires_application = "v2".into();
        assert!(validate_manifest(&m).is_err());
    }
    fn fixture(root: &Path) -> (Policy, State, PathBuf, PathBuf, Manifest) {
        use std::os::unix::fs::PermissionsExt;
        let source = Path::new(env!("CARGO_MANIFEST_DIR"));
        let app = root.join("releases/v1");
        fs::create_dir_all(&app).unwrap();
        for name in [
            "Cargo.toml",
            "Cargo.lock",
            "README.md",
            ".gitignore",
            "rust-toolchain.toml",
            "src",
            "laws",
            "scripts",
            "tests",
            "fixtures",
            "docs",
            "build",
        ] {
            if source.join(name).exists() {
                copy_tree(&source.join(name), &app.join(name)).unwrap();
            }
        }
        // Synthetic process seam only. All knowledge checks/publications use real native policy assets.
        fs::write(app.join("sifttext"), "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(app.join("sifttext"), fs::Permissions::from_mode(0o700)).unwrap();
        persist(
            &app.join("release.json"),
            &Package {
                version: "v1".into(),
                source: "a".repeat(40),
                knowledge: "a".repeat(64),
                format: 1,
            },
        )
        .unwrap();
        symlink("releases/v1", root.join("current")).unwrap();
        let policy = Policy::checked(source).unwrap();
        let home = root.join("home");
        fs::create_dir(&home).unwrap();
        fs::create_dir(home.join("nodes")).unwrap();
        fs::create_dir(home.join("trees")).unwrap();
        fs::write(home.join("trees/.keep"), "").unwrap();
        fs::write(home.join(".gitignore"),"/.sifttext/\n/nodes/*/node.meta.json\n/nodes/*/outline*.lean\n/nodes/*/outline*.lean.meta.json\n").unwrap();
        git(&home, &["init", "-q", "-b", "main"]).unwrap();
        git(&home, &["config", "user.name", "Synthetic Fixture"]).unwrap();
        git(&home, &["config", "user.email", "fixture@example.invalid"]).unwrap();
        git(&home, &["add", "."]).unwrap();
        git(&home, &["commit", "-qm", "knowledge: empty fixture"]).unwrap();
        git(&home, &["checkout", "--detach", "-q"]).unwrap();
        let seed = local::worktree(&policy, &home).unwrap();
        let seed_path = Path::new(seed["worktree"].as_str().unwrap());
        let a = local::create(
            &policy,
            seed_path,
            "Localchoice",
            "Carries a customer change.",
            None,
        )
        .unwrap();
        let b = local::create(
            &policy,
            seed_path,
            "Officialchoice",
            "Carries an upstream change.",
            None,
        )
        .unwrap();
        let node_path = |v: &Value| {
            seed_path
                .join("nodes")
                .join(v["id"].as_str().unwrap())
                .join("node.md")
        };
        let a = node_path(&a);
        let b = node_path(&b);
        for p in [&a, &b] {
            let mut bytes = fs::read_to_string(p).unwrap();
            bytes.push_str("\nBase paragraph.\n");
            fs::write(p, bytes).unwrap();
        }
        git(seed_path, &["add", "nodes", "trees"]).unwrap();
        git(
            seed_path,
            &["commit", "-qm", "knowledge: upstream baseline"],
        )
        .unwrap();
        let seeded = git(seed_path, &["rev-parse", "HEAD"]).unwrap();
        let result = local::submit(
            &policy,
            &crate::checked_publication::Request {
                repo: home.clone(),
                accepted_ref: "refs/heads/main".into(),
                draft: Some(seed_path.into()),
                commit: Some(seeded),
                base: Some(seed["base"].as_str().unwrap().into()),
                explicit: BTreeSet::new(),
                pause_before_publish: None,
                pause_after_publish: None,
            },
        )
        .unwrap();
        assert_eq!(result["status"], "current", "{result}");
        let a = home.join(a.strip_prefix(seed_path).unwrap());
        let b = home.join(b.strip_prefix(seed_path).unwrap());
        let baseline = git(&home, &["rev-parse", "HEAD"]).unwrap();
        git(&home, &["update-ref", "refs/sifttext/upstream", &baseline]).unwrap();
        let kb = root.join("incoming");
        fs::create_dir(&kb).unwrap();
        for name in ["nodes", "trees"] {
            copy_tree(&home.join(name), &kb.join(name)).unwrap();
        }
        let allocation = local::worktree(&policy, &home).unwrap();
        let draft = Path::new(allocation["worktree"].as_str().unwrap());
        let edited = draft.join(a.strip_prefix(&home).unwrap());
        fs::write(
            &edited,
            fs::read_to_string(&edited)
                .unwrap()
                .replace("Base paragraph.", "Customer paragraph."),
        )
        .unwrap();
        crate::status::normalize(&policy, &home, draft, &BTreeSet::new(), None).unwrap();
        git(draft, &["add", "nodes"]).unwrap();
        git(
            draft,
            &["commit", "-qm", "knowledge: customer customization"],
        )
        .unwrap();
        let commit = git(draft, &["rev-parse", "HEAD"]).unwrap();
        let result = local::submit(
            &policy,
            &crate::checked_publication::Request {
                repo: home.clone(),
                accepted_ref: "refs/heads/main".into(),
                draft: Some(draft.into()),
                commit: Some(commit),
                base: Some(baseline.clone()),
                explicit: BTreeSet::new(),
                pause_before_publish: None,
                pause_after_publish: None,
            },
        )
        .unwrap();
        assert_eq!(result["status"], "current", "{result}");
        let mut s = state();
        s.home = home;
        s.baseline = Some(baseline);
        persist(&root.join("state.json"), &s).unwrap();
        let asset = Asset {
            url: format!("{RELEASES}v1/fixture.tar.gz"),
            sha256: s.application_sha256.clone(),
        };
        let manifest = Manifest {
            schema: 1,
            application: Application {
                version: "v1".into(),
                source: "a".repeat(40),
                format: 1,
                requires_knowledge: "b".repeat(64),
                assets: [(platform().unwrap(), asset.clone())].into(),
            },
            knowledge: Knowledge {
                revision: "b".repeat(64),
                format: 1,
                requires_application: "v1".into(),
                asset,
            },
        };
        (policy, s, a, b, manifest)
    }
    #[test]
    fn explicit_cli_status_forces_a_check_without_installing() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let (_policy, state, _, _, mut manifest) = fixture(&root);
        manifest.application.version = "v2".into();
        manifest.application.source = "d".repeat(40);
        manifest.application.requires_knowledge = state.knowledge.clone();
        manifest.knowledge.revision = state.knowledge.clone();
        manifest.knowledge.requires_application = "v2".into();
        for asset in manifest.application.assets.values_mut() {
            asset.url = format!("{RELEASES}v2/fixture.tar.gz");
        }
        manifest.knowledge.asset.url = format!("{RELEASES}v2/fixture.tar.gz");
        let manifest_path = root.join("manifest.json");
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let bin = root.join("mock-bin");
        fs::create_dir(&bin).unwrap();
        let curl = bin.join("curl");
        fs::write(
            &curl,
            "#!/bin/sh\nout=\nwhile [ $# -gt 0 ]; do if [ \"$1\" = --output ]; then out=$2; shift 2; else shift; fi; done\ncp \"$SIFTTEXT_TEST_MANIFEST\" \"$out\"\n",
        )
        .unwrap();
        fs::set_permissions(&curl, fs::Permissions::from_mode(0o700)).unwrap();
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
        let output = Command::new(crate::native_bin("sifttext"))
            .args(["update", "--status"])
            .env("PATH", path)
            .env("SIFTTEXT_INSTALL_ROOT", &root)
            .env("SIFTTEXT_TEST_MANIFEST", &manifest_path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("update_check:update_available"), "{stdout}");
        assert!(stdout.contains("available_application:v2"), "{stdout}");
        assert!(!stdout.starts_with('{'), "{stdout}");
        let shown = crate::outlines::decode_lean(&stdout).unwrap();
        for field in ["success", "attempt", "baseline", "application_sha256", "previous", "pending", "available_config"] {
            assert!(shown.get(field).is_none(), "{field}: {stdout}");
        }
        assert_eq!(shown["activation_pending"], false);
        assert!(
            fs::read_link(root.join("current"))
                .unwrap()
                .ends_with("releases/v1")
        );
        let saved: State = load(&root.join("state.json")).unwrap();
        assert!(saved.pending.is_none());
        assert_eq!(saved.application, state.application);
        assert_eq!(saved.success.wall, saved.attempt.unwrap().wall);
    }

    #[test]
    fn explicit_cli_update_activates_the_staged_release_once() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let (_policy, state, _, _, mut manifest) = fixture(&root);
        manifest.application.version = "v2".into();
        manifest.application.source = "d".repeat(40);
        manifest.application.requires_knowledge = state.knowledge.clone();
        manifest.knowledge.revision = state.knowledge.clone();
        manifest.knowledge.requires_application = "v2".into();

        let package_dir = root.join("package");
        copy_tree(&root.join("releases/v1"), &package_dir).unwrap();
        persist(
            &package_dir.join("release.json"),
            &Package {
                version: "v2".into(),
                source: manifest.application.source.clone(),
                knowledge: state.knowledge.clone(),
                format: 1,
            },
        )
        .unwrap();
        let archive = root.join("app.tar.gz");
        assert!(
            Command::new("tar")
                .args(["-czf"])
                .arg(&archive)
                .args(["-C"])
                .arg(&root)
                .arg("package")
                .status()
                .unwrap()
                .success()
        );
        let asset = manifest
            .application
            .assets
            .get_mut(&platform().unwrap())
            .unwrap();
        asset.url = format!("{RELEASES}v2/app.tar.gz");
        asset.sha256 = crate::sha256(&fs::read(&archive).unwrap());
        let asset_sha256 = asset.sha256.clone();
        let manifest_path = root.join("manifest.json");
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

        let bin = root.join("mock-bin");
        fs::create_dir(&bin).unwrap();
        let curl = bin.join("curl");
        fs::write(
            &curl,
            "#!/bin/sh\nout=\nurl=\nwhile [ $# -gt 0 ]; do\n  if [ \"$1\" = --output ]; then out=$2; shift 2; else url=$1; shift; fi\ndone\necho \"$url\" >> \"$SIFTTEXT_CURL_LOG\"\ncase \"$url\" in\n  */manifest.json) cp \"$SIFTTEXT_TEST_MANIFEST\" \"$out\" ;;\n  */v2/app.tar.gz) cp \"$SIFTTEXT_TEST_APP\" \"$out\" ;;\n  *) exit 9 ;;\nesac\n",
        )
        .unwrap();
        fs::set_permissions(&curl, fs::Permissions::from_mode(0o700)).unwrap();
        let log = root.join("curl.log");
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
        let output = Command::new(crate::native_bin("sifttext"))
            .args(["update"])
            .env("PATH", path)
            .env("SIFTTEXT_INSTALL_ROOT", &root)
            .env("SIFTTEXT_TEST_MANIFEST", &manifest_path)
            .env("SIFTTEXT_TEST_APP", &archive)
            .env("SIFTTEXT_CURL_LOG", &log)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("status:updated"));
        let expected = PathBuf::from(format!("releases/v2-{asset_sha256}"));
        assert_eq!(
            fs::read_link(root.join("current")).unwrap(),
            root.join(&expected)
        );
        let saved: State = load(&root.join("state.json")).unwrap();
        assert_eq!(saved.application, expected);
        assert_eq!(
            saved.previous.unwrap().application,
            PathBuf::from("releases/v1")
        );
        assert!(!root.join("activation.json").exists());
        assert_eq!(fs::read_to_string(log).unwrap().lines().count(), 2);
    }

    #[test]
    fn checked_three_way_merge_preserves_customer_work_and_rolls_back() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let (policy, mut state, a, b, m) = fixture(&root);
        let kb = root.join("incoming");
        let incoming = kb.join(b.strip_prefix(&state.home).unwrap());
        fs::write(
            &incoming,
            fs::read_to_string(&incoming)
                .unwrap()
                .replace("Base paragraph.", "Official paragraph."),
        )
        .unwrap();
        let result = merge_knowledge(
            &root,
            &mut state,
            &m,
            PathBuf::from("releases/v1"),
            &policy,
            &kb,
            &mut |_| Ok(()),
        )
        .unwrap();
        assert_eq!(result["status"], "updated", "{result}");
        assert!(
            fs::read_to_string(&a)
                .unwrap()
                .contains("Customer paragraph.")
        );
        assert!(
            fs::read_to_string(&b)
                .unwrap()
                .contains("Official paragraph.")
        );
        assert!(state.pending.is_none());
        assert_eq!(state.knowledge, m.knowledge.revision);
        assert!(
            !result["refresh_instruction_paths"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let restored = rollback(&root, &mut state).unwrap();
        assert_eq!(restored["status"], "rolled_back");
        assert!(
            fs::read_to_string(&a)
                .unwrap()
                .contains("Customer paragraph.")
        );
        assert!(fs::read_to_string(&b).unwrap().contains("Base paragraph."));
    }
    #[test]
    fn recovery_activates_after_rollback_publication_already_landed() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let (policy, mut state, customer, node, _) = fixture(&root);
        let customer_before = fs::read(&customer).unwrap();
        copy_tree(&root.join("releases/v1"), &root.join("releases/v2")).unwrap();
        activate(&root, &root.join("releases/v2")).unwrap();
        state.application = "releases/v2".into();
        persist(&root.join("state.json"), &state).unwrap();
        let allocation = local::worktree(&policy, &state.home).unwrap();
        let draft = PathBuf::from(allocation["worktree"].as_str().unwrap());
        let file = draft.join(node.strip_prefix(&state.home).unwrap());
        let mut contents = fs::read_to_string(&file).unwrap();
        contents.push_str("\nRollback change.\n");
        fs::write(&file, contents).unwrap();
        crate::status::normalize(&policy, &state.home, &draft, &BTreeSet::new(), None).unwrap();
        git(&draft, &["add", "nodes"]).unwrap();
        git(&draft, &["commit", "-qm", "knowledge: rollback fixture"]).unwrap();
        let commit = git(&draft, &["rev-parse", "HEAD"]).unwrap();
        let request = crate::checked_publication::Request {
            repo: state.home.clone(),
            accepted_ref: "refs/heads/main".into(),
            draft: Some(draft.clone()),
            commit: Some(commit.clone()),
            base: Some(allocation["base"].as_str().unwrap().into()),
            explicit: BTreeSet::new(),
            pause_before_publish: None,
            pause_after_publish: None,
        };
        let receipt = local::submit(&policy, &request).unwrap();
        assert_eq!(receipt["status"], "current", "{receipt}");
        assert!(!draft.exists(), "published draft must already be removed");
        let activation = json!({
            "application":"releases/v1",
            "application_sha256":state.application_sha256,
            "knowledge":state.knowledge,
            "baseline":state.baseline,
            "previous":null,
            "publication":{
                "repo":request.repo,
                "accepted_ref":request.accepted_ref,
                "draft":request.draft,
                "commit":request.commit,
                "base":request.base,
                "explicit":request.explicit,
                "pause_before_publish":null,
                "pause_after_publish":null,
            },
        });
        persist(&root.join("activation.json"), &activation).unwrap();
        let result = update(&root, "--resume").unwrap();
        assert_eq!(result["status"], "recovered");
        assert_eq!(git(&state.home, &["rev-parse", "HEAD"]).unwrap(), commit);
        assert_eq!(fs::read(&customer).unwrap(), customer_before);
        assert_eq!(fs::read_link(root.join("current")).unwrap(), root.join("releases/v1"));
        assert!(!root.join("activation.json").exists());
        assert_eq!(update(&root, "--status").unwrap()["application"], "releases/v1");
        assert_eq!(local::advance(&policy, &state.home).unwrap()["status"], "current");
    }

    #[test]
    fn recovery_refuses_unpublished_missing_candidate_without_changing_home() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let (_, state, customer, _, _) = fixture(&root);
        let before = git(&state.home, &["rev-parse", "HEAD"]).unwrap();
        let customer_before = fs::read(&customer).unwrap();
        let active_before = fs::read_link(root.join("current")).unwrap();
        persist(&root.join("activation.json"), &json!({
            "application":"releases/v1",
            "application_sha256":state.application_sha256,
            "knowledge":state.knowledge,
            "baseline":state.baseline,
            "previous":null,
            "publication":{
                "repo":state.home,"accepted_ref":"refs/heads/main",
                "draft":root.join("missing-draft"),"commit":"f".repeat(40),
                "base":before,"explicit":[]
            }
        })).unwrap();
        assert!(update(&root, "--resume").is_err());
        let saved: State = load(&root.join("state.json")).unwrap();
        assert_eq!(saved.application, state.application);
        assert!(saved.pending.is_none());
        assert_eq!(git(&state.home, &["rev-parse", "HEAD"]).unwrap(), before);
        assert_eq!(fs::read(&customer).unwrap(), customer_before);
        assert_eq!(fs::read_link(root.join("current")).unwrap(), active_before);
        assert!(root.join("activation.json").exists());
    }

    #[test]
    fn update_receipt_preflight_refuses_before_publication_and_retry_returns_same_receipt() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let (policy, mut state, _, b, manifest) = fixture(&root);
        let initial_head = git(&state.home, &["rev-parse", "HEAD"]).unwrap();
        let kb = root.join("incoming");
        let incoming = kb.join(b.strip_prefix(&state.home).unwrap());
        fs::write(
            &incoming,
            fs::read_to_string(&incoming)
                .unwrap()
                .replace("Base paragraph.", "Official paragraph."),
        )
        .unwrap();

        let mut preview = None;
        let error = merge_knowledge(
            &root,
            &mut state,
            &manifest,
            PathBuf::from("releases/v1"),
            &policy,
            &kb,
            &mut |receipt| {
                preview = Some(receipt.clone());
                Err("receipt suppressed".into())
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("receipt suppressed"));
        assert_eq!(
            git(&state.home, &["rev-parse", "HEAD"]).unwrap(),
            initial_head
        );
        assert!(state.pending.as_ref().unwrap().commit.is_some());

        let result = resume(&root, &mut state, &mut |_| Ok(())).unwrap();
        assert_eq!(result, preview.unwrap());
        assert_ne!(
            git(&state.home, &["rev-parse", "HEAD"]).unwrap(),
            initial_head
        );
        assert_eq!(result["status"], "updated");
    }

    #[test]
    fn conflict_keeps_accepted_state_readable_and_agent_resolution_resumes() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let (policy, mut state, a, _, m) = fixture(&root);
        let before = git(&state.home, &["rev-parse", "HEAD"]).unwrap();
        let kb = root.join("incoming");
        let incoming = kb.join(a.strip_prefix(&state.home).unwrap());
        fs::write(
            &incoming,
            fs::read_to_string(&incoming)
                .unwrap()
                .replace("Base paragraph.", "Official paragraph."),
        )
        .unwrap();
        let result = merge_knowledge(
            &root,
            &mut state,
            &m,
            PathBuf::from("releases/v1"),
            &policy,
            &kb,
            &mut |_| Ok(()),
        )
        .unwrap();
        assert_eq!(result["status"], "needs_agent", "{result}");
        assert_ne!(git(&state.home, &["rev-parse", "HEAD"]).unwrap(), before);
        assert!(result["repair"]["maintenance_tree"].as_str().is_some());
        assert!(result["repair"]["guide"].as_str().is_some());
        assert!(
            fs::read_to_string(&a)
                .unwrap()
                .contains("Customer paragraph.")
        );
        local::check(&policy, &state.home, None, BTreeSet::new()).unwrap();
        let tree_id = result["repair"]["maintenance_tree_id"].as_str().unwrap();
        let allocation = local::worktree(&policy, &state.home).unwrap();
        let repair_draft = PathBuf::from(allocation["worktree"].as_str().unwrap());
        let undertaking = local::create(
            &policy,
            &repair_draft,
            "Resolve Update Conflict",
            "Reconcile the customer's KB conflict.",
            Some(tree_id),
        )
        .unwrap();
        git(&repair_draft, &["add", "--", "nodes", "trees"]).unwrap();
        git(
            &repair_draft,
            &["commit", "-qm", "maintenance: record update repair"],
        )
        .unwrap();
        let commit = git(&repair_draft, &["rev-parse", "HEAD"]).unwrap();
        let repair_receipt = local::submit(
            &policy,
            &crate::checked_publication::Request {
                repo: state.home.clone(),
                accepted_ref: "refs/heads/main".into(),
                draft: Some(repair_draft),
                commit: Some(commit),
                base: Some(allocation["base"].as_str().unwrap().to_owned()),
                explicit: BTreeSet::new(),
                pause_before_publish: None,
                pause_after_publish: None,
            },
        )
        .unwrap();
        assert_eq!(repair_receipt["status"], "current", "{repair_receipt}");
        let invoke_update = |args: &[&str]| {
            Command::new(crate::native_bin("sifttext"))
                .arg("update")
                .args(args)
                .env("SIFTTEXT_INSTALL_ROOT", &root)
                .output()
                .unwrap()
        };
        let repair_id = undertaking["id"].as_str().unwrap();
        let repair_path = state.home.join("nodes").join(repair_id).join("node.md");
        let attached_arg = format!("--attach-node={repair_id}");
        for args in [
            &[attached_arg.as_str()][..],
            &["--attach-node", repair_id][..],
        ] {
            let output = invoke_update(args);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let attached =
                crate::outlines::decode_lean(std::str::from_utf8(&output.stdout).unwrap()).unwrap();
            assert_eq!(attached["status"], "repair_attached");
            assert_eq!(attached["repair_node"], repair_id);
            // Publication removes the draft; later agents need the accepted home path.
            assert_eq!(attached["path"], repair_path.to_string_lossy().as_ref());
            let saved: State = load(&root.join("state.json")).unwrap();
            assert_eq!(
                saved.pending.unwrap().repair_node.as_deref(),
                Some(repair_id)
            );
        }
        let saved = fs::read(root.join("state.json")).unwrap();
        let outside_id = a.parent().unwrap().file_name().unwrap().to_str().unwrap();
        for (id, message) in [
            ("not-a-uuid", "invalid maintenance node UUID"),
            (
                outside_id,
                "repair node must belong to SiftText Maintenance",
            ),
        ] {
            let output = invoke_update(&["--attach-node", id]);
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains(message));
            assert_eq!(fs::read(root.join("state.json")).unwrap(), saved);
        }
        assert!(
            repair_path.is_file(),
            "repair node is not readable at {repair_path:?}"
        );
        let mut state: State = load(&root.join("state.json")).unwrap();
        assert_eq!(
            state.pending.as_ref().unwrap().repair_node.as_deref(),
            undertaking["id"].as_str()
        );
        let output = invoke_update(&[]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let later_notice =
            crate::outlines::decode_lean(std::str::from_utf8(&output.stdout).unwrap()).unwrap();
        assert_eq!(later_notice["status"], "needs_agent");
        let repair: Value = serde_json::from_str(later_notice["repair"].as_str().unwrap()).unwrap();
        assert_eq!(repair["repair_node"], undertaking["id"]);
        assert_eq!(
            repair["repair_node_path"],
            repair_path.to_string_lossy().as_ref()
        );
        let pending = state.pending.as_ref().unwrap();
        let path = pending.draft.join(a.strip_prefix(&state.home).unwrap());
        fs::write(
            &path,
            fs::read_to_string(&a).unwrap().replace(
                "Customer paragraph.",
                "Customer and official paragraphs reconciled.",
            ),
        )
        .unwrap();
        crate::status::normalize(&policy, &state.home, &pending.draft, &BTreeSet::new(), None)
            .unwrap();
        git(&pending.draft, &["add", "nodes"]).unwrap();
        git(
            &pending.draft,
            &["commit", "-qm", "knowledge: reconcile official update"],
        )
        .unwrap();
        let result = resume(&root, &mut state, &mut |_| Ok(())).unwrap();
        assert_eq!(result["status"], "updated", "{result}");
        assert!(
            fs::read_to_string(&a)
                .unwrap()
                .contains("Customer and official paragraphs reconciled.")
        );
        let saved = fs::read(root.join("state.json")).unwrap();
        let output = invoke_update(&[attached_arg.as_str()]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("no update is pending"));
        assert_eq!(fs::read(root.join("state.json")).unwrap(), saved);
    }
    #[test]
    fn local_status_cli_is_read_only_during_shared_and_exclusive_operations() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::Instant;

        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let mut s = state(); // The home need not exist: inspection must never enter it.
        s.previous = Some(Recovery {
            application: "releases/v0".into(),
            application_sha256: "d".repeat(64),
            knowledge: "e".repeat(64),
            baseline: None,
            update_commit: None,
        });
        s.pending = Some(Pending {
            plan: UpdatePlan::Rollback(s.previous.clone().unwrap()),
            application: "releases/v0".into(),
            draft: root.join("absent-draft"),
            base: "fixture-base".into(),
            upstream: "fixture-upstream".into(),
            commit: None,
            changed: vec!["private-journal-detail".into()],
            repair_node: None,
        });
        persist(&root.join("state.json"), &s).unwrap();
        let before = fs::read(root.join("state.json")).unwrap();
        let modified = fs::metadata(root.join("state.json")).unwrap().modified().unwrap();
        let bin = root.join("mock-bin");
        fs::create_dir(&bin).unwrap();
        // Any attempted download is a test failure without contacting a remote service.
        fs::write(bin.join("curl"), "#!/bin/sh\nprintf attempted > \"$SIFTTEXT_INSTALL_ROOT/network-attempt\"\nexit 91\n").unwrap();
        fs::set_permissions(bin.join("curl"), fs::Permissions::from_mode(0o700)).unwrap();
        let invoke = |args: &[&str]| {
            let started = Instant::now();
            let result = Command::new(crate::native_bin("sifttext"))
                .args(args)
                .env("PATH", &bin)
                .env("SIFTTEXT_INSTALL_ROOT", &root)
                .env_remove("SIFTTEXT_NO_UPDATE")
                .output().unwrap();
            assert!(started.elapsed().as_secs_f64() < 5.0, "local diagnostic or busy refusal stalled");
            result
        };
        let inspect = || {
            let output = invoke(&["update", "--local-status"]);
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            let stdout = String::from_utf8(output.stdout).unwrap();
            assert!(!stdout.contains("private-journal-detail") && !stdout.contains(&"d".repeat(64)), "{stdout}");
            let shown = crate::outlines::decode_lean(&stdout).unwrap();
            assert_eq!(shown["status"], "recorded_only");
            assert_eq!(shown["consistency"], "not_atomic_snapshot");
            assert_eq!(shown["update_check"], "not_performed");
            assert_eq!(shown["publication_state"], "not_inspected");
            assert_eq!(shown["pending_operation"], "rollback");
            shown
        };
        assert_eq!(inspect()["activation_marker_present"], false);
        assert!(!root.join("update.lock").exists(), "inspection created a lock");
        let shared = lock(&root, true).unwrap(); // Exact mutex/lifetime used by ordinary operations.
        assert_eq!(inspect()["application"], "releases/v1");
        for args in [&["update"][..], &["update", "--status"], &["update", "--resume"], &["update", "--rebuild"]] {
            let refused = invoke(args);
            assert!(!refused.status.success());
            assert!(String::from_utf8_lossy(&refused.stderr).contains("update busy"));
        }
        let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        let home_parent = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(home_parent.path()).unwrap().join("home");
        assert!(initialize(&policy, &root, &home).unwrap_err().to_string().contains("update busy"));
        assert!(!home.exists());
        assert_eq!(fs::read(root.join("state.json")).unwrap(), before);
        assert_eq!(fs::metadata(root.join("state.json")).unwrap().modified().unwrap(), modified);
        drop(shared);
        let exclusive = lock(&root, false).unwrap();
        assert_eq!(inspect()["application"], "releases/v1");
        // Simulate the real writer's atomic file replacement, separately from marker creation.
        s.application = "releases/v2".into();
        persist(&root.join("state.json"), &s).unwrap();
        fs::write(root.join("activation.json"), "not parsed by local inspection").unwrap();
        let written = fs::read(root.join("state.json")).unwrap();
        let shown = inspect();
        assert_eq!(shown["application"], "releases/v2");
        assert_eq!(shown["activation_marker_present"], true);
        assert_eq!(fs::read(root.join("state.json")).unwrap(), written);
        assert!(lock(&root, false).is_err(), "inspection removed the writer's exclusion");
        assert!(!root.join("network-attempt").exists());
        assert!(!root.join("last_manifest.json").exists());
        let mut names = fs::read_dir(&root).unwrap().map(|e| e.unwrap().file_name()).collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, ["activation.json", "mock-bin", "state.json", "update.lock"].map(std::ffi::OsString::from));
        drop(exclusive);
    }

    #[test]
    fn local_status_refuses_missing_invalid_and_unsafe_state_without_creating_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let missing = root.join("absent-install");
        assert!(update(&missing, "--local-status").is_err());
        assert!(!missing.exists());
        assert!(update(&root, "--local-status").is_err());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        let path = root.join("state.json");
        fs::write(&path, br#""credential-like-secret""#).unwrap();
        let error = update(&root, "--local-status").unwrap_err().to_string();
        assert!(error.contains("invalid update state.json") && !error.contains("credential-like-secret"));
        assert_eq!(fs::read(&path).unwrap(), br#""credential-like-secret""#);
        fs::remove_file(&path).unwrap();
        let private = root.join("private");
        fs::write(&private, "credential-like-secret").unwrap();
        symlink(&private, &path).unwrap();
        assert!(update(&root, "--local-status").is_err());
        fs::remove_file(&path).unwrap();
        fs::hard_link(&private, &path).unwrap();
        assert!(update(&root, "--local-status").is_err());
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(update(&root, "--local-status").is_err());
        assert_eq!(fs::read(&private).unwrap(), b"credential-like-secret");
        assert!(!root.join("update.lock").exists());
        let unset = Command::new(crate::native_bin("sifttext"))
            .args(["update", "--local-status"])
            .env_remove("SIFTTEXT_INSTALL_ROOT")
            .output().unwrap();
        assert!(!unset.status.success());
        assert!(String::from_utf8_lossy(&unset.stderr).contains("managed installation launcher"));
    }

    #[test]
    fn shared_operation_excludes_activation_and_state_writes_are_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let held = lock(&root, true).unwrap();
        assert!(lock(&root, false).is_err());
        drop(held);
        assert!(lock(&root, false).is_ok());
        let s = state();
        persist(&root.join("state.json"), &s).unwrap();
        let got: State = load(&root.join("state.json")).unwrap();
        assert_eq!(got.knowledge, s.knowledge);
    }

    mod patch_build_tests {
        include!("../tests/successor/patch_build.rs");
    }
}
