---
id: "b6107909-a33b-4c63-ab0d-351ba1a1b672"
name: "Knowledge Layout"
micro_summary: "Explains how repository locations, UUID folders, authored fields and ordered child relationships fit together for local knowledge navigation."
status: "in_progress"
see_also: []
scope: "Locate the local knowledge home and understand its native identity, content and hierarchy before using retrieval or editing tools."
warnings: ""
---
## Locate the source

Input: the assigned knowledge home, named explicitly on every command; the delivered command has no default `$HOME/sifttext` home. Do not confuse it with the Core source checkout or the bundle it was copied from. Inspect `git -C "$home" rev-parse --show-toplevel` and `git -C "$home" status --short`. If missing, follow CLI Operations for authorized setup. If unfamiliar or dirty, establish ownership before writes. Flagless `init` copies the first-party KB compiled from Core's readable `nodes/` and `trees/`; `init --bundle` chooses another one-time copy source. Neither is live cloud sharing. Normal knowledge work belongs in a private worktree of the separate user-owned knowledge repository, usually `~/sifttext`, not inside the software installation or contributor checkout. All nodes and tree shortcuts are Git-tracked directories/files in that one knowledge repository, not individual Git repositories. A node-owned project code repository remains separate.

```text
HOME/
  nodes/UUID/node.md           authored metadata and Markdown body
  nodes/UUID/children.order    full child UUIDs, one per line
  nodes/UUID/children/UUID     symlink -> ../../UUID
  nodes/UUID/artifacts/NAME    local task files/projects; ignored, not knowledge Git
  trees/ROOT_UUID              symlink -> ../nodes/ROOT_UUID
  .sifttext/index.sqlite       derived, ignored, per working copy
```

Empty directories use empty `.keep` files. UUIDs, not names, are identity. Local tree roots are node UUIDs, not hosted tree-container IDs. Parentage comes from child symlinks; order comes from `children.order`. Every node belongs to one rooted tree. Read `nodes/UUID/node.md` directly or follow the tree shortcut. Do not physically nest node directories to express depth.

## Node artifacts

Task files such as spreadsheets, PDFs, reports and code projects belong under `nodes/UUID/artifacts/` of the deepest node owning that work, in the durable knowledge home, not an expendable client workspace or editing worktree. A user-selected external location remains valid; retain its relationship in the owning node rather than silently moving it.

Run `sifttext worktree HOME` before placing files in an older or cloned home. Its `artifact_home` is the canonical durable home; `worktree` is the disposable knowledge draft. Initialization installs the artifact exclusion, and worktree allocation, save and reader advancement reapply it locally without changing the home's recognized `.gitignore`. For a new owner, publish the knowledge node first, then place its files in that accepted home's node directory. Verify the owning node and real `artifacts/` directory before writing; create the directory if absent, never replace an existing path or make the container itself a symlink. Use recognizable file names. A nested project can keep its own independent Git repository.

In the private knowledge draft, record a meaningful home-relative artifact path in the owner's body, and the external destination relationship for any aliases. Knowledge validation does not open, recursively scan, hash, tokenize or copy artifact contents. Knowledge Git must not track them; do not force-add them, use Git LFS, or treat dependencies/build caches as knowledge. `artifacts/chats/` remains reserved for the separate optional session-evidence workflow; its archive and tracked association metadata retain that workflow's rules.

A symlink inside `artifacts/` is an alias, not a snapshot or backup. Inspect and obtain appropriate scope before modifying or traversing its external target. Editing through the link edits that target. A missing target stays unresolved: report it and reconnect deliberately, never guess from a similar filename. No local artifact files, directories or aliases are transported by knowledge Git, clones, release updates or session restore; keep a separately authorized backup if needed.

After a save, verify the actual task file at its durable path, not only the knowledge receipt. Check that `git -C HOME check-ignore -- nodes/UUID/artifacts/NAME` names the file and `git -C HOME ls-files -- nodes/UUID/artifacts` is empty. The normal home's artifacts survive knowledge save, reader advance, draft cleanup and exact-request recovery because those operations do not relocate or delete them. Before removing or reverting away their owning node, deliberately rehome the files and update their references, then remove the old container, or retain the owner. Checked publication and reader advancement refuse owner removal while its durable artifact container remains, even if empty; they never inspect its contents to guess whether it is disposable. A source-less artifact directory is not valid knowledge and is not disposable residue.

Submission refuses non-chat artifacts in a private draft before publication or cleanup and preserves them. On that refusal, retain the files, verify the intended durable destination, and move them deliberately within the user's permission; never delete them to obtain a clean check. If their owner is new, preserve the files outside the disposable draft until the knowledge-only owner is accepted, then place them under that owner's home `artifacts/` and verify the bytes. Retry the normal knowledge publication procedure. This is local custody, not automatic artifact migration, backup, synchronization or cross-machine recovery.

## Authored grammar

`node.md` starts with `---` and newline, then one `key: JSON-value` per line, then `---` and newline, then the Markdown body. This is restricted metadata, not general YAML. Required: `id`, nonblank `name`, nonblank `micro_summary`, `status`, `see_also`. The `id` must equal its canonical lowercase UUID directory. Optional `scope` and `warnings` are JSON strings. The body holds content; no duplicate `content` field.

`see_also` is an array of `{"target":"UUID","reason":"Nonblank explanation"}`. Targets must exist; no self or duplicate references. Prose links do not author graph edges. Do not author `parent`, `parents`, `children` or `ruled_out` metadata. Preserve unknown JSON fields rather than inventing their meaning.

A micro-summary describes stable responsibility and helps readers choose a node; it is not a progress report. Keep independently meaningful questions visible as nodes. Keep mechanical steps for one task together. Do not duplicate an answer in ancestors or add redundant parent/child references. Explain meaningful rejected attempts rather than erase their evidence.

States: `not_examined`, `in_progress`, `resolved`, `stuck`, `deferred`, `discarded`. Stuck needs `blocker`; deferred needs `deferred_reason` with a resume condition; discarded needs `discard_reason`. Resolved needs a nonblank body and no active direct child. Evidence must prove the node's own claim. Worktree Publication owns activation/reopening and explicit status requests.

## Derived state and limits

`.sifttext/`, `node.meta.json`, `outline.lean`, `outline.full.lean` and their metadata sidecars are disposable. Never commit them or edit them to repair authored errors. The delivered command imposes no fixed node-count ceiling. It enforces checked resource boundaries instead: each authored file must stay within 1 MiB and is otherwise refused as `unsafe or oversized authored file`, symlinked or hard-linked authored files are refused the same way, and each content body must stay within 5000 `o200k_base` tokens, which `check` rejects per node as `content_over_limit` with measured `tokens` and `limit:5000`. Native inventory and process-resource guards still bound a store, so these are error boundaries rather than a capacity promise: no node count, store size or latency is claimed to be affordable. A passing check reports only `nodes`, `status` and the knowledge `version`; a refusal reports actual problems rather than successful per-node token totals. This is not full-store import or resource qualification. Stop on a limit rather than truncate content or change caps.

Return the confirmed home/worktree, source path and relevant UUID to the assigned task. Continue with retrieval for a read task, or Worktree Publication before changing knowledge.
