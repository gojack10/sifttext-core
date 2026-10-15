use std::{fs, path::Path, process::Command};

fn git(path: &Path, config: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", config)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn init(home: &Path, config: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sifttext"))
        .arg("init")
        .arg(home)
        .arg("--empty")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", home.parent().unwrap())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", config)
        // Inherited commit identity must not override the selection.
        .env("GIT_AUTHOR_NAME", "Wrong Author")
        .env("GIT_AUTHOR_EMAIL", "wrong@example.invalid")
        .env("GIT_COMMITTER_NAME", "Wrong Committer")
        .env("GIT_COMMITTER_EMAIL", "wrong@example.invalid")
        .output()
        .unwrap()
}

#[test]
fn selected_init_identity_is_local_verified_and_preserved() {
    for (name, email) in [
        ("Configured User", "configured@example.invalid"),
        ("SiftText Agent", "agent@sifttext.com"),
        ("Confirmed GitHub User – 中", "confirmed@example.invalid"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let config = root.join("gitconfig");
        let knowledge = root.join("knowledge");
        fs::write(&config, "[unrelated]\n\tvalue = unchanged\n").unwrap();
        git(
            &root,
            &config,
            &[
                "config",
                "--file",
                config.to_str().unwrap(),
                "user.name",
                name,
            ],
        );
        git(
            &root,
            &config,
            &[
                "config",
                "--file",
                config.to_str().unwrap(),
                "user.email",
                email,
            ],
        );
        let before = fs::read(&config).unwrap();
        let output = init(&knowledge, &config);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read(&config).unwrap(), before);
        assert_eq!(
            git(
                &knowledge,
                &config,
                &["log", "-1", "--format=%an%n%ae%n%cn%n%ce"]
            ),
            format!("{name}\n{email}\n{name}\n{email}")
        );
        assert_eq!(
            git(&knowledge, &config, &["config", "--local", "user.name"]),
            name
        );
        assert_eq!(
            git(&knowledge, &config, &["config", "--local", "user.email"]),
            email
        );

        // Remove the temporary selection: later drafts still inherit local identity.
        fs::write(
            &config,
            "[user]\n\tname = Different User\n\temail = different@example.invalid\n",
        )
        .unwrap();
        let changed_config = fs::read(&config).unwrap();
        let draft = root.join("draft");
        git(
            &knowledge,
            &config,
            &[
                "worktree",
                "add",
                "-b",
                "identity-test",
                draft.to_str().unwrap(),
            ],
        );
        git(
            &draft,
            &config,
            &[
                "-c",
                "commit.gpgSign=false",
                "-c",
                "core.hooksPath=/dev/null",
                "commit",
                "--allow-empty",
                "-m",
                "identity: retained selection",
            ],
        );
        assert_eq!(
            git(&draft, &config, &["log", "-1", "--format=%an%n%ae"]),
            format!("{name}\n{email}")
        );
        let local_config = fs::read(knowledge.join(".git/config")).unwrap();
        let head = git(&knowledge, &config, &["rev-parse", "HEAD"]);
        assert!(init(&knowledge, &config).status.success());
        assert_eq!(
            fs::read(knowledge.join(".git/config")).unwrap(),
            local_config
        );
        assert_eq!(git(&knowledge, &config, &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(&config).unwrap(), changed_config);
    }

    for (selection, key) in [
        ("", "user.name"),
        ("[user]\n\tname = Partial\n", "user.email"),
        (
            "[user]\n\tname = Invalid <Identity>\n\temail = invalid@example.invalid\n",
            "user.name",
        ),
        ("[invalid config", "user.name"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let config = root.join("gitconfig");
        let home = root.join("knowledge");
        fs::write(&config, selection).unwrap();
        let output = init(&home, &config);
        assert!(!output.status.success());
        let diagnostic = String::from_utf8(output.stderr).unwrap();
        assert!(
            diagnostic.contains(&format!("Git config {key}")),
            "{diagnostic}"
        );
        if selection == "[invalid config" {
            assert!(diagnostic.contains("bad config"), "{diagnostic}");
        }
        assert!(
            !home.exists(),
            "invalid selection created an incomplete home"
        );
        assert_eq!(fs::read_to_string(&config).unwrap(), selection);
    }
}
