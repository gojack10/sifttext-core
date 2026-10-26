---
id: "fb1e0c9f-8c04-4baf-91e6-4688856a14af"
name: "Worktree Publication"
micro_summary: "Explains how isolated knowledge edits reach accepted main through validation and reconciliation, with recoverable history and explicit stop paths."
status: "in_progress"
see_also: [{"target":"dcdd97b3-9557-41e3-b7cb-1f6f9744267d","reason":"Required repo commit conventions before staging and committing local knowledge."},{"target":"c72d552d-3499-445b-a8d5-05d0ff7824f2","reason":"Local proposal, approval and verification discipline before authored changes."}]
scope: "Carry an authorized local edit through proposal, private Git work, validation, review, publication and recovery without overwriting another owner."
warnings: "Cooperative isolation, not an OS sandbox. One publisher owns accepted knowledge at a time; no force, reset, automatic push, or overwriting another worktree."
---
## Authority and private draft

Follow SiftText Commit and its required reads before knowledge edits. The human-facing agent reconstructs and reviews the affected delta, applies explicit approval or an explicit standing write grant within its scope, and verifies affected meaning. A concrete already-authorized edit needs no repeated approval. A bounded worker returns its assigned candidate to its assigning agent; it does not infer publication or push permission. The human may stop or reshape the undertaking without waiving unrelated permissions or preservation checks.

Set `home` to the independent knowledge repository, not a software checkout. Inspect status, worktrees and ownership; preserve unknown or dirty work. Reuse an owned draft for the same undertaking. Allocate only when editing needs a new draft:

```sh
git -C "$home" status --short
git -C "$home" worktree list
sifttext worktree "$home"
```

Use the returned `worktree` as `wt` and immutable `base` SHA as `base_sha`. No separate baseline checkout is needed. Allocation starts from accepted main even when the normal home is dirty or behind; it does not change that home. After uncertain allocation, inspect registered worktrees before retrying. Read Knowledge Layout's Node artifacts section before placing task files: `artifact_home` is durable, `wt` is disposable.

## Edit, inspect and commit

Edit ordinary native files only in `wt`. The optional `create` helper supplies UUID/layout/relationship bookkeeping, not collection validation or global status normalization. Native files with valid UUIDs and layout do not require that helper. Drafts may be incomplete while editing. Create targets before references, preserve complete child ordering and stable UUIDs, and delete last. Never overwrite another contributor's paths or replace a full child list with a partial one.

Before staging/committing, read [[Git Commit Skill|dcdd97b3-9557-41e3-b7cb-1f6f9744267d]]. Inspect the actual files and complete diff, stage only intended authored paths, and commit:

```sh
git -C "$wt" status --short
git -C "$wt" diff
# git -C "$wt" add -- <the inspected authored paths>
git -C "$wt" diff --cached --check
# git -C "$wt" commit -m 'knowledge: describe the approved change'
candidate=$(git -C "$wt" rev-parse HEAD)
```

Never stage `.sifttext/`, generated metadata/outlines or task artifacts. A worker returns the candidate SHA, base SHA, paths, reviewed meaning and open issues here unless publication was separately authorized. A private commit is not accepted knowledge.

## Submit the exact combined candidate

The authorized publisher coordinates writers, preserves the original arguments, and submits the clean committed draft:

```sh
sifttext submit "$home" "$wt" "$candidate" "$base_sha"
```

Append only full UUIDs for explicitly authorized status requests. Do not list every edited node: editing an existing `not_examined` node activates it automatically; reopening an existing resolved leaf requires its explicit UUID. Resolved parents reopen automatically when an active direct child blocks them. Nodes absent from the base are creations; a new leaf may remain `not_examined`. Resolution with active children must fail.

