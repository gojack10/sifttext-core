# Parent-owned hosted acceptance

Local/offline Core remains independent. A hosting parent can set `SIFTTEXT_HOSTED_GATE_FD` to a fresh inherited Unix stream descriptor (at least3). The parent must own the executable/application, authorization context and opposite endpoint; a user-set environment variable or saved receipt is not a remote proof.

`hosted-gate-protocol` verifies the normal checked runtime and returns `hosted_gate:1`. A parent must require this before invoking acceptance; an older executable must never be allowed to publish and only afterward be found incompatible.

After Core validates the exact immutable final normalized combined candidate, but before conditional ref acceptance, it sends an eight-byte unsigned big-endian frame length followed by UTF-8 JSON:

```json
{"binding":{"protocol":1,"commit":"FULL_COMMIT","tree":"FULL_TREE","version":"CORE_VERSION","baseline_tree":"BASELINE_TREE_OR_NULL","recovery":false},"nodes":[{"id":"NODE_ID","text":"EXACT_CHANGED_NODE_MARKDOWN"}]}
```

`baseline_tree` is JSON null for initial immutable `accepted`; all nodes in that selected view are then included. A normal submit includes only new/changed node bodies relative to accepted main, after combination and normalization. Artifacts and unrelated filesystem content are not enumerated. `recovery:true` means already-accepted submit completion, not permission to automatically check an unchecked backlog.

The parent responds with the same framing and exact `binding`, plus `check` using the existing hosted-check result shape: status `passed`, `findings`, `unchecked`, or `not_required`, with actual per-node `results` where available. Findings reject and preserve the draft. A parent may explicitly return unchecked for unavailable checking or no credits; a missing/malformed channel, binding mismatch or unsupported outcome refuses acceptance. Core never reads a persisted marker to authorize this path.

The parent must durably bind its server-owned record to account, candidate, baseline, context and checker versions before responding. It supplies its own account authority outside the channel; no credentials enter the candidate protocol. Core closes the descriptor on subprocess exec. The native CLI retains a single parseable JSON transaction receipt with non-content hosted metadata; the parent retrieves retained findings by request ID. Ordinary local CLI output remains unchanged.

This channel supplies no graph semantics, login, billing, provider implementation or deployment configuration. A guest writable by arbitrary Bash is not a trusted parent. Hosted shared acceptance must re-run this boundary on guest exports regardless of guest status/receipts.
