---
id: "fb1e0c9f-8c04-4baf-91e6-4688856a14af"
name: "Worktree Publication"
micro_summary: "Explains how isolated knowledge edits reach accepted main through validation and reconciliation, with recoverable history and explicit stop paths."
status: "in_progress"
see_also: [{"target":"dcdd97b3-9557-41e3-b7cb-1f6f9744267d","reason":"Required repo commit conventions before staging and committing local knowledge."},{"target":"c72d552d-3499-445b-a8d5-05d0ff7824f2","reason":"Local proposal, approval and verification discipline before authored changes."}]
scope: "Carry an authorized local edit through proposal, private Git work, validation, review, publication and recovery without overwriting another owner."
warnings: "Cooperative isolation, not an OS sandbox. One publisher owns accepted knowledge at a time; no force, reset, automatic push, or overwriting another worktree."
---
## Proposal and permission

The human-facing agent recovers the assignment and reads affected nodes, their owner and necessary constraints. A bounded worker preserves its assigned fence and returns to its assigning agent. SiftText Commit remains the write discipline: reconstruct the intended tree delta, repair it, obtain explicit approval, persist only that delta, reconcile affected state, then verify actual landed fields. A general request to discuss an improvement is not persistence permission. A concrete already-authorized creation/edit need not be approved again.

Before new nontrivial content is persisted, show the affected tree with unchanged context and operation markers (`[+]` create, `[!]` edit, `[→]` move, `[←]` link, `[-]` delete) and effective statuses. Show exact instruction text or a readable diff, destinations and consequences. Separate proposed decisions from observations. Check ownership, independently meaningful questions, stable summaries, links, statuses and evidence; preserve failed attempts and unaffected work. Do not silently expand scope. For an instruction pipeline, walk entry, required reads, roles, action, output, return, failure and human-override paths before asking approval.

Read the local SiftText Commit hub and its required children with ordinary file tools before edits, and applicable instructions in the assigned project. Before staging/committing, read [[Git Commit Skill|dcdd97b3-9557-41e3-b7cb-1f6f9744267d]] in this repository. This package does not authorize remote writes, replace a richer local project method or grant publication/push permission. Report missing authority rather than inventing it; the workflow below requires no hosted access.

## Private draft and fixed baseline

Set `home` to the accepted knowledge repository. Inspect status, worktrees, current branch and HEAD. Preserve any unknown/dirty work. Confirm who may publish; never assume another agent's ownership has expired.

```sh
git -C "$home" status --short
git -C "$home" worktree list
git -C "$home" rev-parse HEAD
sifttext worktree "$home"
```

Use the returned `worktree` as `wt`, its `branch` as `branch`, and its `base` as `base_sha`; do not edit accepted home. Preserve a detached baseline of that exact commit for transition checks:

```sh
base=$(mktemp -d "${TMPDIR:-/tmp}/sifttext-base.XXXXXXXX")
rmdir "$base"
git -C "$home" worktree add --detach "$base" "$base_sha"
```

Apply only the approved delta in `wt`. Use CLI creation where useful and ordinary native file edits otherwise. Create structure before references/content depending on it, set terminal statuses only when justified, and delete last. Preserve stable UUIDs. Never replace an existing ancestor's full child list with a partial starter's list.

After edits, run native status normalization against the fixed baseline, then the complete candidate check:

```sh
sifttext normalize "$base" "$wt"
sifttext check "$wt" "$base"
```

For an explicitly approved status transition, append each full affected UUID to BOTH commands. Do not pass every edited node as explicit: an authored edit activates an existing `not_examined` node automatically, but changing an existing resolved leaf to `in_progress` requires its explicit UUID. Resolved ancestors reopen automatically only when an active direct child blocks them. Nodes absent from that baseline are treated as creation, not an edit; a newly created leaf can remain `not_examined` even with a body. A requested resolution with active children must fail. Review normalization's actual status changes; it is not semantic approval. `check` covers graph, authored fields, statuses/transitions, token ceilings and source consistency, but cannot judge truth or grant permission.

If invalid, keep the draft, inspect the error, repair only within scope and rerun. Never skip the check or change frozen limits. For a small unintended uncommitted edit to a file already tracked at HEAD, after inspecting the diff, restore only your named file with `git -C "$wt" restore --source=HEAD -- nodes/UUID/node.md`. For a new untracked node, HEAD has no file to restore: reverse your own edit with ordinary file tools or a saved pre-edit copy, then recheck. Do not restore unrelated paths or erase untracked work blindly.

## Commit and handoff

Inspect the full diff, normalize/check again after any change, and stage only intended authored paths. Never stage `.sifttext/`, generated metadata or outlines. Use Scoped Commit titles (`knowledge: describe the change`, under 100 characters); no automatic push.

