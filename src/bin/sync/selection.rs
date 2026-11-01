//! Consent and current-state publication. The publication repository never shares
//! objects/alternates with the full knowledge home.
use super::*;
use sifttext_file_graph::{parse_markdown, uuid};
use std::collections::BTreeMap;

pub(super) const IGNORE: &str = "/.sifttext/\n/nodes/*/node.meta.json\n/nodes/*/outline*.lean\n/nodes/*/outline*.lean.meta.json\n";

use sifttext_file_graph::tree_selection::{Action, includes};
pub(super) use sifttext_file_graph::tree_selection::Rule;
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Settings {
    pub mode: String,
    default: Action,
    rules: Vec<Rule>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: "unanswered".into(),
            default: Action::Include,
            rules: Vec::new(),
        }
    }
}
fn config(home: &Path) -> Result<Value> {
    if let Ok(meta) = fs::symlink_metadata(home.join(".sifttext")) {
        if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o022 != 0 {
            return Err("unsafe local configuration directory".into());
        }
    }
    let path = home.join(".sifttext/config.json");
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.into()),
        Ok(meta) => {
            if !meta.is_file() || meta.nlink() != 1 || meta.len() > 1024 * 1024 {
                return Err("unsafe local configuration".into());
            }
            let mut file = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(path)?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            let value: Value = serde_json::from_slice(&bytes)?;
            if !value.is_object() {
                return Err("local configuration must be an object".into());
            }
            Ok(value)
        }
    }
}
pub(super) fn settings(home: &Path) -> Result<Settings> {
    let file = config(home)?;
    let settings: Settings = file
        .get("sync")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_default();
    validate(&settings)?;
    Ok(settings)
}
fn validate(settings: &Settings) -> Result<()> {
    if !["unanswered", "declined", "disabled", "enabled"].contains(&settings.mode.as_str()) {
        return Err("sync.mode must be unanswered, declined, disabled or enabled".into());
    }
    let mut rules: BTreeMap<&str, Action> = BTreeMap::new();
    for rule in &settings.rules {
        if !uuid(&rule.node) {
            return Err("sync rule requires a canonical node UUID".into());
        }
        if rules
            .insert(&rule.node, rule.action)
            .is_some_and(|old| old != rule.action)
        {
            return Err("opposing sync rules for one node; no transfer permitted".into());
        }
    }
    Ok(())
}
pub(super) fn set_policy(home: &Path, input: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(input)?;
    if !meta.is_file() || meta.len() > 1024 * 1024 {
        return Err("sharing policy must be a regular bounded JSON file".into());
    }
    let mut value: Value = serde_json::from_slice(&fs::read(input)?)?;
    let object = value
        .as_object_mut()
        .ok_or("sharing policy must be an object")?;
    if object.contains_key("mode") {
        return Err("policy cannot change consent; use enable, disable or decline".into());
    }
    object.insert("mode".into(), json!(settings(home)?.mode));
    let policy: Settings = serde_json::from_value(value)?;
    validate(&policy)?;
    save_settings(home, &policy)
}
pub(super) fn save_settings(home: &Path, settings: &Settings) -> Result<()> {
    validate(settings)?;
    let mut file = config(home)?;
    let value = serde_json::to_value(settings)?;
    if file.get("sync") == Some(&value) {
        return Ok(());
    }
    file["sync"] = value;
    let parent = home.join(".sifttext");
    if !parent.try_exists()? {
        fs::DirBuilder::new().mode(0o700).create(&parent)?;
    }
    if !fs::symlink_metadata(&parent)?.is_dir() {
        return Err("unsafe local configuration directory".into());
    }
    atomic(
        &parent.join("config.json"),
        &serde_json::to_vec_pretty(&file)?,
    )
}

