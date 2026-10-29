//! Opt-in accepted-knowledge transport. Git is the queue; the journal is custody,
//! not acceptance evidence. No server, model, session/archive or secret backup.
mod delivery;
mod runner;
mod selection;
use super::account;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sifttext_file_graph::{checked_publication, local, Result};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const ENDPOINT: &str = "https://workspace.sifttext.com";
const RESOURCE: &str = "https://workspace.sifttext.com/mcp";
const LIMIT: usize = 32 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct State {
    enabled: bool,
    endpoint: String,
    device: String,
    #[serde(default)]
    binding: Binding,
    uploaded: Option<String>,
    #[serde(default)]
    pending: Option<Pending>,
    // Independent continuations, not a queue: each received history gets the
    // same checked attempt without replacing another repair's journal.
    #[serde(default)]
    additional_pending: Vec<Pending>,
    #[serde(default)]
    remote_pending: Value,
    #[serde(default)]
    projection: Option<selection::Projection>,
}
// Missing ownership in an older journal is unknown, NOT fresh offline consent.
#[derive(Default, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Binding {
    #[default]
    LegacyUnknown,
    Unbound,
    Bound {
        home_id: String,
    },
}
#[derive(Serialize, Deserialize)]
struct Pending {
    accepted: String,
    incoming: String,
    base: Option<String>,
    worktree: PathBuf,
    guide: PathBuf,
    check: String,
    #[serde(default)]
    source_device: Option<String>,
}
impl State {
    fn repairs(&self) -> impl Iterator<Item = &Pending> {
        self.pending.iter().chain(&self.additional_pending)
    }
    fn repair_mut(&mut self, incoming: &str) -> &mut Pending {
        self.pending
            .iter_mut()
            .chain(&mut self.additional_pending)
            .find(|p| p.incoming == incoming)
            .expect("journaled incoming")
    }
    fn retain_repairs(&mut self, keep: impl Fn(&Pending) -> bool) {
        if self.pending.as_ref().is_some_and(|p| !keep(p)) {
            self.pending = None;
        }
        self.additional_pending.retain(keep);
    }
}
struct Store {
    home: PathBuf,
    root: PathBuf,
    persisted: RefCell<Option<Vec<u8>>>,
    _lock: Option<fs::File>,
}

