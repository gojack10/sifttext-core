// These bounded checks cover localized authoring and surface contracts, not all bundle claims.
const LOCAL_AUTHORING: &[&str] = &[
    "10f2d4ab-3318-45d8-9486-f26a817c829e",
    "31dc46ab-eaf1-4f1a-97ed-5ff88200c4d8",
    "6c1273a2-59f3-4037-af16-d884c9d7ac23",
    "ab82fdf1-6a71-4e09-bffe-ed6446eadc9a",
    "d96c84a8-ce99-4474-94ef-7ae1d1db4a1b",
    "072ccc69-a259-4eeb-b0bc-bf0a7630ad1a",
    "5c2f91e4-9bd5-4b45-bbc4-e69c15c1d14d",
    "82ed8baa-ed21-48ab-b233-3e775e29d446",
    "af92e983-aa27-4dec-b628-2a4d0bbbd2b6",
    "b3ed2d68-5a2d-4432-adb8-f5507861e203",
    "ba139839-28a3-43fe-9cff-b5925eff371e",
    "c32661f6-6baf-4812-92df-5aae17e49e73",
    "c72d552d-3499-445b-a8d5-05d0ff7824f2",
    "d00ab6fd-b475-49ef-b37a-812807879308",
    "da56a037-af73-469c-a941-a2b7c5f290b7",
    "da24ff95-9d07-4e15-9571-9313d6b90f94",
    "e9c339c9-7002-450a-ad81-86ffcc5a7141",
    "f2310937-f481-48e4-9aad-8b2486cafc3d",
    "fb1e0c9f-8c04-4baf-91e6-4688856a14af",
    "fe634065-36f1-4214-a131-7d36a587fba2",
];

#[test]
fn embedded_authoring_instructions_need_no_private_or_hosted_method() {
    for id in LOCAL_AUTHORING {
        let path = format!("nodes/{id}/node.md");
        let text = super::FILES
            .iter()
            .find(|(name, _)| *name == path)
            .unwrap()
            .1;
        for unavailable in [
            "ideation_",
            "sifttext_get_node",
            "SHOTGUN",
            "@ebc1156f",
            "app.sifttext.com",
        ] {
            assert!(
                !text.contains(unavailable),
                "{path}: unavailable {unavailable}"
            );
        }
        // Check concrete narrative UUID targets; illustrative `Name|uuid` is not a target.
        for citation in text.split("[[").skip(1) {
            if let Some((_, target)) = citation.split_once('|') {
                let target = target.split("]]").next().unwrap();
                if target.len() == 36 {
                    let target_path = format!("nodes/{target}/node.md");
                    assert!(
                        super::FILES.iter().any(|(name, _)| *name == target_path),
                        "{path}: absent target {target}"
                    );
                }
            }
        }
    }
    let persistence = super::FILES
        .iter()
        .find(|(name, _)| *name == "nodes/af92e983-aa27-4dec-b628-2a4d0bbbd2b6/node.md")
        .unwrap()
        .1;
    assert!(persistence.contains("fixed baseline"));
    assert!(persistence.contains("Ordinary") || persistence.contains("ordinary Git commit"));
    assert!(persistence.contains("checked `submit`"));
    let publication = super::FILES
        .iter()
        .find(|(name, _)| *name == "nodes/fb1e0c9f-8c04-4baf-91e6-4688856a14af/node.md")
        .unwrap()
        .1;
    assert!(publication.contains("changing an existing resolved leaf"));
    assert!(publication.contains("requires its explicit UUID"));
}

#[test]
fn commit_entries_deliver_one_complete_finishing_contract_and_preserve_history() {
    let file = |path: &str| super::FILES.iter().find(|(name, _)| *name == path).unwrap().1;
    let successor = "e9c339c9-7002-450a-ad81-86ffcc5a7141";
    for id in [
        "c72d552d-3499-445b-a8d5-05d0ff7824f2",
        "f2310937-f481-48e4-9aad-8b2486cafc3d",
        "ba139839-28a3-43fe-9cff-b5925eff371e",
    ] {
        let body = file(&format!("nodes/{id}/node.md"));
        let (current, preserved) = body.split_once("## Preserved earlier procedure\n").unwrap();
        assert!(current.contains("## Current execution"));
        assert!(current.contains("Commit Usable Understanding"));
        assert!(preserved.contains("not as\nadditional operative guidance"));
    }
    assert!(file("nodes/c72d552d-3499-445b-a8d5-05d0ff7824f2/children.order")
        .lines().any(|id| id == successor));
    assert!(super::LINKS.iter().any(|(path, target)| {
        *path == format!("nodes/c72d552d-3499-445b-a8d5-05d0ff7824f2/children/{successor}")
            && *target == format!("../../{successor}")
    }));
    let body = file(&format!("nodes/{successor}/node.md"));
    for required in [
        "## Observed conditions",
        "## Inference and response",
        "### Save and finish the affected understanding",
        "Before candidate handoff or checked publication",
        "including affected unchanged wording",
        "obsolete instructions do not still compete as current guidance",
        "preserve the complete earlier procedure",
        "Proposal-only work ends at review",
        "Native checks establish their declared structural and\nstorage properties, not semantic correctness",
        "Improved behavior\nby a fresh agent requires separate evidence",
    ] {
        assert!(body.contains(required), "missing contract: {required}");
    }
}

