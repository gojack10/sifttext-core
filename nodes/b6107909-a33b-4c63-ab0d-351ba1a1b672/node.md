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
  nodes/UUID/artifacts/NAME    files the node owns; kept outside Git until this build accepts them
  trees/ROOT_UUID              symlink -> ../nodes/ROOT_UUID
  .sifttext/index.sqlite       derived, ignored, per working copy
```

Empty directories use empty `.keep` files. UUIDs, not names, are identity. Local tree roots are node UUIDs, not hosted tree-container IDs. Parentage comes from child symlinks; order comes from `children.order`. Every node belongs to one rooted tree. Read `nodes/UUID/node.md` directly or follow the tree shortcut. Do not physically nest node directories to express depth.

## Authored grammar

`node.md` starts with `---` and newline, then one `key: JSON-value` per line, then `---` and newline, then the Markdown body. This is restricted metadata, not general YAML. Required: `id`, nonblank `name`, nonblank `micro_summary`, `status`, `see_also`. The `id` must equal its canonical lowercase UUID directory. Optional `scope` and `warnings` are JSON strings. The body holds crystallization; no duplicate `content` field.

`see_also` is an array of `{"target":"UUID","reason":"Nonblank explanation"}`. Targets must exist; no self or duplicate references. Prose links do not author graph edges. Do not author `parent`, `parents`, `children` or `ruled_out` metadata. Preserve unknown JSON fields rather than inventing their meaning.

A micro-summary describes stable responsibility and helps readers choose a node; it is not a progress report. Keep independently meaningful questions visible as nodes. Keep mechanical steps for one task together. Do not duplicate an answer in ancestors or add redundant parent/child references. Explain meaningful rejected attempts rather than erase their evidence.

States: `not_examined`, `in_progress`, `resolved`, `stuck`, `deferred`, `discarded`. Stuck needs `blocker`; deferred needs `deferred_reason` with a resume condition; discarded needs `discard_reason`. Resolved needs a nonblank body and no active direct child. Evidence must prove the node's own claim. Worktree Publication owns activation/reopening and explicit status requests.

## Node artifacts

A file made for a task, such as a spreadsheet, report, draft or export, belongs to the deepest node whose question it answers. The current build's `submit` refuses any commit that tracks a file under `nodes/UUID/artifacts/` (`authored_tree` allows only the authored node files and relationship links), while `check` accepts the same tree, so only submission catches it. Until Core accepts tracked artifacts, keep the file outside the home, in the client's workspace or a shared drive, and record its location in the owner's body. Do not list it as an `Add artifact` operation. `artifacts/chats/` is reserved for session links and is not tracked.

## Derived state and limits

`.sifttext/`, `node.meta.json`, `outline.lean`, `outline.full.lean` and their metadata sidecars are disposable. Never commit them or edit them to repair authored errors. The delivered command imposes no fixed node-count ceiling. It enforces checked resource boundaries instead: each authored file must stay within 1 MiB and is otherwise refused as `unsafe or oversized authored file`, symlinked or hard-linked authored files are refused the same way, and each crystallization body must stay within 5000 `o200k_base` tokens, which `check` rejects per node as `crystallization_over_limit` with measured `tokens` and `limit:5000`. Native inventory and process-resource guards still bound a store, so these are error boundaries rather than a capacity promise: no node count, store size or latency is claimed to be affordable. A passing check reports only `nodes`, `status` and the knowledge `version`; a refusal reports actual problems rather than successful per-node token totals. This is not full-store import or resource qualification. Stop on a limit rather than truncate content or change caps.

Return the confirmed home/worktree, source path and relevant UUID to the assigned task. Continue with retrieval for a read task, or Worktree Publication before changing knowledge.
