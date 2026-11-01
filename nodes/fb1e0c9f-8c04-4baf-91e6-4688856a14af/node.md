---
id: "fb1e0c9f-8c04-4baf-91e6-4688856a14af"
name: "Worktree Publication"
micro_summary: "Explains how isolated knowledge edits reach accepted main through validation and reconciliation, with recoverable history and explicit stop paths."
status: "resolved"
see_also: [{"target":"dcdd97b3-9557-41e3-b7cb-1f6f9744267d","reason":"Required repo commit conventions before staging and committing local knowledge."},{"target":"c72d552d-3499-445b-a8d5-05d0ff7824f2","reason":"Local proposal, approval and verification discipline before authored changes."}]
scope: "Carry an authorized local edit through proposal, private Git work, validation, review, publication and recovery without overwriting another owner."
warnings: "Cooperative isolation, not an OS sandbox. One publisher owns accepted knowledge at a time; no force, reset, automatic push, or overwriting another worktree."
---
## Supported workflow

Read accepted main; allocate a private worktree from accepted main; edit and commit only in that worktree; submit the exact candidate through Core.

Do not edit the normal home directly or hide edits with Git index flags. Preserving such out-of-workflow edits is not part of the supported publication contract. Preservation instructions below protect active private work and submitted-request recovery; they do not require indefinite retention of disposable metadata or expired abandoned drafts. This does not authorize deleting accepted history, bypassing a current Core refusal, or overwriting another active workspace.

Submission owns its required reader completion. Do not add a ritual `advance`, index refresh or duplicate preflight before ordinary worktree allocation or submission. Use explicit reader recovery only when an actual receipt or reading need calls for it.

## Temporary drafts and expiry

Core allocates new private drafts and internal publication views beneath the platform's temporary directory.

Before ordinary knowledge and sync commands, Core attempts cleanup at most once per hour. Temporary workspaces become eligible after 48 hours without file or directory modifications. Reads alone do not renew that period.

Expiry can delete uncommitted files in an inactive, unsubmitted draft. Keep task artifacts in the durable `artifact_home`, not solely in the draft.

Keep worktrees short-lived. Read SiftText, do the work, save through SiftText Commit, and start the next chat from SiftText.

Active operations and drafts with submitted-save journals are retained. Expiry does not delete accepted Git history or draft branches.

Earlier workspaces outside this temporary namespace are not swept by age. Cleanup failures retain the affected workspace and do not prevent the requested command.

## Authority and private draft

Follow SiftText Commit and its required reads before knowledge edits.

Only explicit preapproval or an autonomous setting with no user interaction skips the normal approval gate.

An ordinary editing request does not.

A bounded worker returns its assigned candidate to its assigning agent.

it does not infer publication or push permission.

The human may stop or reshape the undertaking without waiving unrelated permissions or preservation checks.

Inspect status, worktrees and ownership.

preserve active private drafts and in-flight submitted work.

Reuse an owned draft for the same undertaking.

```sh
git -C "$home" status --short

git -C "$home" worktree list

sifttext worktree "$home"

```

Use the returned `worktree` as `wt` and immutable `base` SHA as `base_sha`.

Allocation starts from accepted main even when the normal home is dirty or behind.

it does not change that home.

After uncertain allocation, inspect registered worktrees before retrying.

## Edit, inspect and commit

Edit ordinary native files only in `wt`.

Native files with valid UUIDs and layout do not require that helper.

Drafts may be incomplete while editing.

Never overwrite another contributor's paths or replace a full child list with a partial one.

Before staging/committing, read [[Git Commit Skill|dcdd97b3-9557-41e3-b7cb-1f6f9744267d]].

```sh
git -C "$wt" status --short

git -C "$wt" diff

# git -C "$wt" add -- <the inspected authored paths>
git -C "$wt" diff --cached --check

# git -C "$wt" commit -m 'knowledge: describe the approved change'
candidate=$(git -C "$wt" rev-parse HEAD)

```

A private commit is not accepted knowledge.

