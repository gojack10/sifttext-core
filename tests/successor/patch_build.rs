use super::*;
use std::{env, ffi::OsString, os::unix::fs::PermissionsExt, sync::Mutex};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn path_with_prefix(prefix: &Path, original: &OsString) -> OsString {
    env::join_paths(std::iter::once(prefix.to_owned()).chain(env::split_paths(original))).unwrap()
}

struct RestorePath(OsString);

impl Drop for RestorePath {
    fn drop(&mut self) {
        unsafe { env::set_var("PATH", &self.0) };
    }
}

fn prepare_fake_builder(active: &Path, root: &Path) {
    let bend = active.join("test-bend");
    fs::write(&bend, "#!/bin/sh\necho 'bend 2.0.27'\n").unwrap();
    fs::set_permissions(&bend, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        active.join("scripts/bend-path.sh"),
        "#!/bin/sh\nprintf '%s\\n' \"$PWD/test-bend\"\n",
    )
    .unwrap();
    fs::set_permissions(
        active.join("scripts/bend-path.sh"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();

    let counter = root.join("build-count");
    let script = format!(
        "#!/bin/sh\nset -eu\nprintf x >> '{}'\nmkdir -p \"$CARGO_TARGET_DIR/release\"\ndigest=$(shasum -a 256 nodes/fixture.md | cut -d ' ' -f 1)\nprintf '#!/bin/sh\\n# %s\\nexit 0\\n' \"$digest\" > \"$CARGO_TARGET_DIR/release/sifttext\"\nchmod 700 \"$CARGO_TARGET_DIR/release/sifttext\"\ncp \"$CARGO_TARGET_DIR/release/sifttext\" sifttext\n{{ printf '%s\\n' Cargo.toml Cargo.lock README.md .gitignore rust-toolchain.toml; for dir in src laws scripts tests fixtures docs; do if [ -d \"$dir\" ]; then find \"$dir\" -type f | LC_ALL=C sort; fi; done; }} | while IFS= read -r file; do shasum -a 256 \"$file\"; done > build/candidate.sha256\n",
        counter.display()
    );
    let build = active.join("scripts/build.sh");
    fs::write(&build, script).unwrap();
    fs::set_permissions(&build, fs::Permissions::from_mode(0o700)).unwrap();
    fs::create_dir_all(active.join("nodes")).unwrap();
    fs::write(active.join("nodes/fixture.md"), "embedded: original\n").unwrap();
    fs::write(active.join("AGENTS.md"), "embedded instructions\n").unwrap();
    fs::write(active.join("install-local.sh"), "embedded installer\n").unwrap();
}

fn build_runs(root: &Path) -> usize {
    fs::read_to_string(root.join("build-count")).map_or(0, |runs| runs.len())
}

#[test]
fn exact_patched_build_is_reused_and_embedded_change_rebuilds() {
    let _env = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let (_policy, state, _, _, _) = fixture(&root);
    let active = root.join(&state.application);
    prepare_fake_builder(&active, &root);

    let source_dir = root.join("releases");
    fs::create_dir_all(&source_dir).unwrap();
    let first = source_dir.join("source-first");
    let second = source_dir.join("source-second");
    copy_tree(&active, &first).unwrap();
    copy_tree(&active, &second).unwrap();
    let candidate = build_candidate(&root, &first).unwrap();
    assert_eq!(build_runs(&root), 1);
    assert_eq!(build_candidate(&root, &second).unwrap(), candidate);
    assert_eq!(build_runs(&root), 1, "exact inputs should reuse the build");

    for (index, (name, contents)) in [
        ("nodes/fixture.md", "embedded: changed\n"),
        ("AGENTS.md", "changed embedded instructions\n"),
        ("install-local.sh", "changed embedded installer\n"),
    ]
    .into_iter()
    .enumerate()
    {
        let source = root.join(format!("source-changed-{index}"));
        copy_tree(&active, &source).unwrap();
        fs::write(source.join(name), contents).unwrap();
        let changed = build_candidate(&root, &source).unwrap();
        assert_ne!(changed, candidate, "{name} must change cache identity");
    }
    assert_eq!(
        build_runs(&root),
        4,
        "each changed embedded input must rebuild"
    );
}

fn fake_tool(path: &Path, real: &Path) {
    fs::write(
        path,
        format!("#!/bin/sh\nexec '{}' \"$@\"\n", real.display()),
    )
    .unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn resolved_compiler_and_linker_path_changes_miss_build_cache() {
    let _env = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let (_policy, state, _, _, _) = fixture(&root);
    let active = root.join(&state.application);
    prepare_fake_builder(&active, &root);

    let original_path = env::var_os("PATH").unwrap();
    let _restore_path = RestorePath(original_path.clone());
    let _restore_cc = RestoreEnv::new("CC");
    let rustc = env::split_paths(&original_path)
        .map(|dir| dir.join("rustc"))
        .find(|path| path.is_file())
        .unwrap();
    let cc = env::split_paths(&original_path)
        .map(|dir| dir.join("cc"))
        .find(|path| path.is_file())
        .unwrap();
    let first_source = root.join("source-first");
    let second_source = root.join("source-second");
    copy_tree(&active, &first_source).unwrap();
    copy_tree(&active, &second_source).unwrap();

    let compiler_a = root.join("compiler-a");
    let compiler_b = root.join("compiler-b");
    fs::create_dir(&compiler_a).unwrap();
    fs::create_dir(&compiler_b).unwrap();
    fake_tool(&compiler_a.join("rustc"), &rustc);
    fake_tool(&compiler_b.join("rustc"), &rustc);
    unsafe { env::set_var("PATH", path_with_prefix(&compiler_a, &original_path)) };
    let compiler_first = build_candidate(&root, &first_source).unwrap();
    unsafe { env::set_var("PATH", path_with_prefix(&compiler_b, &original_path)) };
    let compiler_second = build_candidate(&root, &second_source).unwrap();
    assert_ne!(
        compiler_first, compiler_second,
        "resolved rustc path must affect cache identity"
    );

    let linker_first_source = root.join("linker-source-first");
    let linker_second_source = root.join("linker-source-second");
    copy_tree(&active, &linker_first_source).unwrap();
    copy_tree(&active, &linker_second_source).unwrap();

    let linker_a = root.join("linker-a");
    let linker_b = root.join("linker-b");
    fs::create_dir(&linker_a).unwrap();
    fs::create_dir(&linker_b).unwrap();
    fake_tool(&linker_a.join("cc"), &cc);
    fake_tool(&linker_b.join("cc"), &cc);
    unsafe { env::set_var("CC", "cc") };
    unsafe { env::set_var("PATH", path_with_prefix(&linker_a, &original_path)) };
    let linker_first = build_candidate(&root, &linker_first_source).unwrap();
    unsafe { env::set_var("PATH", path_with_prefix(&linker_b, &original_path)) };
    let linker_second = build_candidate(&root, &linker_second_source).unwrap();
    assert_ne!(
        linker_first, linker_second,
        "resolved linker path must affect cache identity"
    );
    assert_eq!(
        build_runs(&root),
        4,
        "each changed resolved tool path must rebuild"
    );
}

struct RestoreEnv {
    key: &'static str,
    value: Option<OsString>,
}

impl RestoreEnv {
    fn new(key: &'static str) -> Self {
        Self {
            key,
            value: env::var_os(key),
        }
    }
}

impl Drop for RestoreEnv {
    fn drop(&mut self) {
        unsafe {
            if let Some(value) = &self.value {
                env::set_var(self.key, value);
            } else {
                env::remove_var(self.key);
            }
        }
    }
}

#[test]
fn wrapper_target_linker_and_same_path_tool_replacements_miss_cache() {
    let _env = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let (_policy, state, _, _, _) = fixture(&root);
    let active = root.join(&state.application);
    prepare_fake_builder(&active, &root);
    let original_path = env::var_os("PATH").unwrap();
    let _restore_path = RestorePath(original_path.clone());
    let _restore_rustc = RestoreEnv::new("RUSTC");
    let _restore_wrapper = RestoreEnv::new("RUSTC_WRAPPER");
    let _restore_linker = RestoreEnv::new("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER");

    let real_rustc = env::split_paths(&original_path)
        .map(|dir| dir.join("rustc"))
        .find(|path| path.is_file())
        .unwrap();
    let compiler = root.join("compiler");
    let wrapper = root.join("rustc-wrapper");
    fake_tool(&compiler, &real_rustc);
    fake_tool(&wrapper, &real_rustc);
    unsafe { env::set_var("RUSTC", &compiler) };
    unsafe { env::set_var("RUSTC_WRAPPER", &wrapper) };
    let source_first = root.join("source-first");
    copy_tree(&active, &source_first).unwrap();
    let first = build_candidate(&root, &source_first).unwrap();
    fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\n# same path replacement\nexec '{}' \"$@\"\n",
            real_rustc.display()
        ),
    )
    .unwrap();
    let source_second = root.join("source-second");
    copy_tree(&active, &source_second).unwrap();
    let second = build_candidate(&root, &source_second).unwrap();
    assert_ne!(first, second, "wrapper bytes must affect cache identity");

    fs::write(
        &compiler,
        format!(
            "#!/bin/sh\n# compiler replaced at same path\nexec '{}' \"$@\"\n",
            real_rustc.display()
        ),
    )
    .unwrap();
    let source_third = root.join("source-third");
    copy_tree(&active, &source_third).unwrap();
    let third = build_candidate(&root, &source_third).unwrap();
    assert_ne!(
        second, third,
        "same-path compiler replacement must miss cache"
    );

    let linker = root.join("target-linker");
    fake_tool(&linker, &real_rustc);
    unsafe { env::set_var("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER", &linker) };
    let source_fourth = root.join("source-fourth");
    copy_tree(&active, &source_fourth).unwrap();
    let fourth = build_candidate(&root, &source_fourth).unwrap();
    assert_ne!(
        third, fourth,
        "target-specific linker must affect cache identity"
    );
    assert_eq!(build_runs(&root), 4);
}

#[test]
fn same_path_archiver_replacement_changes_build_fingerprint() {
    let _env = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let (_policy, state, _, _, _) = fixture(&root);
    let active = root.join(&state.application);
    prepare_fake_builder(&active, &root);
    let _restore_ar = RestoreEnv::new("AR");
    let real_ar = env::split_paths(&env::var_os("PATH").unwrap())
        .map(|dir| dir.join("ar"))
        .find(|path| path.is_file())
        .unwrap();
    let archiver = root.join("archiver");
    fake_tool(&archiver, &real_ar);
    unsafe { env::set_var("AR", &archiver) };
    let first = crate::patch_build::fingerprint(&active).unwrap();
    fs::write(&archiver, b"same-path replacement").unwrap();
    let second = crate::patch_build::fingerprint(&active).unwrap();
    assert_ne!(
        first, second,
        "same-path AR replacement must change cache identity"
    );
}

#[test]
fn interrupted_pinned_rebuild_recovers_new_digest_and_immediate_rollback_target() {
    let _env = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let (_policy, mut state, _, _, _) = fixture(&root);
    let active = root.join(&state.application);
    prepare_fake_builder(&active, &root);
    state.application_sha256 = crate::sha256(&fs::read(active.join("sifttext")).unwrap());
    let immediate = Recovery {
        application: state.application.clone(),
        application_sha256: state.application_sha256.clone(),
        knowledge: state.knowledge.clone(),
        baseline: state.baseline.clone(),
        update_commit: None,
    };
    state.previous = Some(Recovery {
        application: "releases/older".into(),
        application_sha256: "f".repeat(64),
        knowledge: "e".repeat(64),
        baseline: None,
        update_commit: None,
    });
    persist(&root.join("state.json"), &state).unwrap();
    persist(
        &state.home.join(".sifttext/config.json"),
        &json!({"updates":{"pinned":"old-pin"}}),
    )
    .unwrap();

    fs::remove_file(root.join("current")).unwrap();
    fs::create_dir(root.join("current")).unwrap();
    fs::write(root.join("current/hold"), "force activation rename failure").unwrap();
    assert!(rebuild_active(&root, &mut state).is_err());

    let activation: Value = load(&root.join("activation.json")).unwrap();
    let app: PathBuf = serde_json::from_value(activation["application"].clone()).unwrap();
    let digest = activation["application_sha256"].as_str().unwrap();
    assert_eq!(
        activation["previous"]["application"],
        immediate.application.to_string_lossy().as_ref()
    );
    assert_eq!(
        activation["previous"]["application_sha256"],
        immediate.application_sha256
    );
    assert_eq!(
        digest,
        crate::sha256(&fs::read(root.join(&app).join("sifttext")).unwrap())
    );

    fs::remove_dir_all(root.join("current")).unwrap();
    update(&root, "--recover").unwrap();
    let recovered: State = load(&root.join("state.json")).unwrap();
    assert_eq!(recovered.application, app);
    assert_eq!(recovered.application_sha256, digest);
    let previous = recovered.previous.as_ref().unwrap();
    assert_eq!(previous.application, immediate.application);
    assert_eq!(previous.application_sha256, immediate.application_sha256);
    let config = update_config(&state.home).unwrap();
    assert_eq!(config["pinned"], activation["pinned"]);
}

#[test]
fn cargo_user_config_changes_miss_build_cache() {
    let _env = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let (_policy, state, _, _, _) = fixture(&root);
    let active = root.join(&state.application);
    prepare_fake_builder(&active, &root);
    let _restore_cargo_home = RestoreEnv::new("CARGO_HOME");
    let cargo_home = root.join("cargo-home");
    fs::create_dir(&cargo_home).unwrap();
    let config = cargo_home.join("config.toml");
    fs::write(&config, "[build]\\nrustflags = ['-C', 'opt-level=1']\\n").unwrap();
    unsafe { env::set_var("CARGO_HOME", &cargo_home) };

    let source_dir = root.join("releases");
    fs::create_dir_all(&source_dir).unwrap();
    let first_source = source_dir.join("source-first");
    let second_source = source_dir.join("source-second");
    copy_tree(&active, &first_source).unwrap();
    copy_tree(&active, &second_source).unwrap();
    let first = build_candidate(&root, &first_source).unwrap();
    assert_eq!(build_runs(&root), 1);

    fs::write(&config, "[build]\\nrustflags = ['-C', 'opt-level=2']\\n").unwrap();
    let changed_source = root.join("source-changed");
    copy_tree(&active, &changed_source).unwrap();
    let changed = build_candidate(&root, &changed_source).unwrap();
    assert_ne!(
        changed, first,
        "Cargo config contents must affect cache identity"
    );
    assert_eq!(build_runs(&root), 2);

    for (index, config_dir) in [source_dir.join(".cargo"), root.join(".cargo")]
        .into_iter()
        .enumerate()
    {
        fs::create_dir_all(&config_dir).unwrap();
        let config = config_dir.join("config.toml");
        fs::write(
            &config,
            format!("[build]\\nrustflags = ['-C', 'opt-level={}']\\n", index + 3),
        )
        .unwrap();
        let temporary = tempfile::tempdir_in(&source_dir).unwrap();
        let source = temporary.path().join(format!("source-ancestor-{index}"));
        copy_tree(&active, &source).unwrap();
        let changed = build_candidate(&root, &source).unwrap();
        assert_ne!(
            changed, first,
            "ancestor Cargo config must change cache identity"
        );
    }
    assert_eq!(build_runs(&root), 4);
}

#[test]
fn same_path_clang_replacement_misses_build_cache() {
    let _env = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let (_policy, state, _, _, _) = fixture(&root);
    let active = root.join(&state.application);
    prepare_fake_builder(&active, &root);
    let original_path = env::var_os("PATH").unwrap();
    let _restore_path = RestorePath(original_path.clone());
    let tools = root.join("tools");
    fs::create_dir(&tools).unwrap();
    let clang = tools.join("clang");
    unsafe { env::set_var("PATH", path_with_prefix(&tools, &original_path)) };

    fs::write(
        &clang,
        "#!/bin/sh\\n# first bytes\\necho 'clang version 18.1.0 (fixture)'\\n",
    )
    .unwrap();
    fs::set_permissions(&clang, fs::Permissions::from_mode(0o700)).unwrap();
    let first_source = root.join("source-first");
    copy_tree(&active, &first_source).unwrap();
    let first = build_candidate(&root, &first_source).unwrap();

    fs::write(
        &clang,
        "#!/bin/sh\\n# replacement bytes\\necho 'clang version 18.1.0 (fixture)'\\n",
    )
    .unwrap();
    let second_source = root.join("source-second");
    copy_tree(&active, &second_source).unwrap();
    let second = build_candidate(&root, &second_source).unwrap();
    assert_ne!(first, second, "same-path Clang replacement must miss cache");
    assert_eq!(build_runs(&root), 2);
}

#[test]
fn rustup_toolchain_selection_misses_build_cache() {
    let _env = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let (_policy, state, _, _, _) = fixture(&root);
    let active = root.join(&state.application);
    prepare_fake_builder(&active, &root);
    let _restore_toolchain = RestoreEnv::new("RUSTUP_TOOLCHAIN");
    let first_source = root.join("source-first");
    let second_source = root.join("source-second");
    copy_tree(&active, &first_source).unwrap();
    copy_tree(&active, &second_source).unwrap();

    unsafe { env::set_var("RUSTUP_TOOLCHAIN", "stable-aarch64-apple-darwin") };
    let first = build_candidate(&root, &first_source).unwrap();
    unsafe { env::set_var("RUSTUP_TOOLCHAIN", "1.95.0-aarch64-apple-darwin") };
    let second = build_candidate(&root, &second_source).unwrap();
    assert_ne!(
        first, second,
        "selected rustup toolchain must affect cache identity"
    );
    assert_eq!(build_runs(&root), 2);
}
