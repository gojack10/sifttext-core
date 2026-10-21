# CLI output cull: consequential information, not receipts of effort

## Contract

A command should return only what an LLM needs to understand its outcome, locate the
relevant source, or choose the next safe action. Preserve validation, exact requested
evidence, guardrails, permission and mutation/recovery state. Delete duplication before
compressing it. A small success result does not mean fewer checks ran.

This audit covers **every CLI operation, including SQL**. Names and micro-summaries stay
intact in search; child counts stay in outlines. `read` still returns exact original
source bytes. No new output framework, dependency, flags or silent result truncation.

Implemented in source on local `main`; this is not an installed upgrade or upstream
release. No user knowledge home, credentials or session archives were modified by this
work. Verification uses isolated synthetic homes and mocked account/update services.

## Measured examples

Counts use `o200k_base` ordinary encoding. They are samples, **not maxima**. UUIDs, hash
digits, paths, warnings and JSON whitespace affect counts. Where fixture inputs differ,
the comparison is explicitly illustrative rather than identical-input measurement.

| Operation / scenario | Before | After | Evidence boundary |
|---|---:|---:|---|
| `check`, 904 nodes pass | 54,644 | 44 | Earlier serializer sample versus real CLI fixture; not identical hashes |
| `check`, one failure among 904 nodes | All successful rows mixed in | 155 | Actual refusal output; independent failures remain |
| `submit`, success | 87,620 at 904 nodes | 133–137 at 1,201 nodes | Different fixtures; success size no longer scales with validation rows |
| `accepted`, success | 87,509 at 904 nodes | 148–152 at 1,201 nodes | Different fixtures, same bounded-size projection principle |
| `normalize`, no own writes | 33,102 at 904 nodes | 10 | Earlier serializer shape versus real CLI output |
| `normalize`, one change among 1,025 nodes | Whole status map + duplicate lists | 60 | Actual CLI; unchanged nodes no longer returned |
| Search, deepest 20 hits in a 904-node chain | 757,690 | 1,840 | Same twenty hits; 17,890 ancestor rows reduced to 20 matches |
| SQL, 1,000 homogeneous integer rows | 7,067 | 4,058 | Same requested data, types and values |
| Full outline, 904-node wide tree | 40,135 | 38,320 | Same fixture; names/summaries/children deliberately retained |
| Shallow outline, same wide tree | 41,946 | 38,328 | Same fixture; no silent omission of children |
| `update --status`, journal with 1,000 changed paths | 19,311 | 192 | Same fixture; persisted journal remains intact |
| `account`, synthetic paid fixture | 49 | 22 | Same fixture; no real account accessed |
| `create`, child fixture | 92 | 68 | Same fixture; actual ancestor activation notices remain separate |
| `session-backup` | 40 | 34 | Same commit fixture |
| `session-capture`, unavailable | 17 | 11 | Same outcome; null commit removed |
| `update --config` | 33 | 26 | Same fixture path |
| Opportunistic update notice | 91 | 74 | Same release/action fixture |

The committed serializer probe also reproduces an identical-input comparison with
realistic deterministic fingerprints: 904-node successful check **54,673 → 48** tokens,
accepted sample **87,550 → 100**, and the same deep search hits **757,719 → 1,869**.
Its compact submit receipt is 98 tokens at 1, 100 and 904 nodes. Those differ from the
real CLI fixture table because its paths and identifiers are different.

Do not promise an enormous reduction where useful content dominates. Outline savings
are modest precisely because authored names, summaries, hierarchy, child counts and
warnings remain. SQL payloads explicitly requested by the query remain exact.

## Complete cut / no-cut inventory