#[test]
fn selected_commit_route_defers_capture_and_requires_native_chat_presentation() {
    let node = |id| {
        let path = format!("nodes/{id}/node.md");
        super::FILES.iter().find(|(name, _)| *name == path).unwrap().1
    };
    let entry = node("c72d552d-3499-445b-a8d5-05d0ff7824f2");
    let current = entry.split("## Preserved earlier procedure").next().unwrap();
    assert!(current.contains("Read the child named Commit Usable Understanding"));
    assert!(!entry.contains("commit once, at its end"));
    let selected = node("e9c339c9-7002-450a-ad81-86ffcc5a7141");
    for clause in [
        "carry ordinary mid-chat requests to remember or save something",
        "without interrupting the ongoing task for save approval or asking “now or later?”",
        "An explicit instruction to commit now overrides this end-of-chat timing.",
        "when the human indicates the chat is ending, or its conclusion is otherwise clear",
        "prepare the delta and follow the existing approval or standing-grant boundary",
        "For a context handoff before chat-end, follow the Read procedure's handoff section.",
        "Read this complete procedure and the parent's children named Dialogue",
    ] {
        assert!(selected.contains(clause), "missing timing clause: {clause}");
    }
    let dialogue = node("d00ab6fd-b475-49ef-b37a-812807879308");
    for clause in [
        "directly in ordinary assistant chat, in a fenced text code block",
        "Alongside it, show the exact changed content using contextual diffs or complete replacement bodies",
        "rather than only in a file link, summary, or approval-button label",
        "wait for the human to explicitly authorize the stated delta there",
        "Do not use harness question, decision, or approval widgets for SiftText Commit.",
        "Under an explicit standing write grant, proceed with the reviewed in-scope delta without asking again.",
        "Suspend only the affected work when:",
    ] {
        assert!(dialogue.contains(clause), "missing presentation clause: {clause}");
    }
    assert!(!dialogue.contains("or link the draft file, before any approval control"));
    let entrypoint = node("5e565245-14fe-406c-977c-cf94a9be246b");
    assert!(!entrypoint.contains("rough time estimate"));
    assert!(!entrypoint.contains("at most three questions"));
}

