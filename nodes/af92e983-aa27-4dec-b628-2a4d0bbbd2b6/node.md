---
id: "af92e983-aa27-4dec-b628-2a4d0bbbd2b6"
name: "Persistence"
micro_summary: "Applies the approved local delta in dependency order while preserving recoverable drafts and checked publication."
status: "in_progress"
see_also: [{"target":"fb1e0c9f-8c04-4baf-91e6-4688856a14af","reason":"Owns exact native worktree, normalization, check, submission and recovery commands."}]
scope: "Execute only approved local knowledge changes after Dialogue and Forcing Pass; return actual file and publication evidence."
warnings: "Create structure before dependent references/content; abort dependent edits after failure. Permission is not proof that files or accepted history changed."
---
## Input and role

Input is the approved Unix-style tree delta after the required pull and Forcing Pass. It identifies operations, exact content, effective statuses and the permission boundary. Use ordinary file tools in the authorized private knowledge worktree. Follow Worktree Publication for baseline allocation, checks, commit and serialized submission; do not edit accepted home or a software installation.

The human-facing author applies the approved delta. A bounded worker commits a checked private candidate and returns it to its assigning agent; it does not infer publication or push authority. The designated publisher separately carries an authorized candidate through checked submission.

## Local operation mapping

- `[+]`: use the native `create` helper for a node with a name, stable micro-summary and optional parent, then edit its native fields/body. Use the returned UUID, never an invented result.
- `[!]`: edit the appropriate JSON-valued metadata field or Markdown body in `nodes/UUID/node.md`. Append only incremental discoveries; rewrite only obsolete sections when warranted. Preserve meaningful history and unknown fields.
- `[→]`: change the exact old/new parent's child symlinks and complete `children.order` files; update root shortcuts for root transitions. Keep the node's UUID and directory. Inspect both parents and the node first; no dedicated move CLI is assumed.
- `[←]`: add or revise `see_also` entries as `{"target":"UUID","reason":"Nonblank relationship"}` after reading both endpoints. Narrative `[[Name|UUID]]` citations do not create graph edges.
- `[-]`: remove only approved authored node paths and structural entries, after dependent references have been dispositioned. Preserve the recoverable Git history; never delete another draft or unrelated generated files.

No general YAML, `parent`, `children`, duplicate `content` or `ruled_out` metadata is supported. Knowledge Layout owns the exact grammar. Terminal states need their native reason fields where required and evidence for the represented claim.

## Execution order

1. Create required structures and verify returned paths/UUIDs.
2. Apply moves and complete child ordering/root changes.
3. Add references once targets exist.
4. Edit approved body, scope, summary and warnings.
5. Apply justified status requests; normalize against the fixed baseline and review actual effects.
6. Delete last, then normalize and run the entire candidate check again.

If any operation fails, stop dependent operations, preserve the draft and report what actually landed. Do not invent replacement topology during persistence. Fix connected stale guidance within the approved scope through Reconciliation; ask for genuinely missing authority rather than silently expand it.

## Status and acceptance

Native normalization compares the candidate with the fixed baseline. An edit to an existing node can activate previously unexamined work and reopen affected resolved ancestors; a newly created leaf can remain unexamined. Explicit status requests must be passed consistently to normalization, check and submission. Inspect effects rather than resetting them to cosmetic pre-write markers. Resolve only when evidence proves the node's own claim and active direct children do not block it.

An ordinary Git commit does not enforce native graph/status/token gates. Use Worktree Publication's `check` and exact checked `submit` path. If submission is interrupted or its home/retrieval/cleanup remains unavailable, inspect the receipt and follow Worktree Publication's original-request recovery. Preserve the pending records and finish the already-accepted save without publishing it again.

## Return

Read affected files and native receipts, then report the actual changes, node count, private candidate SHA, checks and the completed home/accepted revision, or the exact unfinished recovery boundary. Distinguish draft, commit and checked publication. Reconciliation and Verify inspect the result next; completion requires their checks, not repeating the proposed delta as if it had executed. Remote pushing requires separately authorized repository access and privacy approval.