## Submit the exact combined candidate

```sh
sifttext submit "$home" "$wt" "$candidate" "$base_sha"

```

Append only full UUIDs for explicitly authorized status requests.

reopening an existing resolved leaf requires its explicit UUID.

Resolved parents reopen automatically when an active direct child blocks them.

Nodes absent from the base are creations.

a new leaf may remain `not_examined`.

Resolution with active children must fail.

It then conditionally advances accepted main.

Normalization may create an attributed commit.

integration retains committed history and its attribution.

Do not bypass the gate or change frozen limits.

Use the optional hosted `epistemic-check` and `slop-check` commands when a separate review is needed.

Inspect actual writes and any partial failure before retrying.

Before submitting knowledge, read [[Epistemic Check|1b668703-b591-4790-ae6b-5c4202413de5]] to handle Core's epistemic review.

Read the actual receipt to establish whether checking completed. Agents remain responsible for interpreting the evidence within the user's authority.

Raw Git can bypass this cooperative workflow.

it is not OS enforcement.

Verify accepted identities and affected source from `home`.

no ritual `advance` or `index` is needed.

Allocate a new draft only for the next authorized edit.

## Refusal, repair and recovery

Preserve the draft, diagnostics, original submit arguments and Core's pending journals.

A failure may mean `not_accepted`, `accepted_incomplete` or `unknown`.

an exit failure does not prove nothing was saved.

Never reset accepted history or manually delete journals/caches to clear a refusal.

For interruption or recorded preparation, retry the original arguments so Core can resume preparation or finish already-accepted work without republishing it.

Follow pending-request diagnostics if that request cannot yet proceed.

do not invent acceptance or erase custody records.

A repair must preserve prior prepared work, not reset back over it.

Revisit meanings or ask the owner only when the repair exceeds existing authority.

Inspect an uncertain partial operation before retrying so successful work is not duplicated.

Missing authority, unsafe paths or unavailable external prerequisites stop dependent work.

continue independent permitted work.

## Accepted reading and completion

`accepted HOME` returns a separate checked detached view. Its LEAN receipt includes the accepted identity and version with the reading path in `view` (`status:ready`, `home_advanced:F`).

Read that view when the normal home cannot move.

A submit receipt with `status:accepted_incomplete` and `check:accepted_finish_incomplete` confirms that acceptance succeeded but finishing failed.

Inspect the reader and draft to establish which finishing work remains.

### Read while finishing is blocked

Run `sifttext accepted HOME` and check its accepted identity and version before reading the returned `view`.

The normal home can remain behind while that view is read. Draft cleanup still needs the original submit request.

### Preserve a dirty reader

Inspect `git -C HOME status --short` without changing files.

Core generates its retrieval database at `.sifttext/index.sqlite`. An untracked root `sifttext.db` is a different file with unknown ownership. Do not delete, relocate or ignore it merely to advance the reader.

Leave the blocking file and normal home unchanged while ownership or preservation authority is unknown. Hand reader recovery to the owner and use the accepted view for reading in the meantime.

Keep the draft and pending journals with the exact original submit arguments. After an authorized recovery resolves the blocker, retry those arguments to finish the pending save. `advance HOME` alone does not finish pending draft cleanup.

`advance HOME` directly advances an eligible clean detached reader to already-accepted main and refreshes retrieval.

it does not resubmit knowledge.

Inspect ownership first.

Retire completed invocation-owned disposable views through their normal cleanup path. Active or unresolved submitted work is not disposable scratch. Apply Temporary drafts and expiry only within its documented scope. Outside that policy, unknown or edited views remain.

For stale retrieval use `index STORE`, which refreshes the current parseable draft without certifying publication.

Retire generated residue only with verified cache ownership.

Never delete arbitrary leftovers to obtain a clean result.

Never reset main.

Rehome durable artifacts deliberately before removing their owner.

empty artifact containers are still protected.

Verify landed fields and affected continuation against the authorized delta.

Behavioral improvement requires separate observation.
