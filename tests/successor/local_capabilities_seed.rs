// Bounded conceptual slice; onboarding and client guides have separate coverage.
#[test]
fn conceptual_seed_separates_core_from_external_agents() {
    for id in [
        "045dd71e-24ae-4125-8957-20ece3d3f7a5",
        "41ea7efb-dbf2-4739-9d5e-3b6758a04865",
        "42b1cde8-5478-4b14-938f-c10869ffda03",
        "537dcdb9-d9ac-4d97-9f8d-f50dfe3487a5",
        "ec26e21e-bd1c-40e6-89b1-a1daafc10851",
        "f2d4825e-7e98-4775-b363-c50de7ee62ed",
    ] {
        let path = format!("nodes/{id}/node.md");
        let text = super::FILES.iter().find(|(p, _)| *p == path).unwrap().1;
        assert!(!text.contains("SIFT"), "{path}: built-in assistant claim");
        assert!(!text.contains("app.sifttext.com"), "{path}: hosted setup");
        assert!(text.contains("Core"), "{path}: local capability boundary");
    }
    let node = |id| {
        let path = format!("nodes/{id}/node.md");
        super::FILES.iter().find(|(p, _)| *p == path).unwrap().1
    };
    assert!(node("41ea7efb-dbf2-4739-9d5e-3b6758a04865").contains("not a checked enum"));
    assert!(node("537dcdb9-d9ac-4d97-9f8d-f50dfe3487a5").contains("rejects an authored `ruled_out` field"));
    assert!(node("42b1cde8-5478-4b14-938f-c10869ffda03").contains("failed check stops dependent work"));
}
