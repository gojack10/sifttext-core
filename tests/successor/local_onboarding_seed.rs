// Generated-seed content checks, not a client behavior or installed-workflow trial.
const ONBOARDING: &[&str] = &[
    "0dfce959-9e09-42c8-b8a1-1595c7b01e5f",
    "8fb66a6b-8e6a-442a-b4e6-faa135e39c86",
    "cd517c48-e498-40bf-a42e-ec5a0b52fbcb",
    "d04227c1-0ccd-4d52-98dc-dba4d4c7f0e3",
    "76d2f448-ed3e-4641-ae07-a353c7170ce5",
    "baf0f870-e03a-4312-a63a-4ff6c1114fa8",
    "e1bd7224-a729-4875-bfbd-ca154e5c1c0b",
    "43a8cb58-979e-47b0-9a64-1d3a0864666f",
    "bd544f51-4c46-4cd3-92dc-29f0452f0d70",
    "e05cca1d-59f2-4d00-a8b2-0a0fe00c80b6",
    "53f9a13b-7be3-42ee-89ff-7f4b2a4f7ea2",
    "b5cfbee3-dda0-4caf-ade0-0f132444eb5a",
    "95c0d4e2-778b-4d69-9882-14e6dbbff894",
    "d3557bc4-7040-4b67-8dd2-707c6ffc85eb",
    "fa3cf666-ef1f-49fa-a36a-31a56969931a",
];

fn node(id: &str) -> &'static str {
    let path = format!("nodes/{id}/node.md");
    super::FILES
        .iter()
        .find(|(name, _)| *name == path)
        .unwrap()
        .1
}

#[test]
fn startup_routes_to_scoped_memory_continuity_without_changing_the_six_child_contract() {
    let continuity = "0dfce959-9e09-42c8-b8a1-1595c7b01e5f";
    let parent = "e05cca1d-59f2-4d00-a8b2-0a0fe00c80b6";
    let file = |path: &str| super::FILES.iter().find(|(p, _)| *p == path).unwrap().1;
    let entry = node(crate::local::ENTRY);
    assert!(entry.contains(&format!("nodes/{continuity}/node.md")));
    assert!(entry.contains(&format!("\"target\":\"{continuity}\"")));
    assert_eq!(
        file(&format!("nodes/{}/children.order", crate::local::ENTRY))
            .lines()
            .count(),
        6
    );
    let current = node(parent)
        .split("## Preserved earlier procedure")
        .next()
        .unwrap();
    assert!(current.contains("Read the child named Maintain Memory Continuity"));
    assert!(
        file(&format!("nodes/{parent}/children.order"))
            .lines()
            .any(|id| id == continuity)
    );
    assert!(super::LINKS.iter().any(|(path, target)| {
        *path == format!("nodes/{parent}/children/{continuity}")
            && *target == format!("../../{continuity}")
    }));
    assert!(include_str!("../../AGENTS.md").contains("selected harness's persistent entrance"));
    assert!(include_str!("../../INSTALL.md").contains("Maintain Memory Continuity"));
    let method = node(continuity);
    for clause in [
        "A read or encountered instruction file grants no permission to edit it",
        "Repeating setup must not duplicate pointers",
        "A memory-only README can become a pointer",
        "A mixed README retains its other responsibilities",
        "preserve selected original content, source location and recoverable history",
        "publish and verify the maintained successor before replacing migrated material",
        "Do not import private material into public knowledge",
        "not three integrations qualified by this text",
        "do not restart onboarding",
        "not ordinary remember requests postponed until chat end",
    ] {
        assert!(
            method.contains(clause),
            "missing migration boundary: {clause}"
        );
    }
}

#[test]
fn embedded_onboarding_requires_no_hosted_connector_or_builtin_agent() {
    for id in ONBOARDING {
        let text = node(id);
        for unavailable in [
            "app.sifttext.com",
            "ideation_",
            "sifttext_get_node",
            "SHOTGUN",
            "SIFT is always available",
            "SIFT created",
            "SIFT asks",
            "SIFT runs",
            "Sign in with SiftText",
            "$20",
            "same tools SIFT uses",
        ] {
            assert!(!text.contains(unavailable), "{id}: {unavailable}");
        }
        assert!(
            text.contains("status: \"in_progress\""),
            "{id}: unproven completion"
        );
    }
    let entry = node("e05cca1d-59f2-4d00-a8b2-0a0fe00c80b6");
    for required in [
        "verified Core command path",
        "SiftText Read before interpreting",
        "permission",
        "Stop",
        "preserve drafts",
        "human",
        "MCP server",
    ] {
        assert!(
            entry.contains(required),
            "missing local handoff: {required}"
        );
    }
    assert!(node("bd544f51-4c46-4cd3-92dc-29f0452f0d70").contains("fixed baseline"));
    let continuation = node("fa3cf666-ef1f-49fa-a36a-31a56969931a");
    for required in ["`submit`", "normal home", "new private worktree"] {
        assert!(
            continuation.contains(required),
            "missing saved-work handoff: {required}"
        );
    }
    for id in [
        "53f9a13b-7be3-42ee-89ff-7f4b2a4f7ea2",
        "95c0d4e2-778b-4d69-9882-14e6dbbff894",
    ] {
        assert!(
            node(id).contains("does not gain access")
                || node(id).contains("does not read, edit or publish")
        );
    }
}