| Operation | Delivered result and cut decision |
|---|---|
| Help / no args | Fixed command reference; minor duplicated update/override help removed. Safety guidance retained. |
| `check` | Success only nodes/status/version. Failure-only actionable rows/locations/measurements; no successful token rows, tokenizer provenance, repeated rule IDs, null-only fields or irrelevant baseline. Rich internal validation unchanged. |
| `normalize` | Own-write-only JSON: unchanged or normalized plus one change per written node, before/after and removed reason-field names. No full status map, duplicate ID lists or duplicate stderr announcements. Sequential write failure preserves partial/unknown progress; exact receipt checked before writes. |
| `submit` | Saved/recovered outcome, accepted commit, knowledge version, home and draft-removal result. No successful nested validation, cache counters, duplicate identities or finishing subreceipts. Accepted-incomplete versus not-accepted versus unknown distinguished. Semantic findings preserved when nonempty. |
| `accepted` | Ready, accepted commit, knowledge version, usable view path, home_advanced=false. No misleading published flag or successful validation dump; retained view remains usable. |
| `advance` | Advanced/current, one accepted commit, version and home. Removes duplicate hashes/boolean. Refresh failure after reader movement is not reported as no change. Retires only the invocation-owned clean detached temporary accepted view, with physical-directory and repository checks; preserves unknown/dirty/replaced copies. |
| `worktree` | Only private worktree path and immutable base. Branch stays internal. Partial allocation failure identifies possible draft/baseline and preserves files. Guidance reuses owned drafts rather than allocating per turn. |
| `search` | Ranked hits only, IDs, intact names/micro-summaries, status, compact readable root/parent IDs, useful query-specific excerpt, total matches, version and path template. Removes expanded ancestor rows and repeated paths/reasons/constants. Applicable warnings once per readable source; warnings over 512 tokens become explicit mandatory source reads, never clipped instructions. |
| `outline` | Keeps requested hierarchy, IDs, intact names/summaries, statuses/reasons/warnings, child counts, coverage, path template and authored knowledge version. Removes descendants, tokenizer/display fingerprint metadata and leaf expansion costs. Shallow expandable branches retain exact full-CLI token estimates. Internal artifacts/identities/verification remain rich. |
| `sql` | Exact requested cells, column names/order/empty schema and precision. Removes scope/count duplication; homogeneous numeric column types declared once, mixed-type cells retain distinctions. No inserted limit, truncation, dedup or summarization. Read-only and query resource bounds retained. |
| `index` | Only nodes/status/version. Removes constant scope and reference count. Normal retrieval already refreshes: no ritual index-before-search/SQL/outline. Not advertised as guaranteed clean rebuild. |
| `read` | **No success cut:** original node.md bytes. Full verification and unfamiliar fields preserved. Inherits compact rejection diagnostics. Oversized authored content is not silently hidden. |
| `init` / `--bundle` / `--empty` | **No receipt cut:** home, created flag, initial commit, copied-node count and pointer outcome distinguish consequential setup states. Existing homes preserved. Empty setup does not claim working instructions. |
| `instructions` | **No cut:** checked version and exact entry path. |
| `create` | Only uncommitted node ID and path; parent echo and constant committed=false removed. Internal outcome and checks intact. Real ancestor activation notices retained because they describe additional writes. |
| `runtime-check` | **No cut:** verified runtime location and status. |
| `source-code` | **No cut:** exact source revision, detached checkout path and maintenance instructions each serve custody/use. |
| `update` / `-y` | Omits empty refreshed-path lists; retains actual changed paths, actionable conflict locations and consent. Full persisted state/recovery journal unchanged. |
| `update --status` | Recorded application/knowledge/home, activation-pending and release-check outcomes, actionable available release and unfinished operation/resume identities. No full journals/manifests, repeated checksums/baselines/timestamps/nulls or path-per-change dump. Recorded identity is not proof activation finished. |
| `update --config`, `--grant`, `--mode`, `--pin`, `--unpin` | Mode, permission, settings path, real pin if set. Internal source label and null pin omitted. Consent/pin semantics unchanged. |
| `update --resume` / `--rollback` / `--rebuild` | Meaningful progress, chosen identities and retained recovery/build locations preserved. Only empty refreshed-path lists cut where applicable. |
| Incidental update notices | Release identities/action/guide/consent/pin remain; repeated heading/settings path removed. Checking/activation policy unchanged. |
| `login` / `account` | Compact allowlisted account ID/email/subscription status/paid outcome; removes display name/username and pretty-print padding. Authorization URL, payment gate and credential protections retained. |
| `logout` | Local removal and remote-revocation uncertainty retained; repeated reassurance about unchanged knowledge removed. |
| `session-capture` | Success custody key/commit retained; unavailable result drops commit:null. No transcript dump. |
| `session-attach` | **No cut:** node, association name and archive key identify distinct custody facts. Existing pre-mutation output gate retained. |
| `session-backup` | Status and sent commit retained; constant destination_configured=true removed. No private destination disclosure. |
| `session-restore` | **No cut:** status/link count, not per-link dump. Preservation and ff-only safeguards retained. |
| `session-retention-config` | **No cut:** consent/location/backup consent/destination-configured are distinct settings. No secret destination disclosure. |
| Internal `install-home` | **No cut:** small initialized/existing outcome and identities retained; application/knowledge separation unchanged. |

