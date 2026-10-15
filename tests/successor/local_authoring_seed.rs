// These bounded checks cover localized authoring and surface contracts, not all bundle claims.
const LOCAL_AUTHORING: &[&str] = &[
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
