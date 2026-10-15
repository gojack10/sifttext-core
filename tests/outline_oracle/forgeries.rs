// Incremental consumer's cache/forgery controls. Authored files remain the oracle;
// agreeing file/sidecar/manifest hashes must not admit forged semantics.
fn mutate_index(store: &Path, sql: &str, params: impl rusqlite::Params) {
    let db = rusqlite::Connection::open(store.join(".sifttext/index.sqlite")).unwrap();
    assert!(db.execute(sql, params).unwrap() > 0);
}
fn reseal_incremental_manifest(store: &Path, paths: &[PathBuf]) {
    let db = rusqlite::Connection::open(store.join(".sifttext/index.sqlite")).unwrap();
    let raw: String = db
        .query_row("SELECT value FROM metadata WHERE key='outlines'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let mut manifest: Value = serde_json::from_str(&raw).unwrap();
    for p in paths {
        let relative = p.strip_prefix(store).unwrap().to_str().unwrap();
        assert!(
            manifest["files"].get(relative).is_some(),
            "untracked forgery target"
        );
        manifest["files"][relative] = json!(sha(&fs::read(p).unwrap()));
    }
    assert_eq!(
        db.execute(
            "UPDATE metadata SET value=? WHERE key='outlines'",
            [manifest.to_string()]
        )
        .unwrap(),
        1
    );
}
#[test]
fn incremental_forgeries_fail_closed_and_refresh_restores_independent_artifacts() {
    let bpe = ORACLE_BPE.clone();
    let dir = TempDir::new().unwrap();
    let store = fs::canonicalize(dir.path()).unwrap().join("store");
    materialize(&store, &dependency_specimen());
    let (original, _) = incremental_call(&store);
    let version = original["version"].as_str().unwrap();
    check_native(&store, version, &bpe);
    let authored = authored_snapshot(&store);
    let baseline = generated_snapshot(&store);
    let p = file(&store, &incremental_id(3), "full");
    let side = p.with_file_name("outline.full.lean.meta.json");
    for case in [
        "manifest-schema",
        "wrong-global",
        "forged-sql",
        "missing-artifact",
        "missing-sidecar",
        "resealed-source",
        "resealed-row",
        "resealed-cost",
    ] {
        match case {
            "manifest-schema" => mutate_index(
                &store,
                "UPDATE metadata SET value=? WHERE key='outlines'",
                [r#"{"version":"old","files":{},"unknown_schema":2}"#],
            ),
            "wrong-global" => mutate_index(
                &store,
                "UPDATE metadata SET value='wrong-global' WHERE key='version'",
                [],
            ),
            "forged-sql" => mutate_index(&store, "UPDATE nodes SET content='forged'", []),
            "missing-artifact" => fs::remove_file(&p).unwrap(),
            "missing-sidecar" => fs::remove_file(&side).unwrap(),
            _ => {
                let original_text = fs::read_to_string(&p).unwrap();
                let text = match case {
                    "resealed-source" => {
                        original_text
                            .lines()
                            .map(|line| {
                                if line.starts_with("source_version:") {
                                    "source_version:planted-file-identity-v9"
                                } else {
                                    line
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                            + "\n"
                    }
                    "resealed-row" => original_text.replace("Summary 3 Ω", "Forged summary Ω"),
                    _ => original_text.clone(),
                };
                if case != "resealed-cost" {
                    assert_ne!(text, original_text, "{case}: mutation did not occur");
                }
                fs::write(&p, &text).unwrap();
                let mut meta: Value = serde_json::from_slice(&fs::read(&side).unwrap()).unwrap();
                meta["hash"] = json!(sha(text.as_bytes()));
                // Exact token reseal for semantic mutations, deliberately wrong for cost control.
                meta["tokens"] = json!(if case == "resealed-cost" {
                    1
                } else {
                    count(&text, &bpe)
                });
                fs::write(&side, serde_json::to_vec(&meta).unwrap()).unwrap();
                reseal_incremental_manifest(&store, &[p.clone(), side.clone()]);
            }
        }
        let forged = snapshot(&store);
        let (ok, rejected) = native(&store, "outline_inspect", None);
        assert!(!ok, "{case}: forged result succeeded: {rejected}");
        assert_eq!(rejected["status"], "stale", "{case}");
        dogfood_empty_unavailable(&rejected, "outline_inspect");
        assert_eq!(
            snapshot(&store),
            forged,
            "{case}: refusal mutated node files"
        );
        let (restored, _) = incremental_call(&store);
        assert_eq!(
            restored, original,
            "{case}: refresh changed complete index result"
        );
        assert_eq!(
            generated_snapshot(&store),
            baseline,
            "{case}: incomplete restoration"
        );
        assert_eq!(
            authored_snapshot(&store),
            authored,
            "{case}: authored files changed"
        );
        check_native(&store, version, &bpe);
    }
    let before_loss = outline_identity(&store);
    fs::remove_file(store.join(".sifttext/index.sqlite")).unwrap();
    let (ok, rejected) = native(&store, "outline_inspect", None);
    assert!(!ok);
    assert_eq!(rejected["status"], "pending");
    assert_eq!(rejected["version"], Value::Null);
    assert_eq!(rejected["files"], json!([]));
    let (restored, _) = incremental_call(&store);
    assert_eq!(restored, original);
    assert_eq!(
        outline_identity(&store),
        before_loss,
        "cache loss rewrote verified outlines"
    );
    check_native(&store, version, &bpe);
    fs::write(store.join(".sifttext/index.sqlite"), "not SQLite").unwrap();
    let corrupt = snapshot(&store);
    // This CLI failure emits its typed error on stderr, with no stdout result.
    // native_request expects a stdout observation, so inspect the process directly.
    let bin = std::env::var_os("CORE_OBSERVE_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/core-observe"));
    let mut command = Command::new(&bin);
    command.current_dir(bin.parent().unwrap().parent().unwrap().parent().unwrap());
    let out = capacity::observer_output(
        command,
        json!({"op":"index", "store":store, "mode":"incremental"})
            .to_string()
            .into_bytes(),
    );
    assert!(!out.status.success(), "corrupt cache was accepted");
    assert!(
        out.stdout.is_empty(),
        "corrupt cache emitted a usable result"
    );
    let rejected: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(rejected["status"], "error");
    assert_eq!(rejected["kind"], "sqlite");
    assert_eq!(
        snapshot(&store),
        corrupt,
        "corrupt-cache refusal changed node files"
    );
    assert_eq!(authored_snapshot(&store), authored);
}
