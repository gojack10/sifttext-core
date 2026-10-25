use std::{
    fs,
    os::unix::fs::symlink,
    process::{Command, Output, Stdio},
};

const ID: &str = "00000000-0000-4000-8000-000000000001";

#[test]
fn read_returns_exact_bytes_or_one_token_count_notice_without_mutation() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let home = root.join("store");
    let git_config = root.join("gitconfig");
    fs::write(&git_config, "[user]\nname = Synthetic Fixture\nemail = fixture@example.invalid\n[commit]\ngpgSign = false\n[core]\nhooksPath = /dev/null\n").unwrap();
    let mut records = Vec::new();
    let mut run = |args: &[&str]| -> Output {
        let out = Command::new(env!("CARGO_BIN_EXE_sifttext"))
            .args(args)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("HOME", &root)
            .env("XDG_CONFIG_HOME", root.join(".config"))
            .env("SIFTTEXT_NO_UPDATE", "1")
            .env("GIT_CONFIG_GLOBAL", &git_config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Synthetic Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Synthetic Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        records.push(serde_json::json!({"args":args,"exit":out.status.code(),
            "stdout":String::from_utf8_lossy(&out.stdout),"stderr":String::from_utf8_lossy(&out.stderr)}));
        if let Some(path) = std::env::var_os("SIFTTEXT_READ_EVIDENCE") {
            fs::create_dir_all(&path).unwrap();
            fs::write(
                std::path::Path::new(&path).join("commands.json"),
                serde_json::to_vec_pretty(&records).unwrap(),
            )
            .unwrap();
        }
        out
    };
    let path = home.to_str().unwrap();
    let init = run(&["init", path, "--empty"]);
    assert!(init.status.success(), "{init:?}");
    let node = home.join("nodes").join(ID);
    fs::create_dir_all(node.join("children")).unwrap();
    fs::write(node.join("children.order"), "").unwrap();
    symlink(format!("../nodes/{ID}"), home.join("trees").join(ID)).unwrap();
    let authored = node.join("node.md");
    let tokenizer = tiktoken_rs::o200k_base_singleton();
    let render = |padding: usize| {
        format!(
            "---\nid: \"{ID}\"\nname: \"Large Read\"\nmicro_summary: \"Synthetic output boundary\"\nstatus: \"not_examined\"\nsee_also: []\nscope: \"日本語🦀{}\"\n---\nSmall content.\n",
            " ordinary".repeat(padding)
        )
    };
    let base_tokens = tokenizer.lock().encode_ordinary(&render(0)).len();
    for expected in [15_000, 15_001] {
        let text = render(expected - base_tokens);
        assert_eq!(tokenizer.lock().encode_ordinary(&text).len(), expected);
        fs::write(&authored, &text).unwrap();
        let before_head = fs::read(home.join(".git/HEAD")).unwrap();
        let before_index = fs::read(home.join(".git/index")).unwrap();
        let out = run(&["read", path, ID]);
        assert_eq!(
            out.status.code(),
            Some(if expected == 15_000 { 0 } else { 1 }),
            "{out:?}"
        );
        assert!(out.stderr.is_empty(), "{out:?}");
        assert_eq!(
            out.stdout,
            if expected == 15_000 {
                text.as_bytes().to_vec()
            } else {
                format!("[{expected} tokens suppressed] Re-run this sifttext read command with -y to print them.\n").into_bytes()
            }
        );
        let forced = run(&["read", path, ID, "-y"]);
        assert_eq!(forced.status.code(), Some(0), "{forced:?}");
        assert!(forced.stderr.is_empty(), "{forced:?}");
        assert_eq!(forced.stdout, text.as_bytes());
        assert_eq!(fs::read(&authored).unwrap(), text.as_bytes());
        assert_eq!(fs::read(home.join(".git/HEAD")).unwrap(), before_head);
        assert_eq!(fs::read(home.join(".git/index")).unwrap(), before_index);
    }
    let before = fs::read(&authored).unwrap();
    let invalid = run(&["read", path, "not-a-node", "-y"]);
    assert_eq!(invalid.status.code(), Some(1), "{invalid:?}");
    assert!(invalid.stdout.is_empty(), "{invalid:?}");
    assert!(String::from_utf8_lossy(&invalid.stderr).starts_with("rejected:"));
    assert_eq!(fs::read(&authored).unwrap(), before);
}