fn git_command(home: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.as_encoded_bytes().starts_with(b"GIT_") {
            command.env_remove(key);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_GRAFT_FILE", "/dev/null")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("-C")
        .arg(home)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.attributesFile=/dev/null",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "merge.autoStash=false",
            "-c",
            "rerere.enabled=false",
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
        ])
        .args(args)
        .stdin(Stdio::null());
    command
}
fn git(home: &Path, args: &[&str]) -> Result<String> {
    let output = bounded(git_command(home, args))?;
    if !output.status.success() {
        return Err(format!(
            "Git {} failed; accepted/draft histories retained",
            args.first().unwrap_or(&"operation")
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
fn command_input(mut command: Command, input: &[u8]) -> Result<Vec<u8>> {
    let mut file = tempfile::tempfile()?;
    file.write_all(input)?;
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(0))?;
    command.stdin(file);
    let output = bounded(command)?;
    if !output.status.success() {
        return Err("native Git publication operation failed; histories retained".into());
    }
    Ok(output.stdout)
}
fn git_input(home: &Path, args: &[&str], input: &[u8]) -> Result<Vec<u8>> {
    command_input(git_command(home, args), input)
}
// Bounded process output and lifetime. Kill the
// complete helper group on overflow/timeout; never leave a pack helper running.
fn bounded(mut command: Command) -> Result<std::process::Output> {
    use std::os::unix::process::CommandExt;
    const CAP: usize = 64 * 1024 * 1024;
    let mut child = command
        .process_group(0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let group = child.id() as libc::pid_t;
    let pipes: [Box<dyn Read + Send>; 2] = [
        Box::new(child.stdout.take().ok_or("missing Git stdout")?),
        Box::new(child.stderr.take().ok_or("missing Git stderr")?),
    ];
    let (tx, rx) = std::sync::mpsc::channel();
    let readers: Vec<_> = pipes
        .into_iter()
        .enumerate()
        .map(|(tag, pipe)| {
            let tx = tx.clone();
            thread::spawn(move || {
                let mut bytes = Vec::new();
                let result = pipe
                    .take((CAP + 1) as u64)
                    .read_to_end(&mut bytes)
                    .map(|_| bytes);
                let _ = tx.send((tag, result));
            })
        })
        .collect();
    drop(tx);
    let start = Instant::now();
    let mut bytes = [Vec::new(), Vec::new()];
    let mut received = 0;
    let result = (|| -> Result<std::process::Output> {
        loop {
            while let Ok((tag, result)) = rx.try_recv() {
                received += 1;
                bytes[tag] = result?;
                if bytes.iter().map(Vec::len).sum::<usize>() > CAP {
                    return Err("Git output resource limit".into());
                }
            }
            if received == 2 {
                if let Some(status) = child.try_wait()? {
                    let [stdout, stderr] = std::mem::take(&mut bytes);
                    return Ok(std::process::Output {
                        status,
                        stdout,
                        stderr,
                    });
                }
            }
            if start.elapsed() >= Duration::from_secs(30) {
                return Err("Git helper timeout".into());
            }
            thread::sleep(Duration::from_millis(2));
        }
    })();
    if result.is_err() {
        unsafe {
            libc::kill(-group, libc::SIGKILL);
        }
        let _ = child.wait();
    }
    let drain = Instant::now();
    while received < 2 && drain.elapsed() < Duration::from_secs(1) {
        if rx.recv_timeout(Duration::from_millis(10)).is_ok() {
            received += 1;
        }
    }
    if received != 2 {
        return Err("Git termination unconfirmed; preserve sync custody".into());
    }
    for reader in readers {
        let _ = reader.join();
    }
    result
}
fn oid(value: &str) -> Result<&str> {
    if ![40, 64].contains(&value.len())
        || !value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err("expected full Git object ID".into());
    }
    Ok(value)
}
fn head(home: &Path) -> Result<String> {
    git(home, &["rev-parse", "refs/heads/main"])
}
fn reader_tips(home: &Path) -> Result<(String, String)> {
    let tips = git(
        home,
        &["rev-parse", "HEAD^{commit}", "refs/heads/main^{commit}"],
    )?;
    let (reader, accepted) = tips
        .split_once('\n')
        .ok_or("missing reader/accepted tips")?;
    Ok((oid(reader)?.into(), oid(accepted)?.into()))
}
fn ancestor(home: &Path, a: &str, b: &str) -> bool {
    a == b || git(home, &["merge-base", "--is-ancestor", a, b]).is_ok()
}
fn endpoint() -> Result<String> {
    let endpoint = std::env::var("SIFTTEXT_SYNC_ORIGIN").unwrap_or_else(|_| ENDPOINT.into());
    if endpoint != ENDPOINT
        && !endpoint
            .strip_prefix("http://127.0.0.1:")
            .and_then(|p| p.parse::<u16>().ok())
            .is_some_and(|p| p > 0)
    {
        return Err("sync origin must be official Workspace or explicit loopback port".into());
    }
    Ok(endpoint)
}
fn atomic(path: &Path, value: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("missing state parent")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    temp.write_all(value)?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
impl Store {
    fn open(home: &Path, create: bool, locking: bool) -> Result<Self> {
        let home = fs::canonicalize(home)?;
        if git(&home, &["rev-parse", "--show-toplevel"])? != home.to_str().ok_or("non-UTF8 home")?
            || !home.join(".git").symlink_metadata()?.is_dir()
        {
            return Err(
                "sync requires an independent initialized knowledge home, not a draft/project"
                    .into(),
            );
        }
        let root = home.join(".git/sifttext-sync");
        if create && !root.try_exists()? {
            fs::DirBuilder::new().mode(0o700).create(&root)?;
        }
        let meta = fs::symlink_metadata(&root)?;
        if !meta.is_dir()
            || meta.uid() != unsafe { libc::geteuid() }
            || meta.mode() & 0o777 != 0o700
        {
            return Err("unsafe sync state directory (requires owned real 0700 directory)".into());
        }
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(root.join("lock"))?;
        if !lock.metadata()?.is_file() || lock.metadata()?.nlink() != 1 {
            return Err("unsafe sync lock".into());
        }
        if locking {
            lock.try_lock()
                .map_err(|_| "sync already running for this home")?;
        }
        Ok(Self {
            home,
            root,
            persisted: RefCell::new(None),
            _lock: locking.then_some(lock),
        })
    }
    fn load(&self) -> Result<State> {
        *self.persisted.borrow_mut() = None;
        let mut file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(self.root.join("selected-state.json"))?;
        let meta = file.metadata()?;
        if !meta.is_file()
            || meta.nlink() != 1
            || meta.uid() != unsafe { libc::geteuid() }
            || meta.mode() & 0o777 != 0o600
        {
            return Err("unsafe sync state file".into());
        }
        let mut bytes = Vec::new();
        (&mut file).take(1024 * 1024).read_to_end(&mut bytes)?;
        let state = serde_json::from_slice(&bytes)?;
        // Compare logical state, not JSON whitespace or omitted legacy defaults.
        *self.persisted.borrow_mut() = Some(serde_json::to_vec(&state)?);
        Ok(state)
    }
    fn save(&self, state: &State) -> Result<()> {
        let bytes = serde_json::to_vec(state)?;
        if self.persisted.borrow().as_ref() == Some(&bytes) {
            return Ok(());
        }
        // A failed atomic write may have renamed before directory fsync failed.
        // Forget the old snapshot so even a reverted state must be persisted again.
        *self.persisted.borrow_mut() = None;
        atomic(&self.root.join("selected-state.json"), &bytes)?;
        *self.persisted.borrow_mut() = Some(bytes);
        Ok(())
    }
}
struct Exchange {
    auth: account::SyncAccess,
    endpoint: String,
    home_id: Option<String>,
}
impl Exchange {
    fn open(state: &State) -> Result<Self> {
        if state.endpoint != endpoint()? {
            return Err("configured sync origin differs; preserve this enrollment and explicitly enroll a new home for another service".into());
        }
        let home_id = match &state.binding {
            Binding::LegacyUnknown => return Err("legacy sync enrollment has no verified owner; transfer paused. Preserve this home and its repairs, disable its runner, and explicitly enroll a new clone under the intended account".into()),
            Binding::Unbound => None,
            Binding::Bound { home_id } => Some(home_id.clone()),
        };
        Ok(Self {
            auth: account::SyncAccess::open(RESOURCE)?,
            endpoint: state.endpoint.clone(),
            home_id,
        })
    }
    fn verify_home(&mut self, remote: &Value) -> Result<()> {
        if remote["protocol"] != 1 {
            return Err("unsupported sync protocol".into());
        }
        // Only the authenticated service's stable private-account home identifies
        // ownership. Never infer it from local flags, device IDs or JWT contents.
        let home_id = remote["home"]["id"]
            .as_str()
            .filter(|id| {
                id.len() == 64
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
            .ok_or("sync service did not confirm a stable account home; no transfer attempted")?;
        if self
            .home_id
            .as_deref()
            .is_some_and(|expected| expected != home_id)
        {
            return Err("sync account home changed; transfer refused. Original enrollment, local/cloud history and repairs retained. Sign back into the original account, or explicitly enroll a separate clone for another account".into());
        }
        self.home_id = Some(home_id.into());
        Ok(())
    }
    fn send(&self, path: &str, body: Option<&str>) -> Result<(u16, Value)> {
        account::http(
            &self.endpoint,
            path,
            Some(self.auth.token()?),
            body.map(|s| ("application/json", s)),
        )
    }
    fn request(&mut self, path: &str, body: Option<&Value>) -> Result<Value> {
        let encoded = body.map(Value::to_string);
        let mut response = self.send(path, encoded.as_deref())?;
        if response.0 == 401 {
            self.auth.renew(RESOURCE)?;
            // Renewal must prove the SAME home before replaying even a POST or
            // downloading objects. A new token alone is not ownership evidence.
            let remote = checked_response(self.send("/sync/v1/state", None)?)?;
            self.verify_home(&remote)?;
            response = if path == "/sync/v1/state" {
                (200, remote)
            } else {
                self.send(path, encoded.as_deref())?
            };
        }
        let value = checked_response(response)?;
        if path == "/sync/v1/state" {
            self.verify_home(&value)?;
        }
        Ok(value)
    }
}
fn checked_response(response: (u16, Value)) -> Result<Value> {
    match response.0 {
        200..=299 => Ok(response.1),
        401 => Err("Workspace rejected renewed authorization; saved login retained. Check account authorization.".into()),
        403 => Err("Workspace access inactive or forbidden; check subscription/account, local knowledge remains usable".into()),
        413 => Err("selected knowledge exceeds current sync transfer limit; local knowledge retained".into()),
        status => {
            let reason: String = response.1["error"]
                .as_str()
                .unwrap_or("service unavailable")
                .chars()
                .filter(|c| !c.is_control())
                .take(1024)
                .collect();
            Err(format!("Sync request failed (HTTP {status}): {reason}. Accepted history remains queued in Git; inspect the cause before retrying.").into())
        }
    }
}
enum HistoryScope<'a> {
    Received,
    Publication { hosted: Option<&'a str> },
}
// Inspect received knowledge paths separately from new publication ancestry.
// Already-hosted history may retain provenance; new uploads may not introduce it.
fn scoped_history(store: &Store, revision: &str, scope: HistoryScope<'_>) -> Result<()> {
    if !fs::symlink_metadata(store.home.join(".git/shallow"))
        .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
    {
        return Err("sync requires complete reachable history, not a shallow repository".into());
    }
    // This ref names a closure checked by the EXACT allowlist below, not native
    // acceptance or an upload acknowledgement. Bump its version when that policy
    // changes; the old installed/main v1 JSON tree sets are deliberately not reused.
    let (scoped, received, hosted) = match scope {
        HistoryScope::Received => (
            "refs/sifttext-sync/scope/received-knowledge-paths-v1",
            true,
            None,
        ),
        HistoryScope::Publication { hosted } => (
            "refs/sifttext-sync/scope/publication-paths-v3",
            false,
            hosted,
        ),
    };
    let boundary = git(
        &store.home,
        &[
            "for-each-ref",
            "--format=%(objectname) %(objecttype) %(symref)",
            scoped,
        ],
    )?;
    let boundary = boundary
        .split_once(' ')
        .filter(|(sha, kind)| oid(sha).is_ok() && *kind == "commit")
        .map(|(sha, _)| sha);
    if boundary == Some(revision) {
        return Ok(());
    }
    // Git excludes only the previously inspected closure. Initial/unrelated
    // histories still walk their unknown closure; missing objects fail normally.
    let mut args = vec!["rev-list", "--format=%T", "--no-commit-header", revision];
    let exclusions = boundary
        .into_iter()
        .chain(hosted)
        .map(|sha| oid(sha).map(|sha| format!("^{sha}")))
        .collect::<Result<Vec<_>>>()?;
    args.extend(exclusions.iter().map(String::as_str));
    let inventory = git(&store.home, &args)?;
    let trees: BTreeSet<_> = inventory.lines().collect();
    for tree in trees {
        oid(tree)?;
        for line in git(&store.home, &["ls-tree", "-r", tree])?.lines() {
            let (meta, path) = line.split_once('\t').ok_or("invalid tree inventory")?;
            let parts: Vec<_> = path.split('/').collect();
            let regular = meta.starts_with("100644 blob ");
            let link = meta.starts_with("120000 blob ");
            let uuid = |s: &str| {
                s.len() == 36
                    && s.bytes().enumerate().all(|(i, b)| {
                        if [8, 13, 18, 23].contains(&i) {
                            b == b'-'
                        } else {
                            b.is_ascii_hexdigit()
                        }
                    })
            };
            let allowed = match parts.as_slice() {
                [".gitignore"] | ["nodes", ".keep"] | ["trees", ".keep"] => regular,
                ["nodes", id, "node.md" | "children.order"] => regular && uuid(id),
                ["nodes", id, "session-sources.json"] => received && regular && uuid(id),
                ["nodes", id, "children", ".keep"] => regular && uuid(id),
                ["nodes", id, "children", child] => link && uuid(id) && uuid(child),
                ["trees", id] => link && uuid(id),
                _ => false,
            };
            if !allowed {
                return Err("sync scope excludes non-knowledge history or session attachments; use a dedicated selected knowledge home (no data removed)".into());
            }
        }
    }
    if !inventory.is_empty() {
        // Only this subsystem's single boundary moves; no pruning of other refs.
        git(&store.home, &["update-ref", "--no-deref", scoped, revision])?;
    }
    Ok(())
}
fn enrollment_home(home: &Path) -> Result<()> {
    const IGNORE: &[u8] = b"/.sifttext/\n/nodes/*/node.meta.json\n/nodes/*/outline*.lean\n/nodes/*/outline*.lean.meta.json\n";
    if fs::read(home.join(".gitignore"))? != IGNORE {
        return Err("not an initialized local knowledge home".into());
    }
    let dotgit = home.join(".git");
    for name in [
        "HEAD",
        "config",
        "config.worktree",
        "index",
        "packed-refs",
        "refs/heads/main",
    ] {
        if let Ok(meta) = fs::symlink_metadata(dotgit.join(name)) {
            if !meta.is_file() || meta.nlink() != 1 {
                return Err("unsafe reader Git file".into());
            }
        }
    }
    for name in ["refs", "refs/heads"] {
        if let Ok(meta) = fs::symlink_metadata(dotgit.join(name)) {
            if !meta.is_dir() {
                return Err("unsafe reader Git directory".into());
            }
        }
    }
    for name in [
        "objects/info/alternates",
        "shallow",
        "commondir",
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "info/sparse-checkout",
    ] {
        if !fs::symlink_metadata(dotgit.join(name))
            .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        {
            return Err("unfinished, indirect or sparse knowledge reader".into());
        }
    }
    if fs::read(dotgit.join("HEAD"))?.starts_with(b"ref:")
        || !git(
            home,
            &["for-each-ref", "--format=%(symref)", "refs/heads/main"],
        )?
        .is_empty()
        || git(home, &["worktree", "list", "--porcelain"])?
            .lines()
            .any(|s| s == "branch refs/heads/main")
    {
        return Err(
            "knowledge home must be detached; accepted main must not be checked out".into(),
        );
    }
    if !git(home, &["ls-files", "-v"])?
        .lines()
        .all(|s| s.starts_with("H "))
        || !git(
            home,
            &[
                "status",
                "--porcelain=v1",
                "--untracked-files=all",
                "--ignore-submodules=none",
            ],
        )?
        .is_empty()
    {
        return Err(
            "knowledge reader is dirty or has hidden/sparse edits; preserve files before enrolling"
                .into(),
        );
    }
    git(
        home,
        &[
            "diff",
            "--exit-code",
            "--no-ext-diff",
            "--no-textconv",
            "HEAD",
            "--",
        ],
    )?;
    Ok(())
}
fn upload(
    store: &Store,
    state: &mut State,
    exchange: &mut Exchange,
    remote: Option<&str>,
    remote_pending: &Value,
    commit: &str,
    consent_home: &Path,
    policy: &selection::Settings,
) -> Result<()> {
    if remote == Some(commit) {
        if state.uploaded.as_deref() != Some(commit) {
            state.uploaded = Some(commit.into());
            store.save(state)?;
        }
        return Ok(());
    }
    // A stale acknowledgement plus ANY nonempty remote main is not custody.
    // Reuse only a freshly retained exact contribution or known accepted ancestry.
    if state.uploaded.as_deref() == Some(commit)
        && (remote_pending
            .as_array()
            .is_some_and(|records| records.iter().any(|p| p["incoming"] == commit))
            || remote.is_some_and(|r| ancestor(&store.home, commit, r)))
    {
        return Ok(());
    }
    scoped_history(store, commit, HistoryScope::Publication { hosted: remote })?;
    let bundle = tempfile::NamedTempFile::new_in(&store.root)?;
    let path = bundle.path().to_str().ok_or("non-UTF8 bundle")?;
    let exclusion = remote
        .filter(|r| *r != commit && ancestor(&store.home, r, commit))
        .map(|r| format!("^{r}"));
    let mut args = vec!["bundle", "create", path, "refs/heads/main"];
    if let Some(exclusion) = &exclusion {
        args.push(exclusion);
    }
    git(&store.home, &args)?;
    if !git(&store.home, &["bundle", "list-heads", path])?
        .lines()
        .any(|line| line == format!("{commit} refs/heads/main"))
    {
        return Err("accepted main changed during transfer preparation; retry".into());
    }
    if bundle.as_file().metadata()?.len() > LIMIT as u64 {
        return Err("knowledge bundle exceeds 32MiB transfer limit; local history retained".into());
    }
    let body = json!({"operation":"snapshot-import-v1","publication":"selected-current-v1","commit":commit,"base":remote,"bundle":STANDARD.encode(fs::read(bundle.path())?)});
    if serde_json::to_value(selection::settings(consent_home)?)? != serde_json::to_value(policy)?
        || policy.mode != "enabled"
    {
        return Err("consent/sharing changed during bundle preparation; upload refused".into());
    }
    let response = exchange.request(&format!("/sync/v1/devices/{}", state.device), Some(&body))?;
    delivery::acknowledge(&response, commit)?;
    state.uploaded = Some(commit.into());
    state.remote_pending = response["pending"].clone();
    store.save(state)
}
fn receive(
    store: &Store,
    state: &mut State,
    runtime: &Path,
    incoming: &str,
    source_device: Option<String>,
) -> Result<()> {
    oid(incoming)?;
    let accepted = head(&store.home)?;
    if ancestor(&store.home, incoming, &accepted) {
        // Clear only the actually completed continuation, not an unrelated older
        // received draft if the service is temporarily reporting an earlier main.
        state.retain_repairs(|p| !ancestor(&store.home, &p.incoming, &accepted));
        store.save(state)?;
        return Ok(());
    }
    if let Some(pending) = state.repairs().find(|p| p.incoming == incoming) {
        // Only replay the exact interrupted mechanical request, never a known conflict.
        if !["received", "home_or_publication_busy"].contains(&pending.check.as_str()) {
            return Ok(());
        }
    }
    let base = state
        .repairs()
        .find(|p| p.incoming == incoming)
        .and_then(|p| p.base.clone())
        .or_else(|| git(&store.home, &["merge-base", &accepted, incoming]).ok());
    let suffix = &incoming[..16];
    let worktree = store.root.join(format!("repair-{suffix}"));
    let guide = store.root.join(format!("repair-{suffix}.md"));
    let source = source_device
        .as_deref()
        .unwrap_or("shared hosted accepted main")
        .to_owned();
    // Journal before allocation: a restart can recover a registered draft, never reset it.
    if !state.repairs().any(|p| p.incoming == incoming) {
        let pending = Pending {
            accepted: accepted.clone(),
            incoming: incoming.into(),
            base: base.clone(),
            worktree: worktree.clone(),
            guide: guide.clone(),
            check: "received".into(),
            source_device,
        };
        if state.pending.is_none() {
            state.pending = Some(pending);
        } else {
            state.additional_pending.push(pending);
        }
    }
    store.save(state)?;
    if !guide.try_exists()? {
        atomic(&guide,format!("# Reconcile received accepted knowledge\n\nAccepted: `{accepted}`\nIncoming: `{incoming}` (received from {source}; receipt is custody, not semantic endorsement)\nPrivate repair worktree: `{}`\n\nNormal accepted knowledge and other drafts remain usable. Defer unrelated work freely; reconcile before relying on both histories. Inspect both versions and Git conflict markers, preserve competing reasons, and ask the human when meaning/authority is unclear. No clock-latest-wins.\n\nResolve only this retained Git worktree within your knowledge-write permission, preserve both contributions, review and commit. Submission combines accepted history and applies the native acceptance rules; no standalone check or mandatory normalize step is required. Submit with `sifttext submit '{}' '{}' COMMIT '{}'` (use the actual full repaired COMMIT). If there is no common baseline, do not invent one: explicitly reconcile/import the intended trees in a normal Core worktree under user authority. Never reset accepted main or another draft. Repeating `sifttext sync once '{}'` verifies accepted ancestry and clears the continuation after checked publication.\n",worktree.display(),store.home.display(),worktree.display(),base.as_deref().unwrap_or("NO_COMMON_BASE"),store.home.display()).as_bytes())?;
    }
    if !worktree.try_exists()? {
        let branch = format!("sifttext/sync-{suffix}");
        if git(
            &store.home,
            &["rev-parse", "--verify", &format!("refs/heads/{branch}")],
        )
        .is_ok()
        {
            git(
                &store.home,
                &[
                    "worktree",
                    "add",
                    worktree.to_str().ok_or("non-UTF8 repair")?,
                    &branch,
                ],
            )?;
        } else {
            git(
                &store.home,
                &[
                    "worktree",
                    "add",
                    "-b",
                    &branch,
                    worktree.to_str().ok_or("non-UTF8 repair")?,
                    incoming,
                ],
            )?;
        }
    }
    for name in ["nodes", "trees"] {
        fs::create_dir_all(worktree.join(name))?;
    }
    let Some(base) = base else {
        state.repair_mut(incoming).check = "unrelated_histories".into();
        return store.save(state);
    };
    let receipt = local::submit_received(
        &super::checked_policy(runtime)?,
        &checked_publication::Request {
            repo: store.home.clone(),
            accepted_ref: "refs/heads/main".into(),
            draft: Some(worktree),
            commit: Some(incoming.into()),
            base: Some(base),
            explicit: BTreeSet::new(),
            pause_before_publish: None,
            pause_after_publish: None,
        },
    );
    match receipt {
        Ok(value)
            if ["current", "saved", "recovered"]
                .iter()
                .any(|status| value["status"] == *status)
                && value["worktree_removed"] == true
                && ancestor(&store.home, incoming, &head(&store.home)?) =>
        {
            state.retain_repairs(|p| p.incoming != incoming);
        }
        Ok(value) => {
            state.repair_mut(incoming).check = value["check"]
                .as_str()
                .unwrap_or("candidate_rejected")
                .into()
        }
        Err(_) => state.repair_mut(incoming).check = "home_or_publication_busy".into(),
    }
    store.save(state)
}
fn compatible(value: &Value) -> Result<()> {
    delivery::compatible(value)?;
    if value["publication"] != "selected-current-v1"
    {
        return Err("sync service requires compatible selected-current-v1 and durable-receipt-v1 support; no transfer attempted, local knowledge remains usable".into());
    }
    Ok(())
}
fn cycle(store: &Store, state: &mut State, runtime: &Path) -> Result<()> {
    let policy = selection::settings(&store.home)?;
    if policy.mode != "enabled" {
        return Err("sync disabled; local knowledge unchanged".into());
    }
    let mut exchange = Exchange::open(state)?;
    let remote = exchange.request("/sync/v1/state", None)?;
    compatible(&remote)?;
    if matches!(state.binding, Binding::Unbound) {
        state.binding = Binding::Bound {
            home_id: exchange
                .home_id
                .clone()
                .ok_or("missing verified sync home")?,
        };
        store.save(state)?;
    }
    state.remote_pending = remote["pending"].clone();
    store.save(state)?;
    if delivery::processing(&state.remote_pending)? {
        // Server-owned completion is not a client repair or a reason to resend.
        // In particular, don't fetch a just-accepted main while its worker owns
        // the mutation lane, or build another publication on the older baseline.
        return Ok(());
    }
    let (reader, accepted) = reader_tips(&store.home)?;
    if reader != accepted {
        local::advance(&super::checked_policy(runtime)?, &store.home)?;
    }
    let publication = selection::repository(store)?;
    let remote_sha = remote["accepted"].as_str().map(oid).transpose()?;
    let have = state
        .projection
        .as_ref()
        .filter(|p| oid(&p.published).is_ok())
        .map(|p| p.published.clone());
    // No objects request or projection scan for an unchanged shared identity.
    if remote_sha != have.as_deref() {
        if let Some(incoming) = remote_sha {
            // Exact retained object custody is reusable even when local native
            // acceptance is blocked. GET state still confirms today's owner,
            // generation and pending journals; no repeat conflict download.
            let retained = git(
                &publication.home,
                &[
                    "for-each-ref",
                    "--format=%(objectname) %(objecttype) %(symref)",
                    &format!("refs/sifttext-sync/objects/{incoming}"),
                ],
            )?
            .trim()
                == format!("{incoming} commit");
            if !retained {
                let path = have
                    .as_ref()
                    .map(|h| format!("/sync/v1/objects?have={h}"))
                    .unwrap_or_else(|| "/sync/v1/objects".into());
                let response = exchange.request(&path, None)?;
                compatible(&response)?;
                if response["home"]["id"] != remote["home"]["id"] {
                    return Err("sync object ownership mismatch".into());
                }
                if response["accepted"].as_str() != remote_sha {
                    return Err("hosted accepted state changed; retry without uploading".into());
                }
                import_bundle(&publication, &response, incoming)?;
            }
            scoped_history(&publication, incoming, HistoryScope::Received)?;
            if let Err(error) = selection::import(store, &publication, state, runtime, incoming) {
                retain_selected_error(store, state, incoming, &error.to_string())?;
                return Err(error);
            }
        }
    }
    // Pending hosted contributions remain visible and in server custody; they
    // cannot be silently overwritten by a current-state snapshot.
    if state
        .remote_pending
        .as_array()
        .is_none_or(|p| !p.is_empty())
    {
        return Err("hosted completion is blocked or needs reconciliation; retained remote_pending is available in sync status".into());
    }
    // An unacknowledged local projection may contain newly excluded content.
    // Never parent it merely because a previous failed cycle built it.
    let parent = remote_sha;
    let published = selection::project(store, &publication, state, &policy, parent)?;
    if serde_json::to_value(selection::settings(&store.home)?)? != serde_json::to_value(&policy)? {
        return Err("sharing policy changed before upload; retry".into());
    }
    // Only the independent publication main and its already-hosted ancestry can
    // reach this bundle. Full local commit IDs never become upload prerequisites.
    upload(
        &publication,
        state,
        &mut exchange,
        remote_sha,
        &remote["pending"],
        &published,
        &store.home,
        &policy,
    )?;
    store.save(state)
}
fn retain_selected_error(
    store: &Store,
    state: &mut State,
    incoming: &str,
    error: &str,
) -> Result<()> {
    let local_tip = head(&store.home)?;
    let policy_id =
        sifttext_file_graph::sha256(&serde_json::to_vec(&selection::settings(&store.home)?)?);
    let previous = state.projection.as_ref().and_then(|p| p.blocked.as_ref());
    if previous.is_some_and(|p| {
        p["incoming"] == incoming && p["local_tip"] == local_tip && p["policy_id"] == policy_id
    }) {
        return Ok(());
    }
    let mut blocked = previous
        .filter(|p| p["incoming"] == incoming)
        .cloned()
        .unwrap_or_else(|| json!({}));
    if blocked["repair_worktree"].is_null() {
        let guide = store
            .root
            .join(format!("selected-incoming-{}.md", &incoming[..16]));
        atomic(&guide, format!("# Hosted edit retained for local reconciliation\n\nIncoming `{incoming}` remains in the private publication Git store under `{}`. No local knowledge was discarded. Reason: {error}\n\nInspect `sifttext sync status '{}'` and any translated repair guide. Reconcile only under the user's knowledge-write authority. Never treat an omitted local-only node as a deletion or reset either history.\n", store.root.join("publication").display(), store.home.display()).as_bytes())?;
        blocked["repair_guide"] = json!(guide);
    }
    blocked["incoming"] = json!(incoming);
    blocked["local_tip"] = json!(local_tip);
    blocked["policy_id"] = json!(policy_id);
    blocked["check"] = json!(error);
    if let Some(projection) = &mut state.projection {
        projection.blocked = Some(blocked);
    } else {
        state.projection = Some(selection::Projection {
            blocked: Some(blocked),
            ..Default::default()
        });
    }
    store.save(state)
}
fn import_bundle(store: &Store, response: &Value, incoming: &str) -> Result<()> {
    if let Some(encoded) = response["bundle"].as_str() {
        if encoded.len() > LIMIT.div_ceil(3) * 4 {
            return Err("oversized received bundle".into());
        }
        let bytes = STANDARD.decode(encoded)?;
        if bytes.len() > LIMIT {
            return Err("oversized received bundle".into());
        }
        let mut bundle = tempfile::NamedTempFile::new_in(&store.root)?;
        bundle.write_all(&bytes)?;
        let path = bundle.path().to_str().ok_or("non-UTF8 bundle")?;
        git(&store.home, &["bundle", "verify", path])?;
        if git(&store.home, &["bundle", "list-heads", path])?
            != format!("{incoming} refs/heads/main")
        {
            return Err("received bundle does not match advertised history".into());
        }
        git(
            &store.home,
            &[
                "fetch",
                "--no-tags",
                path,
                &format!("refs/heads/main:refs/sifttext-sync/objects/{incoming}"),
            ],
        )?;
    }
    Ok(())
}
fn unresolved(store: &Store, state: &State, accepted: &str) -> Result<BTreeSet<String>> {
    let mut incoming = BTreeSet::new();
    for p in state.repairs() {
        incoming.insert(p.incoming.clone());
    }
    for p in state.remote_pending.as_array().into_iter().flatten() {
        if let Some(sha) = p["incoming"].as_str() {
            oid(sha)?;
            incoming.insert(sha.to_owned());
        }
    }
    Ok(incoming
        .into_iter()
        .filter(|sha| !ancestor(&store.home, sha, accepted))
        .collect())
}
fn status(store: &Store, state: &State) -> Result<Value> {
    let accepted = head(&store.home)?;
    let pending = unresolved(store, state, &accepted)?;
    let mut reconciliations = Vec::new();
    for p in state.repairs() {
        if pending.contains(&p.incoming) {
            if !p.worktree.is_dir() || !p.guide.is_file() {
                return Err("interrupted sync preparation; run sifttext sync once HOME to recover retained incoming objects before using its repair paths".into());
            }
            let conflicts = git(&p.worktree, &["diff", "--name-only", "--diff-filter=U"])?
                .lines()
                .count();
            reconciliations.push(json!({"status":"reconciliation_required","accepted":accepted,"incoming":p.incoming,"source_device":p.source_device.as_deref().unwrap_or("shared-hosted-main"),"pending_count":pending.len(),"conflicted_files":conflicts,"repair_guide":p.guide,"repair_worktree":p.worktree,"check":p.check}));
        }
    }
    let pending = if reconciliations.is_empty() {
        Value::Null
    } else {
        reconciliations.remove(0)
    };
    Ok(
        json!({"enabled":selection::settings(&store.home)?.mode == "enabled","consent":selection::settings(&store.home)?,"device":state.device,"binding":state.binding,"accepted":accepted,"uploaded_publication":state.uploaded,"publication":state.projection,"reconciliation":pending,"additional_reconciliations":reconciliations,"remote_pending":state.remote_pending,"runner":runner::status(&store.home),"runner_error":runner::error(store)?}),
    )
}
/// No network or credentials on ordinary instructions. Only retained received work.
pub(super) fn instructions(home: &Path) -> Result<Vec<Value>> {
    if !home
        .join(".git/sifttext-sync/selected-state.json")
        .try_exists()?
    {
        return Ok(Vec::new());
    }
    let store = Store::open(home, false, false)?;
    let state = store.load()?;
    let mut repairs = Vec::new();
    let error = runner::error(&store)?;
    if !error.is_null() {
        repairs.push(json!({"status":"sync_failed","runner_error":error["error"],"retry":error["retry"]}));
    }
    if state.repairs().next().is_none()
        && state
            .projection
            .as_ref()
            .is_none_or(|p| p.blocked.is_none())
    {
        return Ok(repairs);
    }
    let result = status(&store, &state)?;
    if let Some(blocked) = state.projection.as_ref().and_then(|p| p.blocked.as_ref()) {
        repairs.push(blocked.clone());
    }
    if !result["reconciliation"].is_null() {
        repairs.push(result["reconciliation"].clone());
    }
    repairs.extend(
        result["additional_reconciliations"]
            .as_array()
            .into_iter()
            .flatten()
            .cloned(),
    );
    Ok(repairs)
}
pub(super) fn guide_path() -> Result<PathBuf> {
    let path = local::runtime()?.join("docs/account-sync.md");
    if !path.is_file() {
        return Err("installed account/sync guide missing; preserve local knowledge and repair the package".into());
    }
    Ok(path)
}

fn migrate(store: &Store) -> Result<()> {
    let legacy = store.root.join("state.json");
    if !legacy.try_exists()? {
        return Ok(());
    }
    let meta = fs::symlink_metadata(&legacy)?;
    if !meta.is_file()
        || meta.nlink() != 1
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o777 != 0o600
        || meta.len() > 1024 * 1024
    {
        return Err("unsafe legacy sync journal; no transfer permitted".into());
    }
    let mut value: Value = serde_json::from_slice(&fs::read(&legacy)?)?;
    let prior = store.root.join("legacy-enrollment.json");
    if !prior.try_exists()? {
        atomic(&prior, &serde_json::to_vec(&value)?)?;
    }
    // Fence older clients FIRST. Keep every legacy repair/binding/receipt intact.
    if value["enabled"] != false {
        value["enabled"] = json!(false);
        atomic(&legacy, &serde_json::to_vec(&value)?)?;
    }
    if !store.root.join("selected-state.json").try_exists()? {
        let original: Value = serde_json::from_slice(&fs::read(&prior)?)?;
        let was_enabled = original["enabled"]
            .as_bool()
            .ok_or("invalid legacy enrollment")?;
        let mut state: State = serde_json::from_value(value)?;
        state.enabled = false;
        state.uploaded = None;
        state.projection = None;
        let mut settings = selection::settings(&store.home)?;
        if settings.mode == "unanswered" {
            settings.mode = if was_enabled { "enabled" } else { "disabled" }.into();
            selection::save_settings(&store.home, &settings)?;
        }
        store.save(&state)?;
    }
    Ok(())
}
fn initialize(store: &Store) -> Result<State> {
    migrate(store)?;
    if store.root.join("selected-state.json").try_exists()? {
        return store.load();
    }
    let mut random = [0u8; 16];
    fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
    let state = State {
        enabled: false,
        endpoint: endpoint()?,
        device: random.iter().map(|b| format!("{b:02x}")).collect(),
        binding: Binding::Unbound,
        uploaded: None,
        pending: None,
        additional_pending: Vec::new(),
        remote_pending: json!([]),
        projection: None,
    };
    store.save(&state)?;
    Ok(state)
}
pub(super) fn run(runtime: &Path, args: &[String]) -> Result<Value> {
    let quiet = matches!(args, [_, operation, _, flag] if operation == "once" && flag == "--quiet");
    let args = if quiet { &args[..3] } else { args };
    if args.len() < 3 || args.len() > 4 {
        return Err("usage: sifttext sync enable|disable|decline|status|once|run HOME; sync policy HOME FILE.json".into());
    }
    let operation = args[1].as_str();
    let home = Path::new(&args[2]);
    if ![
        "enable", "disable", "decline", "status", "once", "run", "tick", "policy",
    ]
    .contains(&operation)
        || (args.len() == 4) != (operation == "policy")
    {
        return Err("unknown sync operation/arguments".into());
    }
    if operation == "status"
        && !home
            .join(".git/sifttext-sync/selected-state.json")
            .try_exists()?
    {
        let home = fs::canonicalize(home)?;
        if !home.join(".git").symlink_metadata()?.is_dir()
            || git(&home, &["rev-parse", "--show-toplevel"])?
                != home.to_str().ok_or("non-UTF8 home")?
        {
            return Err("sync requires an independent initialized knowledge home".into());
        }
        return Ok(
            json!({"enabled":false,"status":if home.join(".git/sifttext-sync/state.json").try_exists()? {"legacy_enrollment_requires_migration"} else {"unconfigured"},"consent":selection::settings(&home)?,"runner":runner::status(&home)}),
        );
    }
    if operation == "run" {
        loop {
            {
                let store = Store::open(home, false, true)?;
                migrate(&store)?;
                let mut state = store.load()?;
                if selection::settings(&store.home)?.mode != "enabled" {
                    return status(&store, &state);
                }
                match cycle(&store, &mut state, runtime) {
                    Ok(()) => runner::record(&store, None)?,
                    Err(error) => {
                        runner::record(&store, Some(&error.to_string()))?;
                        eprintln!("Knowledge sync failed; retrying in 15 seconds: {error}");
                    }
                }
            }
            thread::sleep(Duration::from_secs(15));
        }
    }
    let store = Store::open(
        home,
        ["enable", "disable", "decline", "policy"].contains(&operation),
        operation != "status",
    )?;
    if operation == "status" {
        return status(&store, &store.load()?);
    }
    let mut state = initialize(&store)?;
    match operation {
        "enable" => {
            enrollment_home(&store.home)?;
            if state.endpoint != endpoint()? {
                return Err(
                    "cannot change an existing sync service; preserve enrollment and binding"
                        .into(),
                );
            }
            let mut settings = selection::settings(&store.home)?;
            settings.mode = "enabled".into();
            selection::save_settings(&store.home, &settings)?;
            // Legacy enabled is permanently false: old clients cannot ignore policy.
            state.enabled = false;
            store.save(&state)?;
            match runner::install(&store.home) {
                Ok(()) => (),
                Err(error) => {
                    runner::record(&store, Some(&error.to_string()))?;
                    return Err(error);
                }
            }
            status(&store, &state)
        }
        "disable" | "decline" => {
            let mut settings = selection::settings(&store.home)?;
            settings.mode = if operation == "decline" {
                "declined"
            } else {
                "disabled"
            }
            .into();
            selection::save_settings(&store.home, &settings)?;
            state.enabled = false;
            store.save(&state)?; // Consent blocks transfers even if OS teardown fails.
            runner::disable(&store.home)?;
            status(&store, &state)
        }
        "policy" => {
            selection::set_policy(&store.home, Path::new(&args[3]))?;
            status(&store, &state)
        }
        "once" | "tick" => {
            match cycle(&store, &mut state, runtime) {
                Ok(()) => runner::record(&store, None)?,
                Err(error) => {
                    runner::record(&store, Some(&error.to_string()))?;
                    return Err(error);
                }
            }
            if quiet || operation == "tick" {
                Ok(Value::Null)
            } else {
                status(&store, &state)
            }
        }
        _ => unreachable!(),
    }
}