#[derive(Clone, PartialEq, Eq)]
struct File {
    mode: String,
    bytes: Vec<u8>,
}
#[derive(Clone, PartialEq, Eq)]
struct Snapshot {
    files: BTreeMap<String, File>,
}
impl Snapshot {
    fn read(repo: &Path, revision: &str) -> Result<Self> {
        oid(revision)?;
        let inventory = git(repo, &["ls-tree", "-r", revision])?;
        let mut objects = Vec::new();
        for line in inventory.lines() {
            let (meta, path) = line.split_once('\t').ok_or("invalid knowledge tree")?;
            let parts: Vec<_> = meta.split_whitespace().collect();
            if parts.len() != 3 || parts[1] != "blob" {
                return Err("unsupported knowledge object".into());
            }
            // Only canonical authored paths are ever materialized from received data.
            let p: Vec<_> = path.split('/').collect();
            let valid = match p.as_slice() {
                [".gitignore"] | ["nodes", ".keep"] | ["trees", ".keep"] => parts[0] == "100644",
                [
                    "nodes",
                    id,
                    "node.md" | "children.order" | "session-sources.json",
                ] => uuid(id) && parts[0] == "100644",
                ["nodes", id, "children", ".keep"] => uuid(id) && parts[0] == "100644",
                ["nodes", id, "children", child] => uuid(id) && uuid(child) && parts[0] == "120000",
                ["trees", id] => uuid(id) && parts[0] == "120000",
                _ => false,
            };
            if !valid {
                return Err("unsupported authored path; histories retained".into());
            }
            oid(parts[2])?;
            objects.push((path.to_owned(), parts[0].to_owned(), parts[2].to_owned()));
        }
        use std::io::{BufRead, BufReader};
        use std::os::unix::process::CommandExt;
        let mut child = git_command(repo, &["cat-file", "--batch"])
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut input = child.stdin.take().ok_or("missing Git stdin")?;
        let mut output = BufReader::new(child.stdout.take().ok_or("missing Git stdout")?);
        let (tx, rx) = std::sync::mpsc::channel();
        // Drain each small request group before writing another, so neither pipe
        // can fill while waiting for the other. Keep one helper for the snapshot.
        let reader = thread::spawn(move || {
            let result = (|| -> Result<BTreeMap<String, File>> {
                let mut files = BTreeMap::new();
                for group in objects.chunks(32) {
                    let request = group.iter().map(|(_, _, sha)| format!("{sha}\n"))
                        .collect::<String>();
                    input.write_all(request.as_bytes())?;
                    input.flush()?;
                    for (path, mode, sha) in group {
                        let mut header = Vec::new();
                        output.by_ref().take(256).read_until(b'\n', &mut header)?;
                        if header.last() != Some(&b'\n') || header.len() >= 256 {
                            return Err("oversized/truncated Git object header".into());
                        }
                        let header = std::str::from_utf8(&header)?;
                        let h: Vec<_> = header.split_whitespace().collect();
                        if h.len() != 3 || h[0] != sha || h[1] != "blob" {
                            return Err("invalid Git object response".into());
                        }
                        let len: usize = h[2].parse()?;
                        if len > 1024 * 1024 {
                            return Err("oversized knowledge blob".into());
                        }
                        let mut bytes = vec![0; len];
                        output.read_exact(&mut bytes)?;
                        let mut end = [0];
                        output.read_exact(&mut end)?;
                        if end != [b'\n'] {
                            return Err("malformed Git object framing".into());
                        }
                        files.insert(path.clone(), File { mode: mode.clone(), bytes });
                    }
                }
                drop(input);
                let mut extra = [0];
                if output.read(&mut extra)? != 0 {
                    return Err("unexpected Git object data".into());
                }
                Ok(files)
            })().map_err(|error| error.to_string());
            let _ = tx.send(result);
        });
        let result = rx.recv_timeout(Duration::from_secs(30))
            .map_err(|_| "Git snapshot reader timeout or interruption".to_owned())
            .and_then(|result| result);
        if result.is_err() {
            unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL); }
        }
        let status = child.wait();
        let joined = reader.join();
        let files = result?;
        if joined.is_err() || !status?.success() {
            return Err("Git snapshot reader failed; histories retained".into());
        }
        let snapshot = Self { files };
        snapshot.parents()?;
        Ok(snapshot)
    }
    fn nodes(&self) -> Result<BTreeSet<String>> {
        let mut result = BTreeSet::new();
        for (path, file) in &self.files {
            if path.ends_with("/node.md") {
                let id = path.split('/').nth(1).ok_or("missing node ID")?;
                if parse_markdown(std::str::from_utf8(&file.bytes)?)?.id != id {
                    return Err("node identity mismatch".into());
                }
                result.insert(id.to_owned());
            }
        }
        Ok(result)
    }
    fn children(&self, id: &str) -> Result<Vec<String>> {
        let Some(file) = self.files.get(&format!("nodes/{id}/children.order")) else {
            let own = self.files.get(&format!("nodes/{id}/node.md")).ok_or("missing parent")?;
            if sifttext_file_graph::ordering::position(std::str::from_utf8(&own.bytes)?)?.is_none() {
                return Err("missing children.order or per-node position".into());
            }
            let prefix = format!("nodes/{id}/children/");
            let members = self.files.keys().filter_map(|path| path.strip_prefix(&prefix))
                .filter(|child| uuid(child)).map(str::to_owned);
            return sifttext_file_graph::ordering::sort(members, |child| {
                let file = self.files.get(&format!("nodes/{child}/node.md")).ok_or("missing child")?;
                sifttext_file_graph::ordering::position(std::str::from_utf8(&file.bytes)?)
            });
        };
        let bytes = file.bytes.as_slice();
        let children: Vec<_> = std::str::from_utf8(bytes)?
            .lines()
            .map(str::to_owned)
            .collect();
        if children.iter().any(|id| !uuid(id)) {
            return Err("invalid child identity".into());
        }
        Ok(children)
    }
    fn parents(&self) -> Result<BTreeMap<String, String>> {
        let nodes = self.nodes()?;
        let mut parents = BTreeMap::new();
        for id in &nodes {
            for child in self.children(id)? {
                if !nodes.contains(&child) || parents.insert(child.clone(), id.clone()).is_some() {
                    return Err("invalid knowledge forest".into());
                }
                if self
                    .files
                    .get(&format!("nodes/{id}/children/{child}"))
                    .is_none_or(|f| f.bytes != format!("../../{child}").as_bytes())
                {
                    return Err("invalid child link".into());
                }
            }
        }
        for id in &nodes {
            let mut seen = BTreeSet::new();
            let mut cursor = id;
            while let Some(parent) = parents.get(cursor) {
                if !seen.insert(cursor) {
                    return Err("cyclic knowledge forest".into());
                }
                cursor = parent;
            }
            if !parents.contains_key(id)
                && self
                    .files
                    .get(&format!("trees/{id}"))
                    .is_none_or(|f| f.bytes != format!("../nodes/{id}").as_bytes())
            {
                return Err("missing knowledge root".into());
            }
        }
        Ok(parents)
    }
    fn put(&mut self, path: String, mode: &str, bytes: impl Into<Vec<u8>>) {
        self.files.insert(
            path,
            File {
                mode: mode.into(),
                bytes: bytes.into(),
            },
        );
    }
    fn topology(
        &mut self,
        nodes: &BTreeSet<String>,
        children: &BTreeMap<String, Vec<String>>,
        parents: &BTreeMap<String, String>,
    ) -> Result<()> {
        self.files.retain(|p, _| {
            !(p.starts_with("trees/") || p.ends_with("/children.order") || p.contains("/children/"))
        });
        self.put("nodes/.keep".into(), "100644", Vec::new());
        self.put("trees/.keep".into(), "100644", Vec::new());
        for id in nodes {
            let order = children.get(id).cloned().unwrap_or_default();
            let node = self.files.get(&format!("nodes/{id}/node.md")).ok_or("missing topology node")?;
            if sifttext_file_graph::ordering::position(std::str::from_utf8(&node.bytes)?)?.is_none() {
                self.put(
                    format!("nodes/{id}/children.order"), "100644",
                    order.iter().map(|id| format!("{id}\n")).collect::<String>().into_bytes(),
                );
            }
            self.put(format!("nodes/{id}/children/.keep"), "100644", Vec::new());
            for child in order {
                self.put(
                    format!("nodes/{id}/children/{child}"),
                    "120000",
                    format!("../../{child}").into_bytes(),
                );
            }
            if !parents.contains_key(id) {
                self.put(
                    format!("trees/{id}"),
                    "120000",
                    format!("../nodes/{id}").into_bytes(),
                );
            }
        }
        Ok(())
    }
    fn selected(&self, policy: &Settings) -> Result<BTreeSet<String>> {
        let parents = self.parents()?;
        Ok(self
            .nodes()?
            .into_iter()
            .filter(|id| {
                let ancestors = std::iter::successors(parents.get(id), |parent| parents.get(*parent));
                includes(&policy.rules, policy.default, id, ancestors.map(String::as_str))
            })
            .collect())
    }
    fn project(&self, policy: &Settings) -> Result<Self> {
        let selected = self.selected(policy)?;
        let parents = self
            .parents()?
            .into_iter()
            .filter(|(child, parent)| selected.contains(child) && selected.contains(parent))
            .collect();
        let mut children = BTreeMap::new();
        let mut result = Self {
            files: BTreeMap::new(),
        };
        result.put(".gitignore".into(), "100644", IGNORE.as_bytes().to_vec());
        for id in &selected {
            let path = format!("nodes/{id}/node.md");
            let text = std::str::from_utf8(&self.files[&path].bytes)?;
            let refs = parse_markdown(text)?
                .see_also
                .into_iter()
                .filter(|r| selected.contains(&r.target))
                .collect::<Vec<_>>();
            result.put(path, "100644", references(text, &refs)?.into_bytes());
            children.insert(
                id.clone(),
                self.children(id)?
                    .into_iter()
                    .filter(|id| selected.contains(id))
                    .collect(),
            );
        }
        result.topology(&selected, &children, &parents)?;
        Ok(result)
    }
    fn write(&self, repo: &Path, parent: Option<&str>) -> Result<String> {
        // Native Git builds one isolated tree. Never fetch full-home objects here.
        // fast-import's stream reset does not permit a non-fast-forward ref update
        // after an unacknowledged attempt. Retire only the disposable build ref;
        // publication main, mappings and every received/unacknowledged object stay.
        git(
            repo,
            &["update-ref", "--no-deref", "-d", "refs/sifttext-sync/build"],
        )?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let message = "Publish selected current knowledge";
        let mut stream = format!("feature done\nreset refs/sifttext-sync/build\n\ncommit refs/sifttext-sync/build\ncommitter SiftText Sync <sync@sifttext.invalid> {now} +0000\ndata {}\n{message}\n", message.len()).into_bytes();
        if let Some(parent) = parent {
            stream.extend_from_slice(format!("from {}\n", oid(parent)?).as_bytes());
        }
        stream.extend_from_slice(b"deleteall\n");
        for (path, file) in &self.files {
            stream.extend_from_slice(
                format!(
                    "M {} inline {}\ndata {}\n",
                    file.mode,
                    path,
                    file.bytes.len()
                )
                .as_bytes(),
            );
            stream.extend_from_slice(&file.bytes);
            stream.push(b'\n');
        }
        stream.extend_from_slice(b"\ndone\n");
        git_input(repo, &["fast-import", "--quiet"], &stream)?;
        let commit = git(repo, &["rev-parse", "refs/sifttext-sync/build"])?;
        oid(&commit)?;
        if let Some(parent) = parent {
            if git(repo, &["rev-parse", &format!("{parent}^{{tree}}")])?
                == git(repo, &["rev-parse", &format!("{commit}^{{tree}}")])?
            {
                return Ok(parent.into());
            }
        }
        Ok(commit)
    }
}
fn identity(mut command: Command) -> Command {
    command
        .env("GIT_AUTHOR_NAME", "SiftText Sync")
        .env("GIT_AUTHOR_EMAIL", "sync@sifttext.invalid")
        .env("GIT_COMMITTER_NAME", "SiftText Sync")
        .env("GIT_COMMITTER_EMAIL", "sync@sifttext.invalid");
    command
}
fn references(text: &str, refs: &[sifttext_file_graph::Reference]) -> Result<String> {
    if parse_markdown(text)?.see_also == refs {
        return Ok(text.into());
    }
    let mut result = String::new();
    let mut fences = 0;
    let mut replaced = false;
    for line in text.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            fences += 1;
        }
        if fences == 1
            && line
                .split_once(':')
                .is_some_and(|(key, _)| key.trim() == "see_also")
        {
            result.push_str(&format!("see_also: {}\n", serde_json::to_string(refs)?));
            replaced = true;
        } else {
            result.push_str(line);
        }
    }
    if !replaced {
        return Err("missing structured references".into());
    }
    Ok(result)
}