```sh
git -C "$wt" diff --stat
git -C "$wt" diff
git -C "$wt" status --short
# git -C "$wt" add -- <the inspected authored paths>
git -C "$wt" diff --cached --check
# git -C "$wt" commit -m 'knowledge: describe the approved change'
```

Record the candidate SHA and actual check receipts. A worker stops here and returns the SHA, paths, baseline, evidence and open issues. Do not call a private candidate published.

## Checked submission, only when authorized

The publisher checks ownership and clean home/draft state, then rechecks the committed candidate against its fixed baseline. Use the canonical independent home with its reader detached; initialized homes already satisfy this. A checked-out accepted branch or unsupported reader must be resolved with its owner before submission. Current accepted main may have advanced through another checked submission. Stop editing the draft during submission and coordinate any other home writer.

```sh
candidate=$(git -C "$wt" rev-parse HEAD)
sifttext check "$wt" "$base"
sifttext submit "$home" "$wt" "$candidate" "$base_sha"
```

Repeat approved explicit UUID arguments on `submit` for approved status transitions. Core combines current accepted main in this same submitted worktree, checks the exact combined graph, authored fields, statuses, token limits and source identity, refreshes derived retrieval, and conditionally advances accepted history. A fast-forward reuses the submitted commit. Divergence creates an integration commit preserving its selected author/co-author policy from Git Commit Skill. Existing committed history keeps its attribution.

Success completes the save: the normal home matches accepted history, SQLite/search/complete LEAN outlines match that knowledge, and the submitted worktree and temporary branch are removed. Require a current receipt with `home` and `view` naming that home, `worktree_removed:true`, and the exact `candidate`, `tree` and `version`. `check` distinguishes new acceptance (`published_checked_candidate`) from recovery (`accepted_revision_recovered`). Inspect those identities, then read from `home`:

```sh
git -C "$home" rev-parse HEAD refs/heads/main
sifttext search "$home" 'the saved question'
sifttext worktree "$home"  # Fresh draft for the next approved contribution.
```

Do not keep editing the old `wt`; allocate a new draft for the next edit. Remove only your own clean disposable baseline after its checks and result are retained. Other agents' drafts remain intact.

### Refusal and interrupted recovery

A file conflict remains in the submitted draft for repair. Inspect its Git conflict and the affected nodes; resolve within the approved intent, or ask the owner when meanings conflict. Finish the draft's Git merge, run fixed-baseline normalization/checks and commit any resulting changes, then submit its actual clean HEAD. Invalid combined knowledge, changed source/ref identities, dirty or unregistered drafts, unrelated baselines and unsafe paths also refuse. Preserve unaccepted work and inspect the receipt before choosing repair; never force, reset accepted history or bypass native checks.

An interruption can leave a prepared combined draft before acceptance, or accepted history with home refresh/cleanup unfinished. Preserve the original submit arguments and Core's pending-operation records in the home's Git directory. Retry that exact `submit` so Core can recheck recorded preparation or finish accepted home/derived recovery and cleanup. An already-accepted revision is recovered without another publication. Until completion returns, report the accepted revision and unfinished home/cleanup separately. Preserve a dirty or unexpectedly changed home/draft and return its refusal to the owner.

`accepted HOME` remains a separate checked detached-view inspection/recovery command. `advance HOME` remains an explicit ff-only refresh for an eligible clean detached home. Normal successful `submit` completes home advancement itself. For an independently requested `advance`, inspect ownership and status first; preserve dirty, divergent, branch-owned, linked or unsafe readers, and retry only the recorded recovery after an interruption. Core's cooperative locks do not prevent arbitrary external Git writers.

## Recovery and completion

For a lost/stale index run `sifttext index "$store"`; it rebuilds from authored files without importing knowledge. Do not delete an unknown cache or restore source files as a speculative repair. Inspect source/check errors first.

To undo accepted work, create a new private worktree and a new detached baseline at the then-accepted HEAD, inspect the offending commit, use a reviewed `git revert --no-commit SHA`, then normalize, validate and publish a new compensating commit against that baseline through this procedure. Never reset accepted history. Restore content deliberately; status normalization may correctly differ from historical status. Git may leave ignored outlines in deleted node directories; Core treats directories containing only known safe generated files as derived residue, not authored nodes. It does not delete them or conceal unrelated resources/unsafe links; those still require investigation on rejection. Successful submission removes its completed draft; preserve blocked/dirty drafts for continuation and remove only your own clean disposable baselines after their result is retained.

Return actual file/relationship changes, commits, check output, derived version and any failures. Verify landed fields and references against the approved delta, including status side effects and affected consumers. A check pass, a commit, publication and independent use are separate claims. Human permission never proves that a write occurred.
