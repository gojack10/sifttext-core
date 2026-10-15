//! Integration attribution follows the submitted commit's selected policy.
use std::{io, process::Command};

/// `metadata` comes from sanitized Git's
/// `log -1 --format=%an%x00%ae%x00%(trailers:key=Co-authored-by) <submitted>`.
/// The caller retains its guarded transport, parent/tree checks and environment fence.
pub(crate) fn integration_commit(command: &mut Command, metadata: &str) -> io::Result<()> {
    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid submission attribution",
        )
    };
    let fields: Vec<_> = metadata.split('\0').collect();
    let [name, email, trailers] = fields.as_slice() else {
        return Err(invalid());
    };
    for identity in [name, email] {
        if identity.trim().is_empty() || identity.chars().any(char::is_control) {
            return Err(invalid());
        }
    }
    let mut message = "checked-publication: combine retained draft".to_owned();
    for line in trailers.lines().filter(|line| !line.is_empty()) {
        let Some((key, credit)) = line.split_once(':') else {
            return Err(invalid());
        };
        if !key.eq_ignore_ascii_case("Co-authored-by")
            || credit.trim().is_empty()
            || line.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        if !message.contains('\n') {
            message.push('\n');
        }
        message.push('\n');
        message.push_str(line);
    }
    command
        .env("GIT_AUTHOR_NAME", name)
        .env("GIT_AUTHOR_EMAIL", email)
        .env("GIT_COMMITTER_NAME", name)
        .env("GIT_COMMITTER_EMAIL", email)
        .arg("-m")
        .arg(message);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn git(repo: &Scratch, args: &[&str]) -> Command {
        let mut c = Command::new("git");
        for (key, _) in std::env::vars_os() {
            if key.as_encoded_bytes().starts_with(b"GIT_") {
                c.env_remove(key);
            }
        }
        c.env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .arg("-C")
            .arg(&repo.0)
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgSign=false",
            ])
            .args(args);
        c
    }
    fn output(c: &mut Command) -> String {
        let out = c.output().unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout)
            .unwrap()
            .trim_end_matches('\n')
            .to_owned()
    }

    #[test]
    fn integration_preserves_selected_author_and_credit_without_config_changes() {
        let repo = Scratch(std::env::temp_dir().join(format!(
                "sifttext-attribution-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )));
        fs::create_dir(&repo.0).unwrap();
        output(&mut git(&repo, &["init", "--bare", "-q"]));
        let config = fs::read(repo.0.join("config")).unwrap();
        let tree = output(git(&repo, &["mktree"]).stdin(std::process::Stdio::null()));
        for (name, email, credit) in [
            (
                "Selected Human — 中",
                "human@example.invalid",
                "Co-authored-by: SiftText Agent <agent@sifttext.com>",
            ),
            ("SiftText Agent", "agent@sifttext.com", ""),
            (
                "Selected Human",
                "human@example.invalid",
                "Co-authored-by: SiftText Swarm Agent deadbeef <agent@sifttext.com>",
            ),
            ("SiftText Swarm Agent deadbeef", "agent@sifttext.com", ""),
            ("Selected Human", "human@example.invalid", ""), // Owner omitted agent credit.
            (
                "Selected Human",
                "human@example.invalid",
                "Co-authored-by: SiftText Swarm Agent deadbeef <agent@sifttext.com>\nCo-authored-by: SiftText Swarm Agent cafe1234 <agent@sifttext.com>",
            ),
        ] {
            let message = format!("knowledge: edit\n\n{credit}");
            let source = output(
                git(&repo, &["commit-tree", &tree, "-m", &message])
                    .env("GIT_AUTHOR_NAME", name)
                    .env("GIT_AUTHOR_EMAIL", email)
                    .env("GIT_COMMITTER_NAME", name)
                    .env("GIT_COMMITTER_EMAIL", email),
            );
            let format = "%an%x00%ae%x00%(trailers:key=Co-authored-by)";
            let metadata = output(&mut git(
                &repo,
                &["log", "-1", &format!("--format={format}"), &source],
            ));
            let mut merge = git(&repo, &["commit-tree", &tree, "-p", &source]);
            merge
                .env("GIT_AUTHOR_NAME", "Hostile inherited author")
                .env("GIT_AUTHOR_EMAIL", "hostile@example.invalid")
                .env("GIT_COMMITTER_NAME", "Hostile inherited committer")
                .env("GIT_COMMITTER_EMAIL", "hostile@example.invalid");
            integration_commit(&mut merge, &metadata).unwrap();
            let accepted = output(&mut merge);
            assert_eq!(
                output(&mut git(
                    &repo,
                    &["log", "-1", &format!("--format={format}"), &accepted]
                )),
                metadata
            );
            assert_eq!(
                output(&mut git(
                    &repo,
                    &["log", "-1", "--format=%cn%x00%ce", &accepted]
                )),
                format!("{name}\0{email}")
            );
            assert_eq!(
                output(&mut git(&repo, &["log", "-1", "--format=%s", &accepted])),
                "checked-publication: combine retained draft"
            );
        }
        assert_eq!(fs::read(repo.0.join("config")).unwrap(), config);
    }

    #[test]
    fn malformed_metadata_does_not_configure_a_commit() {
        for metadata in [
            "",
            "name\0email",
            "\0email\0",
            "name\0\0",
            "name\nspoof\0email\0",
            "name\0email\rspoof\0",
            "name\0email\0Co-authored-by: ",
            "name\0email\0Signed-off-by: other",
            "name\0email\0Co-authored-by: other\0extra",
            "name\0email\0Co-authored-by: good\nSigned-off-by: bad",
        ] {
            let mut c = Command::new("git");
            assert!(integration_commit(&mut c, metadata).is_err());
            assert_eq!(c.get_envs().count(), 0);
            assert_eq!(c.get_args().count(), 0);
        }
    }
}
