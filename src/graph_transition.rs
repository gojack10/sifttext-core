//! Conditional graph preservation for exact, fully proven bases and body-only
//! edits. This does not validate authored bodies/status/tokens or publish anything.
use crate::{
    Entry, Graph, LIMIT, Policy, Result, Tokens, coherent_capture::Capture, encode, graph_words,
    invoke, require, sha256,
};
use std::{
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(test)]
#[path = "../tests/successor/graph_transition.rs"]
mod tests;

/// In-memory capability, not a serializable/user-asserted VALID label. Its only
/// constructors perform the full-law check or consume an exact successful
/// candidate-validation capture, always retaining the final identity barrier.
pub struct ProvenBase {
    capture: Capture,
    version: String,
    policy_identity: String,
}
impl ProvenBase {
    pub fn version(&self) -> &str {
        &self.version
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Preservation {
    /// Only graph preservation at the final capture barrier, not full acceptance.
    Preserved {
        base_version: String,
        candidate_version: String,
    },
    /// Caller must use the unchanged full-law/full-candidate path, never accept
    /// merely because this class is unsupported.
    FullLawRequired(&'static str),
}

/// Reuses the actual Bend preservation predicate. All imported source remains
/// covered by the candidate stamp; the executable is independently hashed and
/// checked before and after every invocation.
pub struct Transition<'a> {
    policy: &'a Policy,
    executable: PathBuf,
    executable_hash: String,
    policy_identity: String,
}
impl<'a> Transition<'a> {
    /// Load the source-build asset without invoking a compiler. The stamped
    /// identity is captured in memory, never taken from a later mutable manifest.
    pub fn checked(policy: &'a Policy) -> Result<Self> {
        policy.verify_stamp()?;
        let stamp = String::from_utf8(crate::read_regular(
            &policy.project.join("build/graph-transition.sha256"),
        )?)?;
        let (expected, path) = stamp
            .trim_end()
            .split_once("  ")
            .ok_or("malformed transition stamp")?;
        require(
            stamp.lines().count() == 1
                && path == "build/graph-transition"
                && expected.len() == 64
                && expected.bytes().all(|b| b.is_ascii_hexdigit()),
            "incorrect transition stamp",
        )?;
        let transition = Self {
            policy,
            executable: policy.project.join(path),
            executable_hash: expected.to_owned(),
            policy_identity: identity(policy)?,
        };
        transition.verify()?;
        Ok(transition)
    }

    /// Build/check-only API; normal publication uses `checked`.
    /// `directory` must be a new, exclusive location outside the source checkout.
    /// Compilation failures leave inspectable artifacts, never a trusted object.
    pub fn compile(policy: &'a Policy, directory: &Path) -> Result<Self> {
        policy.verify_stamp()?;
        let parent = fs::canonicalize(directory.parent().ok_or("missing build parent")?)?;
        require(
            !parent.starts_with(fs::canonicalize(&policy.project)?),
            "transition build outside source checkout",
        )?;
        fs::DirBuilder::new().mode(0o700).create(directory)?;
        let directory = fs::canonicalize(directory)?;
        let executable = directory.join("graph-transition");
        let policy_identity = identity(policy)?;
        let resolved = Command::new("sh")
            .arg(policy.project.join("scripts/bend-path.sh"))
            .output()?;
        require(
            resolved.status.success(),
            "verified Bend compiler unavailable",
        )?;
        let bend = PathBuf::from(std::str::from_utf8(&resolved.stdout)?.trim());
        require(
            bend.is_absolute(),
            "compiler resolver returned a relative path",
        )?;
        let version = Command::new(&bend).arg("version").output()?;
        require(
            version.status.success()
                && version.stdout == b"bend 2.0.27\n"
                && version.stderr.is_empty(),
            "unexpected Bend compiler",
        )?;
        let source = policy.project.join("laws/successor/GraphTransition.bend");
        for args in [
            vec!["--check-only"],
            vec![
                "-o",
                executable.to_str().ok_or("non-UTF-8 executable path")?,
            ],
        ] {
            let output = Command::new(&bend).arg(&source).args(args).output()?;
            fs::write(
                directory.join(if executable.exists() {
                    "compile.log"
                } else {
                    "check.log"
                }),
                [&output.stdout[..], &output.stderr[..]].concat(),
            )?;
            require(
                output.status.success(),
                "Bend transition compilation failed (see build directory)",
            )?;
        }
        policy.verify_stamp()?;
        require(
            identity(policy)? == policy_identity,
            "policy changed during compilation",
        )?;
        let executable_hash = sha256(&fs::read(&executable)?);
        Ok(Self {
            policy,
            executable,
            executable_hash,
            policy_identity,
        })
    }

    pub(crate) fn verify(&self) -> Result<()> {
        self.policy.verify_stamp()?;
        require(
            identity(self.policy)? == self.policy_identity,
            "transition policy identity changed",
        )?;
        require(
            fs::symlink_metadata(&self.executable)?.is_file()
                && sha256(&fs::read(&self.executable)?) == self.executable_hash,
            "transition executable changed",
        )
    }

    /// A preserved graph must belong to the validator's selected policy, not
    /// merely to a separately valid transition/base pair.
    pub(crate) fn check_policy(&self, policy: &Policy) -> Result<()> {
        require(
            identity(policy)? == self.policy_identity,
            "foreign graph transition policy",
        )
    }

    pub fn prove_base(&self, path: &Path) -> Result<ProvenBase> {
        self.verify()?;
        let capture = Capture::read(path, None)?;
        self.policy.validate(&capture.store)?;
        capture.unchanged_all()?;
        self.verify()?;
        let version = crate::candidate_validation::version(&capture.store);
        Ok(ProvenBase {
            capture,
            version,
            policy_identity: self.policy_identity.clone(),
        })
    }

    /// Consume the original successful validator capture rather than reread the
    /// base and recompute the same graph law. Transition/helper barriers remain.
    pub(crate) fn prove_validated(
        &self,
        validated: crate::candidate_validation::ValidatedCapture,
    ) -> Result<ProvenBase> {
        self.verify()?;
        let capture = validated.into_capture(self.policy)?;
        let version = crate::candidate_validation::version(&capture.store);
        self.verify()?;
        Ok(ProvenBase {
            capture,
            version,
            policy_identity: self.policy_identity.clone(),
        })
    }

    /// Routing version alone grants no authority. Absent/unproven bases require
    /// full validation; stale bases or changed native identities fail closed.
    pub fn check(
        &self,
        base: Option<&ProvenBase>,
        expected_base_version: &str,
        candidate: &Path,
    ) -> Result<Preservation> {
        self.check_before_accept(base, expected_base_version, candidate, || {})
    }

    fn check_before_accept(
        &self,
        base: Option<&ProvenBase>,
        expected_base_version: &str,
        candidate: &Path,
        before_accept: impl FnOnce(),
    ) -> Result<Preservation> {
        self.verify()?;
        let Some(base) = base else {
            return Ok(Preservation::FullLawRequired("unproven base"));
        };
        require(
            base.policy_identity == self.policy_identity && base.version == expected_base_version,
            "stale or foreign base receipt",
        )?;
        base.capture.unchanged_all()?;
        let after = Capture::read(candidate, None)?;
        self.check_captured(base, expected_base_version, &after, before_accept)
    }

    /// Crate-only capture path: candidate validation keeps this exact snapshot
    /// for body/status/token checks and its final source barrier as well.
    pub(crate) fn check_captured(
        &self,
        base: &ProvenBase,
        expected_base_version: &str,
        after: &Capture,
        before_accept: impl FnOnce(),
    ) -> Result<Preservation> {
        self.verify()?;
        require(
            base.policy_identity == self.policy_identity && base.version == expected_base_version,
            "stale or foreign base receipt",
        )?;
        base.capture.unchanged_all()?;
        let before = &base.capture.store;
        let candidate = &after.store;
        let verdict = if body_only(before, candidate)? {
            let ids = before.nodes.keys().cloned().collect::<Vec<_>>();
            // Shared injective symbol table, not independent positional UUID or
            // reason mappings which could hide identity changes across versions.
            let mut tokens = Tokens::default();
            let old = encode(before, &ids, &mut tokens);
            let new = encode(candidate, &ids, &mut tokens);
            self.run(&old, &new)?
        } else {
            false
        };
        before_accept();
        base.capture.unchanged_all()?;
        after.unchanged_all()?;
        self.verify()?;
        Ok(if verdict {
            Preservation::Preserved {
                base_version: base.version.clone(),
                candidate_version: crate::candidate_validation::version(candidate),
            }
        } else {
            Preservation::FullLawRequired("not an unchanged-graph/body-only transition")
        })
    }

    fn run(&self, before: &Graph, after: &Graph) -> Result<bool> {
        self.verify()?;
        let before = graph_words(before)?;
        let after = graph_words(after)?;
        let wire = [vec![1, before.len()], before, after]
            .concat()
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        require(wire.len() < LIMIT, "transition wire exceeds slice limit")?;
        let output = invoke(&self.executable, wire.as_bytes())?;
        self.verify()?;
        match output.as_str() {
            "STGT1 PRESERVED\n" => Ok(true),
            "STGT1 FALLBACK\n" => Ok(false),
            _ => Err("malformed checked transition verdict".into()),
        }
    }
}
fn identity(policy: &Policy) -> Result<String> {
    policy.validation_identity()
}

// Native edit-class guard, not a graph validator. Equality of relationship
// files alone is deliberately NOT substituted for the compiled Bend predicate.
fn body_only(before: &crate::Store, after: &crate::Store) -> Result<bool> {
    if before.nodes.keys().ne(after.nodes.keys())
        || before.inventory.keys().ne(after.inventory.keys())
    {
        return Ok(false);
    }
    for (path, old) in &before.inventory {
        let new = &after.inventory[path];
        if old == new {
            continue;
        }
        let node_file = path.file_name().is_some_and(|p| p == "node.md")
            && path.components().count() == 3
            && path.starts_with("nodes");
        match (node_file, old, new) {
            (true, Entry::File(a), Entry::File(b)) => {
                let header = |bytes: &[u8]| -> Result<usize> {
                    let text = std::str::from_utf8(bytes)?;
                    let body = crate::markdown(text)?.authored.content;
                    Ok(bytes.len() - body.len())
                };
                if a[..header(a)?] != b[..header(b)?] {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
}