#[derive(Serialize, Deserialize)]
pub(super) struct Mapping {
    pub local: String,
    pub published: String,
}
#[derive(Default, Serialize, Deserialize)]
pub(super) struct Projection {
    pub local: String,
    pub policy: String,
    pub published: String,
    #[serde(default)]
    pub received: Option<String>,
    #[serde(default)]
    pub blocked: Option<Value>,
}
pub(super) fn repository(store: &Store) -> Result<Store> {
    let home = store.root.join("publication");
    if !home.try_exists()? {
        fs::DirBuilder::new().mode(0o700).create(&home)?;
        git(&home, &["init", "-q"])?;
        git(&home, &["config", "core.fsync", "committed"])?;
    }
    if !fs::symlink_metadata(&home)?.is_dir()
        || !fs::symlink_metadata(home.join(".git"))?.is_dir()
        || home.join(".git/objects/info/alternates").try_exists()?
    {
        return Err("unsafe publication repository".into());
    }
    Ok(Store {
        home,
        root: store.root.clone(),
        persisted: RefCell::new(None),
        _lock: None,
    })
}
fn mapping(store: &Store, published: &str, local: &str) -> Result<()> {
    let dir = store.root.join("mappings");
    if !dir.try_exists()? {
        fs::DirBuilder::new().mode(0o700).create(&dir)?;
    }
    let path = dir.join(format!("{}.json", oid(published)?));
    if !path.try_exists()? {
        atomic(
            &path,
            &serde_json::to_vec(&Mapping {
                local: local.into(),
                published: published.into(),
            })?,
        )?;
    }
    Ok(())
}
pub(super) fn project(
    store: &Store,
    publication: &Store,
    state: &mut State,
    policy: &Settings,
    parent: Option<&str>,
) -> Result<String> {
    let local = head(&store.home)?;
    let policy_id = sifttext_file_graph::sha256(&serde_json::to_vec(policy)?);
    if state.projection.as_ref().is_some_and(|p| {
        p.local == local && p.policy == policy_id && parent == Some(p.published.as_str())
    }) {
        return Ok(state.projection.as_ref().unwrap().published.clone());
    }
    let snapshot = Snapshot::read(&store.home, &local)?.project(policy)?;
    let published = snapshot.write(&publication.home, parent)?;
    // The OS runner and once share the same lock. A manual config edit racing a
    // projection cannot broaden that upload under an obsolete policy.
    if serde_json::to_value(settings(&store.home)?)? != serde_json::to_value(policy)?
        || head(&store.home)? != local
    {
        return Err("knowledge or sharing policy changed during projection; retry".into());
    }
    mapping(store, &published, &local)?;
    git(
        &publication.home,
        &["update-ref", "refs/heads/main", &published],
    )?;
    let received = state.projection.as_ref().and_then(|p| p.received.clone());
    state.projection = Some(Projection {
        local,
        policy: policy_id,
        published: published.clone(),
        received,
        blocked: None,
    });
    store.save(state)?;
    Ok(published)
}