#[test]
fn reconciled_methods_are_selected_and_structurally_readable() {
    let file = |path: &str| super::FILES.iter().find(|(p, _)| *p == path).unwrap().1;
    for (parent, child, name) in [
        ("d00ab6fd-b475-49ef-b37a-812807879308", "d96c84a8-ce99-4474-94ef-7ae1d1db4a1b", "Draft From Originating Conditions"),
        ("da24ff95-9d07-4e15-9571-9313d6b90f94", "ab82fdf1-6a71-4e09-bffe-ed6446eadc9a", "Review Origin and Response Coverage"),
        ("957aa569-75be-4871-b376-6b1943cc4db4", "6c1273a2-59f3-4037-af16-d884c9d7ac23", "Author Navigable Procedures"),
        ("1e1b2959-562b-48a8-8094-8f099c8e3cc4", "10f2d4ab-3318-45d8-9486-f26a817c829e", "Write for the Reader"),
    ] {
        let entry = file(&format!("nodes/{parent}/node.md"));
        let (current, prior) = entry.split_once("## Preserved earlier procedure").unwrap();
        assert!(current.contains(name));
        assert!(!prior.trim().is_empty());
        assert!(file(&format!("nodes/{parent}/children.order")).lines().any(|id| id == child));
        assert!(super::LINKS.iter().any(|(path, target)| {
            *path == format!("nodes/{parent}/children/{child}") && *target == format!("../../{child}")
        }));
        let body = file(&format!("nodes/{child}/node.md"));
        assert!(body.contains("## Observed conditions"));
        assert!(body.contains("## Inference and response"));
    }
    let draft = file("nodes/d96c84a8-ce99-4474-94ef-7ae1d1db4a1b/node.md");
    assert!(draft.contains("A tool-origin finding needs its actual observation"));
    assert!(draft.contains("Do not use harness question, decision, or approval widgets"));
    let review = file("nodes/ab82fdf1-6a71-4e09-bffe-ed6446eadc9a/node.md");
    assert!(review.contains("Review each affected node in both directions"));
    assert!(review.contains("Useful findings beneath a discarded approach retain their own evidential standing"));
    for id in ["82ed8baa-ed21-48ab-b233-3e775e29d446", "c32661f6-6baf-4812-92df-5aae17e49e73"] {
        let text = file(&format!("nodes/{id}/node.md"));
        let (current, _) = text.split_once("## Preserved earlier guidance").unwrap();
        assert!(!current.contains("Keep useful discoveries outside discarded ancestry"));
        assert!(!current.contains("Keep reusable measurements under a non-discarded owner"));
    }
    let read = "5c2f91e4-9bd5-4b45-bbc4-e69c15c1d14d";
    let interaction = "31dc46ab-eaf1-4f1a-97ed-5ff88200c4d8";
    assert!(file(&format!("nodes/{read}/node.md")).contains("read the child named Develop Understanding Through Interaction"));
    assert!(file(&format!("nodes/{read}/children.order")).lines().any(|id| id == interaction));
    assert!(super::LINKS.iter().any(|(path, target)| {
        *path == format!("nodes/{read}/children/{interaction}") && *target == format!("../../{interaction}")
    }));
    assert!(file(&format!("nodes/{interaction}/node.md")).contains("Do not infer refusal from silence"));
}

#[test]
fn embedded_surfaces_and_methods_match_local_capabilities() {
    let node = |id| {
        let path = format!("nodes/{id}/node.md");
        super::FILES
            .iter()
            .find(|(name, _)| *name == path)
            .unwrap()
            .1
    };
    for (id, required, obsolete) in [
        (
            "488a3d9f-3d7f-4208-957c-211bd43d50e9",
            "neither automatically injects",
            "Push guarantees delivery",
        ),
        (
            "acfc85d8-04b8-47b8-90de-6a6443bf41e2",
            "Core supplies no web UI or assistant",
            "humans browsing the web UI",
        ),
        (
            "17c42051-88c0-4e41-a151-5d1bab4be83b",
            "**Stuck**",
            "five statuses",
        ),
        (
            "e0198b59-2371-470a-94dc-dec5ea98ad67",
            "required dependencies",
            "These are not dependencies",
        ),
        (
            "5f0d176d-c68a-4755-94c6-a461415a1835",
            "Local Core Agent Entrypoint",
            "Antifragile Probing|",
        ),
        (
            "81b3ee00-4320-4dd7-9920-34480f8431e2",
            "owning workflow's conversational ledger",
            "under Outline & Continuation",
        ),
        (
            "991e877a-20fe-403d-96f6-75a80436a405",
            "do not require an absent external",
            "the linked reference-refresh law",
        ),
        (
            "e8a42273-9959-4451-8350-f656cc6e9d3f",
            "observation checkpoint before building",
            "Apply the linked Glass-Box",
        ),
        (
            "feb468af-0cc6-4190-b3f8-cc854dcb8cd1",
            "installs no Decision Models program",
            "The linked Decision Models program",
        ),
    ] {
        let text = node(id);
        assert!(text.contains(required), "{id}: missing {required}");
        assert!(!text.contains(obsolete), "{id}: obsolete {obsolete}");
    }
    // All concrete narrative citations must be readable in this emitted forest.
    for (path, text) in super::FILES.iter().filter(|(p, _)| p.ends_with("/node.md")) {
        for citation in text.split("[[").skip(1) {
            if let Some((_, target)) = citation.split_once('|') {
                let target = target
                    .split("]]")
                    .next()
                    .unwrap()
                    .split('@')
                    .next()
                    .unwrap();
                if target.len() == 36 {
                    let target_path = format!("nodes/{target}/node.md");
                    assert!(
                        super::FILES.iter().any(|(p, _)| *p == target_path),
                        "{path}: absent {target}"
                    );
                }
            }
        }
    }
}