## Reproduce and budget honestly

```sh
sh scripts/with-tools.sh cargo run --locked --example cli-output-budget
# Also count exact saved CLI output files, including wrappers and stderr if captured:
sh scripts/with-tools.sh cargo run --locked --example cli-output-budget /tmp/output.txt
```

The example uses production check, publication, normalization and search projections
with deterministic synthetic identifiers. It asserts essential sample invariants; it
never opens a store, writes files, runs Git or contacts a service. It deliberately does
not duplicate the private outline or SQL renderer. Their committed runnable checks are
`retrieval_output_budget`, `outline_output_cli` and `sql_output_cli`. Use
`scripts/build.sh` first when source/build fingerprints change.

For planning before exact measurement:

- Compact UUID-heavy diagnostic/change row: allow about **100 tokens**.
- Pretty JSON diagnostic/change row: allow about **150 tokens**.
- Add complete authored text **each time actually displayed**, including warnings.
- Successful check/publication/advance/index receipts no longer grow per checked node.
- A safe loose bound for a concrete rendered output is **tokens <= final UTF-8 bytes**.
  Count after escaping/repetition; English `characters / 4` is not a worst-case bound.

These per-row allowances are heuristics, not proofs. Full names, summaries, SQL cells,
headers, remote account fields and subprocess errors can exceed them arbitrarily within
their own input/resource constraints. A read's successful source file is capped at
1,048,576 bytes, not 5,000 total tokens; the body ceiling excludes its header.

There is still **no universal CLI-wide output maximum**. A 904-node wide outline can
still be ~38k tokens. Genuine failures/changes and explicit broad SQL can still be huge.
Search no longer multiplies all metadata by every hit's entire ancestry in its CLI
output, but internal rich evidence remains and huge names/summaries stay intact.

Guarded returns over 15,000 tokens require `-y`; that is not a global stdout+stderr cap
or proof of validity. Direct normalize checks the exact receipt before writing. Successful
publication exposes its compact completion even after a save. Ordinary errors and some
incidental notices remain outside that limiter. `SIFTTEXT_NO_UPDATE=1` isolates commands
from incidental update notices for measurements; normal managed invocations can include them.

SQL's warning remains exactly:

```text
[<tokens> tokens suppressed] Re-run this sifttext sql command with -y to print them.
```

Narrow SQL at the source: count when a count is needed, select required fields, filter,
and explicitly limit discovery. Never silently rewrite a requested evidence query.
Start outline navigation shallow and expand selected branches. Reuse unchanged reads,
continue an owned draft, and do not repeat advance/index after a completed submit.

## Verification boundaries and follow-ups

After merging all four original worker branches, the combined run passed **41 selected
binary/CLI/native tests** and **301 library tests**; **3 library failures and 4 ignored
tests** remained. The failures were the two baseline literal-phrase mismatches and the
baseline policy-helper injection failure listed below. A subsequent bounded integration
repair shares failure-only diagnostics between direct checks and nested rejected
publication receipts; its worker passed 9 affected CLI/projection tests, 40 selected
library checks and 1 ignored timing probe. Final main verification of that repair is
reported with delivery; do not reinterpret the broad run as a green complete suite.

Worker reports preserve actual output measurements and native custody/recovery checks.
Internal outline oracle verification was retained rather than replaced by size assertions.
An isolated integrated smoke test also initialized and checked all 90 bundled nodes,
verified exact source read bytes, and exercised instructions/index/SQL/worktree. No user
knowledge home was used.

The pre-integration library baseline was not green: two stale literal-phrase checks in
`default_native_kb.rs`, a policy-helper signal/timeout injection failure, and a process
stderr-classification failure were observed. Some process/lock failures vary across runs.
These must not be relabeled as fixed or silently skipped to claim a green full suite.
The earlier default CLI test's JSON-versus-LEAN consumer mismatch was repaired as part of
migrating the affected retrieval contract; its end-to-end preservation checks remain.

The separately advertised `update --attach-node=UUID` has a pre-existing parser/help
mismatch. Its underlying API receipt needs no cut; adding CLI parsing is a separate
functional repair, not smuggled into this output cull. No live cloud login, installation,
upstream release or crash-proof universal cleanup guarantee is claimed.
