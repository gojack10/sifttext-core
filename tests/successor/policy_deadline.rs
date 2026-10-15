use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

fn id(n: usize) -> String {
    format!("71000000-0000-4000-8000-{n:012x}")
}
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("trees")).unwrap();
    for n in 1..=3 {
        let path = dir.path().join("nodes").join(id(n));
        fs::create_dir_all(path.join("children")).unwrap();
        fs::write(path.join("node.md"), format!(
            "---\nid: {:?}\nname: \"Synthetic {n}\"\nmicro_summary: \"Synthetic deadline test\"\nstatus: \"in_progress\"\nsee_also: []\n---\nBody {n}.\n", id(n))).unwrap();
        fs::write(
            path.join("children.order"),
            if n == 1 {
                format!("{}\n{}\n", id(2), id(3))
            } else {
                String::new()
            },
        )
        .unwrap();
    }
    for n in 2..=3 {
        symlink(
            format!("../../{}", id(n)),
            dir.path()
                .join("nodes")
                .join(id(1))
                .join("children")
                .join(id(n)),
        )
        .unwrap();
    }
    symlink(
        format!("../nodes/{}", id(1)),
        dir.path().join("trees").join(id(1)),
    )
    .unwrap();
    dir
}
fn helper(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("producer");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}
fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
fn checked_sizing_small_floor_quadratic_growth_and_hard_cap() {
    assert_eq!(policy_budget(0, 0).unwrap(), PROCESS_DEADLINE);
    assert_eq!(policy_budget(256, 1000).unwrap(), PROCESS_DEADLINE);
    assert_eq!(policy_budget(1000, 2000).unwrap(), Duration::from_secs(70));
    assert_eq!(policy_budget(10000, 30000).unwrap(), MAX_POLICY_DEADLINE);
    assert!(
        policy_budget(usize::MAX, 2)
            .unwrap_err()
            .to_string()
            .contains("overflow")
    );
    assert!(policy_budget(usize::MAX / 30 + 1, 1).is_err());
    let graph = Graph {
        assets: 1,
        roots: vec![1],
        nodes: vec![WireNode {
            id: 1,
            payload: 2,
            children: vec![2, 3],
            refs: vec![(4, 5)],
        }],
    };
    assert_eq!(policy_deadline(&graph).unwrap(), PROCESS_DEADLINE);
    let missing = Path::new("/not/a/producer");
    for budget in [Duration::ZERO, MAX_POLICY_DEADLINE + Duration::from_secs(1)] {
        assert_eq!(
            invoke_with_deadline(missing, b"", budget)
                .unwrap_err()
                .to_string(),
            "policy deadline exceeds resource budget"
        );
    }
}

#[test]
fn real_deadline_kills_and_reaps_producer_then_valid_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("pid");
    let exe = helper(
        dir.path(),
        &format!("echo $$ > '{}'; exec sleep 60", pidfile.display()),
    );
    let start = Instant::now();
    assert_eq!(
        invoke_with_deadline(&exe, b"wire", Duration::from_millis(100))
            .unwrap_err()
            .to_string(),
        "policy process timed out"
    );
    assert!(start.elapsed() >= Duration::from_millis(100));
    assert!(start.elapsed() < Duration::from_secs(3));
    let pid = fs::read_to_string(pidfile).unwrap();
    let alive = Command::new("kill")
        .args(["-0", pid.trim()])
        .output()
        .unwrap();
    assert!(!alive.status.success(), "producer was not reaped");
    let exe = helper(dir.path(), "printf 'STG1 VALID\\n'");
    assert_eq!(invoke(&exe, b"wire").unwrap(), "STG1 VALID\n");
}

#[test]
fn native_rejection_preserves_authored_and_accepted_derived_state() {
    let dir = fixture();
    let real = policy();
    let before = Store::read(dir.path()).unwrap();
    real.validate(&before).unwrap();
    let accepted = index::refresh(&real, dir.path(), None, index::Mode::Full, None).unwrap();
    let before = Store::read(dir.path()).unwrap();
    let cache = dir.path().join(".sifttext/index.sqlite");
    let cache_bytes = fs::read(&cache).unwrap();
    let producers = tempfile::tempdir().unwrap();
    for body in [
        "exit 9",
        "printf 'STG1 VALID\\n'; exit 9",
        "printf 'STG1 VAL'",
        "printf 'STG1 VALID\\nSTG1 VALID\\n'",
        "printf 'STG1 OK 1 1 1\\n'",
        "printf 'STG1 VALID\\n'; printf 'unexpected' >&2",
    ] {
        let fake = Policy {
            project: real.project.clone(),
            executable: helper(producers.path(), body),
        };
        assert!(fake.validate(&before).is_err());
        assert!(
            apply(
                &fake,
                &before,
                &Operation::Rename {
                    node: id(2),
                    name: "Never accepted".into()
                }
            )
            .is_err()
        );
        assert_eq!(Store::read(dir.path()).unwrap().inventory, before.inventory);
        assert_eq!(fs::read(&cache).unwrap(), cache_bytes);
    }
    let mut invalid = before.clone();
    invalid.roots.push(id(1));
    assert!(!real.graph_valid(&invalid).unwrap());
    assert_eq!(
        index::refresh(&real, dir.path(), None, index::Mode::Incremental, None)
            .unwrap()
            .version,
        accepted.version
    );
    real.validate(&Store::read(dir.path()).unwrap()).unwrap();
}

#[test]
fn checked_candidate_stamp_is_still_required() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("build")).unwrap();
    fs::write(dir.path().join("build/candidate.sha256"), "").unwrap();
    fs::write(dir.path().join("build/graph-policy.sha256"), "").unwrap();
    assert!(Policy::checked(dir.path()).is_err());
}
