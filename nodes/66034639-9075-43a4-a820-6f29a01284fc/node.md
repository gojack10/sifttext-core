---
id: "66034639-9075-43a4-a820-6f29a01284fc"
name: "Writing Useful Tests"
micro_summary: "Guides test maintenance toward stable behavioral invariants, with recent test-file churn as a signal for fixture, rewrite or deletion review."
status: "resolved"
scope: "How should agents write, assess and maintain software tests so they establish intended behavior as implementations evolve?"
see_also: []
---
## Operating rule
Write tests that establish intended behavior, invariants and independently grounded expected results. Identify what meaningful behavior each test establishes. Use fixtures and assertions that allow unrelated implementation details to evolve without rewriting the test's meaning.

Review Git history, emphasizing recent changes, to spot high-churn tests. Investigate whether churn reflects changed requirements, brittle fixtures, unnecessary coupling to implementation details or weak behavioral coverage. Use churn to select reviews, not as a deletion verdict.

Improve fixtures to absorb incidental API/signature changes. Rewrite tests to verify meaningful behavior. Delete obsolete, redundant or implementation-coupled tests when they provide insufficient value. Maintain useful regression evidence through an appropriate replacement or retained independent check. Expected results follow intended behavior and independent evidence, not the current implementation's output.

After changes, run relevant checks and record consequential test-maintenance reasoning with the owning work. Apply this skill within the project's actual verification needs; a green subset establishes only its tested behavior.

## Origin and evidence
Jack accepted this as reusable testing guidance on October 13, 2026 after sharing Kyle Mistele (@0xblacklight)'s October 12 post about high-churn tests, brittle fixtures and invariant-based tests. The post supplies a heuristic and rationale, not a measured general rule that churn predicts low value. Fresh-agent application and downstream quality effects remain untested.