Submit combines current accepted main with the candidate in the draft, performs mechanical status normalization, and gates the exact combined Git candidate on Bend structural laws, required fields, status/transition rules and the 5000 `o200k_base` body-token ceiling. It then conditionally advances accepted main. Normalization may create an attributed commit; integration retains committed history and its attribution. A changed main, conflict or failed rule returns to ordinary Git repair, review, commit and resubmission. Do not bypass the gate or change frozen limits. There is no standalone `check` command or required preflight normalization. Optional `normalize BEFORE WORKTREE [EXPLICIT_UUID ...]` is for deliberate mechanical editing with a known baseline directory, not acceptance; inspect its actual writes and any partial failure before retrying.

The receipt reports `semantic_check:deferred`, not semantic approval. Agents remain responsible for evidence, meaning and authority. Raw Git can bypass this cooperative workflow; it is not OS enforcement.

Successful customer receipts are `status:saved` or `status:recovered`, with `accepted`, knowledge `version`, `home` and `worktree_removed:true`. Internal `current` is not a substitute for inspecting the customer completion receipt. Completed submission advances the normal home, refreshes SQLite/search/outlines and removes only the saved draft and its temporary branch. Verify accepted identities and affected source from `home`; no ritual `advance` or `index` is needed. Allocate a new draft only for the next authorized edit.

## Refusal, repair and recovery

Preserve the draft, diagnostics, original submit arguments and Core's pending journals. A failure may mean `not_accepted`, `accepted_incomplete` or `unknown`; an exit failure does not prove nothing was saved. If main is already accepted but a dirty home, retrieval refresh or cleanup blocks finishing, report the accepted SHA separately and preserve the dirty work. Never reset accepted history or manually delete journals/caches to clear a refusal.

For interruption or recorded preparation, retry the original arguments so Core can resume preparation or finish already-accepted work without republishing it. For an unaccepted candidate needing changes, inspect the actual prepared draft tip and Git state, repair within the grant, commit the repair, then resubmit its actual explicit commit with the original base and intended explicit UUIDs. Follow pending-request diagnostics if that request cannot yet proceed; do not invent acceptance or erase custody records. A repair must preserve prior prepared work, not reset back over it. Revisit meanings or ask the owner only when the repair exceeds existing authority.

Resolve Git conflicts in the retained draft, inspect affected nodes, finish the native merge and commit. Invalid fields, missing reasons, relationships or excessive body tokens need authored repair, not truncated evidence or cache edits. Non-chat artifacts in a draft refuse publication/cleanup: preserve them and follow Knowledge Layout's deliberate durable-home placement procedure. Inspect an uncertain partial operation before retrying so successful work is not duplicated. Missing authority, unsafe paths or unavailable external prerequisites stop dependent work; continue independent permitted work.

## Accepted reading and completion

`accepted HOME` returns a separate checked detached view (`status:ready`, `accepted`, `version`, `view`, `home_advanced:false`). Read that view when the normal home cannot move. `advance HOME` directly advances an eligible clean detached reader to already-accepted main and refreshes retrieval; it does not resubmit knowledge. Inspect ownership first. Dirty, divergent, branch-owned, linked or unsafe readers are preserved. A finishing failure can follow reader movement: retain files and retry without rollback. Only verified invocation-owned clean views may be retired; unknown or edited views remain.

For stale retrieval use `index STORE`, which refreshes the current parseable draft without certifying publication. Generated-only residue is retired only with verified cache ownership and safe native paths; unknown, corrupt, unsafe or artifact-bearing directories remain for investigation. Never delete arbitrary leftovers to obtain a clean result.

To undo accepted work, allocate a new draft at then-accepted main, inspect the offending commit, apply a reviewed `git revert --no-commit SHA`, inspect/stage/commit the intended authored changes and submit the compensating commit. Never reset main. Rehome durable artifacts deliberately before removing their owner; empty artifact containers are still protected.

Return actual paths, relationship/status changes, candidate/accepted identities, customer receipts and remaining recovery work. Verify landed fields and affected continuation against the authorized delta. A diff review, commit, deterministic gate, accepted save and fresh-agent use are distinct evidence; a text walkthrough proves no behavioral improvement.
