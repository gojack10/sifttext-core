//! One native ceiling verdict in the repository owner's Git metadata.
use crate::Result;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

pub(super) struct AcceptedCeiling {
    path: PathBuf,
    policy: String,
    owner: u32,
}

impl AcceptedCeiling {
    pub(super) fn open(common: &Path, policy: &str) -> Option<Self> {
        let metadata = fs::symlink_metadata(common).ok()?;
        let owner = unsafe { libc::geteuid() };
        if !metadata.is_dir() || metadata.uid() != owner || metadata.mode() & 0o022 != 0 {
            return None;
        }
        Some(Self {
            path: common.join("sifttext-accepted-ceiling"),
            policy: policy.to_owned(),
            owner,
        })
    }

    fn verdict(&self, commit: &str, tree: &str, version: &str) -> String {
        format!(
            "sifttext-accepted-ceiling-v1\n{commit}\n{tree}\n{version}\n{}\n{}\n",
            self.policy,
            crate::token_count::vocabulary_identity(),
        )
    }

    pub(super) fn matches(&self, commit: &str, tree: &str, version: &str) -> bool {
        let Ok(file) = OpenOptions::new().read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&self.path) else { return false };
        let expected = self.verdict(commit, tree, version);
        let Ok(metadata) = file.metadata() else { return false };
        if !metadata.is_file() || metadata.uid() != self.owner || metadata.mode() & 0o777 != 0o600
            || metadata.nlink() != 1 || metadata.len() != expected.len() as u64
        {
            return false;
        }
        let mut observed = Vec::with_capacity(expected.len());
        file.take(expected.len() as u64 + 1).read_to_end(&mut observed).is_ok()
            && observed == expected.as_bytes()
    }

    pub(super) fn record(&self, commit: &str, tree: &str, version: &str) -> Result<()> {
        let directory = self.path.parent().ok_or("missing Git metadata directory")?;
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        file.write_all(self.verdict(commit, tree, version).as_bytes())?;
        file.persist(&self.path)?;
        Ok(())
    }
}