fn inverse(
    full: &Snapshot,
    baseline: &Snapshot,
    remote: &Snapshot,
    policy: &Settings,
) -> Result<Snapshot> {
    let original = full.nodes()?;
    let visible = baseline.nodes()?;
    let incoming = remote.nodes()?;
    let selected = full.selected(policy)?;
    let full_parents = full.parents()?;
    let base_parents = baseline.parents()?;
    let remote_parents = remote.parents()?;
    let mut result = full.clone();
    for id in visible.union(&incoming) {
        let path = format!("nodes/{id}/node.md");
        let before = baseline.files.get(&path);
        let after = remote.files.get(&path);
        if before.map(|f| &f.bytes) == after.map(|f| &f.bytes) {
            continue;
        }
        if original.contains(id) && (!visible.contains(id) || !selected.contains(id)) {
            return Err("hosted edit targets a currently local-only identity".into());
        }
        if let Some(after) = after {
            let mut text = std::str::from_utf8(&after.bytes)?.to_owned();
            let mut refs = parse_markdown(&text)?.see_also;
            // Validate received targets before restoring omitted local references.
            // A sender cannot introduce a reference to a local-only identity.
            if refs.iter().any(|r| !incoming.contains(&r.target)) {
                return Err("hosted reference targets knowledge outside its publication".into());
            }
            if let Some(local) = full.files.get(&path) {
                let omitted = parse_markdown(std::str::from_utf8(&local.bytes)?)?
                    .see_also
                    .into_iter()
                    .filter(|r| !visible.contains(&r.target));
                for r in omitted {
                    if !refs.iter().any(|other| other.target == r.target) {
                        refs.push(r);
                    }
                }
            }
            text = references(&text, &refs)?;
            result.put(path, "100644", text.into_bytes());
        } else {
            if full
                .children(id)?
                .iter()
                .any(|child| !visible.contains(child))
            {
                return Err("hosted deletion has local-only children; rehome locally".into());
            }
            if full
                .files
                .contains_key(&format!("nodes/{id}/session-sources.json"))
            {
                return Err(
                    "hosted deletion has retained local session provenance; reconcile locally"
                        .into(),
                );
            }
            result
                .files
                .retain(|path, _| !path.starts_with(&format!("nodes/{id}/")));
        }
    }
    let nodes = result.nodes()?;
    let mut parents = full_parents.clone();
    parents.retain(|child, parent| nodes.contains(child) && nodes.contains(parent));
    for id in &incoming {
        if original.contains(id) && !visible.contains(id) {
            return Err("hosted identity collides with local-only knowledge".into());
        }
        if base_parents.get(id) != remote_parents.get(id) || !original.contains(id) {
            if original.contains(id) && !selected.contains(id) {
                return Err("hosted move targets currently local-only knowledge".into());
            }
            parents.remove(id);
            if let Some(parent) = remote_parents.get(id) {
                parents.insert(id.clone(), parent.clone());
            }
        }
        // Promoted roots that remain roots keep their full local hidden parent.
    }
    let mut children = BTreeMap::new();
    for id in &nodes {
        if !incoming.contains(id) {
            // A hidden parent's child order is not present in the projected view.
            // Promoted roots are unchanged placements, not appended children.
            children.insert(
                id.clone(),
                full.children(id)?
                    .into_iter()
                    .filter(|child| nodes.contains(child) && parents.get(child) == Some(id))
                    .collect(),
            );
            continue;
        }
        let desired: Vec<_> = remote
            .children(id)?
            .into_iter()
            .filter(|child| parents.get(child) == Some(id))
            .collect();
        let mut visible_order = desired.iter();
        let mut order = Vec::new();
        for child in full.children(id)? {
            if !nodes.contains(&child) {
                continue;
            }
            if visible.contains(&child) {
                if let Some(next) = visible_order.next() {
                    order.push(next.clone());
                }
            } else if parents.get(&child) == Some(id) {
                order.push(child);
            }
        }
        order.extend(visible_order.cloned());
        // Unchanged promoted roots can remain children of a private parent and
        // therefore are absent from the remote parent's order.
        for child in full.children(id)? {
            if nodes.contains(&child) && parents.get(&child) == Some(id) && !order.contains(&child)
            {
                order.push(child);
            }
        }
        children.insert(id.clone(), order);
    }
    result.topology(&nodes, &children, &parents)?;
    let now = result.selected(policy)?;
    if original
        .intersection(&nodes)
        .any(|id| selected.contains(id) != now.contains(id))
        || incoming.difference(&original).any(|id| !now.contains(id))
    {
        return Err("hosted topology would change local sharing permission; review locally".into());
    }
    Ok(result)
}

