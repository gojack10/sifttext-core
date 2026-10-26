//! Opt-in accepted-knowledge transport. Git is the queue; the journal is custody,
//! not acceptance evidence. No server, model, session/archive or secret backup.
use super::account;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sifttext_file_graph::{Policy, Result, checked_publication, local};
use std::{
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
const LIMIT: usize = 8 * 1024 * 1024;

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
    _lock: Option<fs::File>,
}

fn git(home: &Path, args: &[&str]) -> Result<String> {
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
        .args(args);
    let output = bounded(command)?;
    if !output.status.success() {
        return Err(format!(
            "Git {} failed; accepted/draft histories retained",
            args.first().unwrap_or(&"operation")
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
// Same 30s/16MiB boundary as publication's private native runner. Kill the
// complete helper group on overflow/timeout; never leave a pack helper running.
fn bounded(mut command: Command) -> Result<std::process::Output> {
    use std::os::unix::process::CommandExt;
    const CAP: usize = 16 * 1024 * 1024;
    let mut child = command
        .process_group(0)
        .stdin(Stdio::null())
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
fn ancestor(home: &Path, a: &str, b: &str) -> bool {
    git(home, &["merge-base", "--is-ancestor", a, b]).is_ok()
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
            _lock: locking.then_some(lock),
        })
    }
    fn load(&self) -> Result<State> {
        let mut file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(self.root.join("state.json"))?;
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
        Ok(serde_json::from_slice(&bytes)?)
    }
    fn save(&self, state: &State) -> Result<()> {
        atomic(&self.root.join("state.json"), &serde_json::to_vec(state)?)
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
        _ => Err("Workspace unavailable; accepted history remains queued in Git for retry".into()),
    }
}
// Verify every reachable tree, not just today's tip: a bundle must not silently
// carry an old chat/archive/project/credential blob from a repurposed repository.
fn scoped_history(store: &Store, revision: &str) -> Result<()> {
    if !fs::symlink_metadata(store.home.join(".git/shallow"))
        .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
        return Err("sync requires complete reachable history, not a shallow repository".into());
    }
    // Scope facts only: never acceptance evidence. A version change invalidates
    // every entry. Unsafe, malformed or oversized caches fall back to real trees.
    let cache_path = store.root.join("permitted-trees-v1.json");
    let cached = (|| -> Result<BTreeSet<String>> {
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&cache_path)?;
        let meta = file.metadata()?;
        if !meta.is_file()
            || meta.nlink() != 1
            || meta.uid() != unsafe { libc::geteuid() }
            || meta.mode() & 0o777 != 0o600
            || meta.len() > 1024 * 1024
        {
            return Err("unsafe scope cache".into());
        }
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
        let trees: BTreeSet<String> = serde_json::from_slice(&bytes)?;
        if trees.len() > 8192 || trees.iter().any(|tree| oid(tree).is_err()) {
            return Err("malformed scope cache".into());
        }
        Ok(trees)
    })()
    .unwrap_or_default();
    let mut permitted = cached.clone();
    // One bounded native walk, not rev-list plus one subprocess per commit.
    // Deduplicate identical root trees before inspecting actual paths/modes.
    let inventory = git(
        &store.home,
        &["rev-list", "--format=%T", "--no-commit-header", revision],
    )?;
    let trees: BTreeSet<_> = inventory.lines().collect();
    for tree in trees {
        oid(tree)?;
        if cached.contains(tree) {
            continue;
        }
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
                ["nodes", id, "children", ".keep"] => regular && uuid(id),
                ["nodes", id, "children", child] => link && uuid(id) && uuid(child),
                ["trees", id] => link && uuid(id),
                _ => false,
            };
            if !allowed {
                return Err("sync scope excludes non-knowledge history or session attachments; use a dedicated selected knowledge home (no data removed)".into());
            }
        }
        if permitted.len() < 8192 {
            permitted.insert(tree.to_owned());
        }
    }
    if permitted != cached {
        atomic(&cache_path, &serde_json::to_vec(&permitted)?)?;
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
) -> Result<()> {
    let commit = head(&store.home)?;
    if remote == Some(&commit) {
        if state.uploaded.as_deref() != Some(&commit) {
            state.uploaded = Some(commit);
            store.save(state)?;
        }
        return Ok(());
    }
    if remote.is_some() && state.uploaded.as_deref() == Some(&commit) {
        return Ok(());
    }
    scoped_history(store, &commit)?;
    let bundle = tempfile::NamedTempFile::new_in(&store.root)?;
    let path = bundle.path().to_str().ok_or("non-UTF8 bundle")?;
    let exclusion = remote
        .filter(|r| *r != commit && ancestor(&store.home, r, &commit))
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
        return Err("knowledge bundle exceeds 8MiB MVP limit; local history retained".into());
    }
    let body =
        json!({"commit":commit,"base":remote,"bundle":STANDARD.encode(fs::read(bundle.path())?)});
    let response = exchange.request(&format!("/sync/v1/devices/{}", state.device), Some(&body))?;
    if response["incoming"] != commit {
        return Err(
            "service did not acknowledge exact incoming commit; retain queued history".into(),
        );
    }
    state.uploaded = Some(commit);
    state.remote_pending = response["pending"].clone();
    store.save(state)
}
fn receive(
    store: &Store,
    state: &mut State,
    policy: &Policy,
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
    let receipt = local::submit(
        policy,
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
fn cycle(store: &Store, state: &mut State, policy: &Policy) -> Result<Value> {
    if !state.enabled {
        return Err("sync disabled; local knowledge unchanged".into());
    }
    // Authenticate ownership BEFORE local journal recovery, upload or import.
    let mut exchange = Exchange::open(state)?;
    let remote = exchange.request("/sync/v1/state", None)?;
    if matches!(state.binding, Binding::Unbound) {
        state.binding = Binding::Bound {
            home_id: exchange
                .home_id
                .clone()
                .ok_or("missing verified sync home")?,
        };
        store.save(state)?; // Durable binding precedes the very first transfer.
    }
    // Complete a publication interrupted before its clean reader advance.
    if git(&store.home, &["rev-parse", "HEAD"])? != head(&store.home)? {
        local::advance(policy, &store.home)?;
    }
    // Recover each exact mechanical journal, never replay a known conflict.
    let accepted = head(&store.home)?;
    state.retain_repairs(|p| !ancestor(&store.home, &p.incoming, &accepted));
    store.save(state)?;
    let interrupted: Vec<_> = state
        .repairs()
        .filter(|p| ["received", "home_or_publication_busy"].contains(&p.check.as_str()))
        .map(|p| (p.incoming.clone(), p.source_device.clone()))
        .collect();
    for (incoming, source) in interrupted {
        receive(store, state, policy, &incoming, source)?;
    }
    let remote_sha = remote["accepted"].as_str().map(oid).transpose()?;
    upload(store, state, &mut exchange, remote_sha)?;
    let have = head(&store.home)?;
    let response = exchange.request(&format!("/sync/v1/objects?have={have}"), None)?;
    state.remote_pending = response["pending"].clone();
    store.save(state)?;
    if let Some(incoming) = response["accepted"].as_str() {
        oid(incoming)?;
        import_bundle(store, &response, incoming)?;
        scoped_history(store, incoming)?;
        receive(store, state, policy, incoming, None)?;
    }
    // Unaccepted contributions from another device are real received work too.
    // Retain distinct unresolved repairs; ancestry and exact OIDs coalesce retries.
    let pending_records = state.remote_pending.as_array().cloned().unwrap_or_default();
    for pending in pending_records {
        let Some(incoming) = pending["incoming"].as_str() else {
            continue;
        };
        oid(incoming)?;
        if ancestor(&store.home, incoming, &head(&store.home)?) {
            continue;
        }
        if state.repairs().any(|p| p.incoming == incoming) {
            continue;
        }
        let response = exchange.request(
            &format!(
                "/sync/v1/objects?incoming={incoming}&have={}",
                head(&store.home)?
            ),
            None,
        )?;
        if response["incoming"] != incoming {
            return Err("service did not identify requested pending history".into());
        }
        import_bundle(store, &response, incoming)?;
        scoped_history(store, incoming)?;
        receive(
            store,
            state,
            policy,
            incoming,
            pending["device"].as_str().map(str::to_owned),
        )?;
    }
    Ok(status(store, state)?)
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
        json!({"enabled":state.enabled,"device":state.device,"binding":state.binding,"accepted":accepted,"uploaded":state.uploaded,"reconciliation":pending,"additional_reconciliations":reconciliations,"remote_pending":state.remote_pending}),
    )
}
/// No network or credentials on ordinary instructions. Only retained received work.
pub(super) fn instructions(home: &Path) -> Result<Vec<Value>> {
    if !home.join(".git/sifttext-sync/state.json").try_exists()? {
        return Ok(Vec::new());
    }
    let store = Store::open(home, false, false)?;
    let state = store.load()?;
    if state.repairs().next().is_none() {
        return Ok(Vec::new());
    }
    let result = status(&store, &state)?;
    let mut repairs = Vec::new();
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
pub(super) fn run(policy: &Policy, args: &[String]) -> Result<Value> {
    let [_, operation, home] = args else {
        return Err("usage: sifttext sync enable|disable|status|once|run HOME".into());
    };
    if !["enable", "disable", "status", "once", "run"].contains(&operation.as_str()) {
        return Err("unknown sync operation".into());
    }
    if operation == "status" && fs::symlink_metadata(Path::new(home).join(".git/sifttext-sync"))
        .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
        let home = fs::canonicalize(home)?;
        if !home.join(".git").symlink_metadata()?.is_dir()
            || git(&home, &["rev-parse", "--show-toplevel"])?
                != home.to_str().ok_or("non-UTF8 home")?
        {
            return Err("sync requires an independent initialized knowledge home".into());
        }
        return Ok(json!({"enabled":false,"status":"unconfigured"}));
    }
    if operation == "run" {
        // One cycle lock only: instructions/status stay local and atomic snapshots;
        // enable/disable and explicit once can run between bounded transfer cycles.
        loop {
            {
                let store = Store::open(Path::new(home), false, true)?;
                let mut state = store.load()?;
                if !state.enabled {
                    return status(&store, &state);
                }
                if let Err(error) = cycle(&store, &mut state, policy) {
                    eprintln!("Knowledge sync paused: {error}");
                }
            }
            thread::sleep(Duration::from_secs(15));
        }
    }
    let store = Store::open(
        Path::new(home),
        operation == "enable",
        operation != "status",
    )?;
    if operation == "enable" {
        // Enrollment is mechanical, not submission or derived rehydration. Only
        // advance when actually behind; unchanged HEAD needs no full index pass.
        enrollment_home(&store.home)?;
        if git(&store.home, &["rev-parse", "HEAD"])? != head(&store.home)? {
            local::advance(policy, &store.home)?;
            enrollment_home(&store.home)?;
        }
        scoped_history(&store, &head(&store.home)?)?;
        let mut state = if !fs::symlink_metadata(store.root.join("state.json"))
            .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
            store.load()?
        } else {
            let mut random = [0u8; 16];
            fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
            State {
                enabled: true,
                endpoint: endpoint()?,
                device: random.iter().map(|b| format!("{b:02x}")).collect(),
                binding: Binding::Unbound,
                uploaded: None,
                pending: None,
                additional_pending: Vec::new(),
                remote_pending: Value::Null,
            }
        };
        if state.endpoint != endpoint()? {
            return Err("cannot change an existing sync service; preserve this home and explicitly enroll a new clone".into());
        }
        state.enabled = true;
        store.save(&state)?;
        let mut output = status(&store, &state)?;
        output["runner_installation"] = json!("not_performed");
        output["runner_arguments"] = json!(["sync", "run", store.home]);
        return Ok(output);
    }
    if operation == "status" && fs::symlink_metadata(store.root.join("state.json"))
        .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
        return Ok(json!({"enabled":false,"status":"unconfigured"}));
    }
    let mut state = store.load()?;
    match operation.as_str() {
        "disable" => {
            state.enabled = false;
            store.save(&state)?;
            status(&store, &state)
        }
        "status" => status(&store, &state),
        "once" => cycle(&store, &mut state, policy),
        _ => unreachable!(),
    }
}