pub(super) fn import(
    store: &Store,
    publication: &Store,
    state: &mut State,
    runtime: &Path,
    incoming: &str,
) -> Result<()> {
    if state
        .projection
        .as_ref()
        .and_then(|p| p.received.as_deref())
        == Some(incoming)
    {
        return Ok(());
    }
    let policy_id = sifttext_file_graph::sha256(&serde_json::to_vec(&settings(&store.home)?)?);
    let local_tip = head(&store.home)?;
    if state
        .projection
        .as_ref()
        .and_then(|p| p.blocked.as_ref())
        .is_some_and(|blocked| {
            blocked["incoming"] == incoming
                && blocked["local_tip"] == local_tip
                && blocked["policy_id"] == policy_id
                // The removed blanket guard must not strand its old notice.
                && blocked["check"].as_str() != Some("concurrent local and hosted topology needs local reconciliation before sharing can change")
        })
    {
        return Err("hosted contribution remains in local reconciliation; follow retained guide before retrying unchanged work".into());
    }
    let history = git(&publication.home, &["rev-list", "--topo-order", incoming])?;
    let mut base = None;
    for sha in history.lines() {
        oid(sha)?;
        let path = store.root.join(format!("mappings/{sha}.json"));
        if path.try_exists()? {
            base = Some(serde_json::from_slice::<Mapping>(&fs::read(path)?)?);
            break;
        }
    }
    let Some(base) = base else {
        // A new device may have the same current selection without the private
        // correspondence journal. Exact current bytes establish that baseline;
        // a differing unrelated graph still requires explicit reconciliation.
        let local = head(&store.home)?;
        if Snapshot::read(&store.home, &local)?.project(&settings(&store.home)?)?
            == Snapshot::read(&publication.home, incoming)?
        {
            mapping(store, incoming, &local)?;
            return Ok(());
        }
        // An already-hosted legacy history is allowed local custody, not permission
        // to upload full local ancestors. Reuse native import only for that history.
        git(
            &store.home,
            &[
                "fetch",
                "--no-tags",
                publication
                    .home
                    .to_str()
                    .ok_or("non-UTF8 publication path")?,
                &format!(
                    "refs/sifttext-sync/objects/{incoming}:refs/sifttext-sync/objects/{incoming}"
                ),
            ],
        )?;
        scoped_history(store, incoming, HistoryScope::Received)?;
        receive(store, state, runtime, incoming, None)?;
        if !ancestor(&store.home, incoming, &head(&store.home)?) {
            return Err("legacy hosted contribution needs retained local reconciliation".into());
        }
        mapping(store, incoming, incoming)?;
        return Ok(());
    };
    oid(&base.local)?;
    oid(&base.published)?;
    if base.published == incoming {
        return Ok(());
    }
    let policy = settings(&store.home)?;
    let full = Snapshot::read(&store.home, &base.local)?;
    let baseline = Snapshot::read(&publication.home, &base.published)?;
    let remote = Snapshot::read(&publication.home, incoming)?;
    let result = inverse(&full, &baseline, &remote, &policy)?;
    let current = Snapshot::read(&store.home, &head(&store.home)?)?;
    let original_ids = full.nodes()?;
    let current_ids = current.nodes()?;
    if remote
        .nodes()?
        .difference(&original_ids)
        .any(|id| current_ids.contains(id))
    {
        return Err("hosted new identity collides with concurrent local knowledge".into());
    }
    let full_nodes = full.nodes()?;
    let result_nodes = result.nodes()?;
    for id in full_nodes.difference(&result_nodes) {
        if store
            .home
            .join(format!("nodes/{id}/artifacts"))
            .try_exists()?
        {
            return Err("hosted deletion has local artifacts; preserve/rehome them locally".into());
        }
    }
    let worktree = store
        .root
        .join(format!("selected-repair-{}", &incoming[..16]));
    let journal = store.root.join(format!("translation-{incoming}.json"));
    let contribution = if journal.try_exists()? {
        let saved: Mapping = serde_json::from_slice(&fs::read(&journal)?)?;
        if saved.local != base.local {
            return Err("translation baseline changed; preserve retained repair".into());
        }
        saved.published
    } else {
        if worktree.try_exists()? {
            return Err("interrupted translation draft retained; inspect before retry".into());
        }
        git(
            &store.home,
            &[
                "worktree",
                "add",
                "-b",
                &format!("sifttext/selected-{}", &incoming[..16]),
                worktree.to_str().ok_or("non-UTF8 repair")?,
                &base.local,
            ],
        )?;
        for path in full
            .files
            .keys()
            .chain(result.files.keys())
            .collect::<BTreeSet<_>>()
        {
            let before = full.files.get(path);
            let after = result.files.get(path);
            if before.map(|f| (&f.mode, &f.bytes)) == after.map(|f| (&f.mode, &f.bytes)) {
                continue;
            }
            let target = worktree.join(path);
            if fs::symlink_metadata(&target).is_ok() {
                fs::remove_file(&target)?;
            }
            if let Some(file) = after {
                fs::create_dir_all(target.parent().ok_or("missing authored parent")?)?;
                if file.mode == "120000" {
                    std::os::unix::fs::symlink(std::str::from_utf8(&file.bytes)?, &target)?;
                } else {
                    fs::write(&target, &file.bytes)?;
                }
            }
            git(&worktree, &["add", "-A", "--", path])?;
        }
        let tree = git(&worktree, &["write-tree"])?;
        let contribution = String::from_utf8(command_input(
            identity(git_command(
                &worktree,
                &[
                    "commit-tree",
                    &tree,
                    "-p",
                    &base.local,
                    "-m",
                    "Import hosted selected knowledge",
                ],
            )),
            &[],
        )?)?
        .trim()
        .to_owned();
        oid(&contribution)?;
        git(&worktree, &["update-ref", "HEAD", &contribution])?;
        atomic(
            &journal,
            &serde_json::to_vec(&Mapping {
                local: base.local.clone(),
                published: contribution.clone(),
            })?,
        )?;
        contribution
    };
    let guide = store
        .root
        .join(format!("selected-repair-{}.md", &incoming[..16]));
    atomic(&guide, format!("# Reconcile hosted selected knowledge\n\nHosted history `{incoming}` is retained in `{}`. Full local baseline `{}`; translated contribution `{contribution}`. Private draft `{}`. Hidden nodes/references/placements were retained. Review meaning under the user's knowledge-write authority. Resolve and commit this draft, then use `sifttext submit '{}' '{}' COMMIT '{}'`. Never reset local main. Resume sync after local acceptance; the original exact native request remains recoverable.\n", publication.home.display(), base.local, worktree.display(), store.home.display(), worktree.display(), base.local).as_bytes())?;
    if let Some(p) = &mut state.projection {
        p.blocked = Some(
            json!({"incoming":incoming,"repair_worktree":worktree,"repair_guide":guide,"contribution":contribution,"local_tip":head(&store.home)?,"policy_id":policy_id}),
        );
    }
    store.save(state)?;
    let receipt = local::submit_received(
        &super::super::checked_policy(runtime)?,
        &checked_publication::Request {
            repo: store.home.clone(),
            accepted_ref: "refs/heads/main".into(),
            draft: Some(worktree),
            commit: Some(contribution.clone()),
            base: Some(base.local),
            explicit: BTreeSet::new(),
            pause_before_publish: None,
            pause_after_publish: None,
        },
    )?;
    if !["current", "saved", "recovered"]
        .iter()
        .any(|s| receipt["status"] == *s)
        || receipt["worktree_removed"] != true
        || !ancestor(&store.home, &contribution, &head(&store.home)?)
    {
        return Err(
            "hosted selected edits require retained local reconciliation; inspect sync status"
                .into(),
        );
    }
    // Map to the translated contribution, NOT the combined accepted tip: local
    // concurrent edits must remain concurrent to the next hosted delta too.
    mapping(store, incoming, &contribution)?;
    if let Some(p) = &mut state.projection {
        p.received = Some(incoming.into());
        p.blocked = None;
        p.local.clear();
    }
    store.save(state)
}
