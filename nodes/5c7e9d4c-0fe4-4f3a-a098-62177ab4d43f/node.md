---
id: "5c7e9d4c-0fe4-4f3a-a098-62177ab4d43f"
name: "Render a Tree Delta"
micro_summary: "Renders private draft changes or selected static trees for review."
status: "resolved"
see_also: []
scope: "Choose a rendering, declare artifact links, and present the resulting review file without publishing it."
position: 10240
---
## Choose the view

During [SiftText Commit](../c72d552d-3499-445b-a8d5-05d0ff7824f2/node.md), epistemically check non-exempt proposed nodes before presentation. Official bundled documentation is exempt, not passed. Rendering makes no checker calls and does not approve or publish knowledge.

For a native private draft, derive the selection and allocation base:

```sh
sifttext tree-render WORKTREE --auto
sifttext tree-render WORKTREE --auto PROPOSAL.json
```

Automatic mode selects the changed nodes and their tree context. It reads the draft's stored allocation base, or the original branch-creation reflog for an older draft. If neither is available, use the original worktree receipt with manual mode. Never substitute today's main.

Artifact link changes are never derived. The agent must declare each intended addition or removal, even in automatic mode. With no declarations, the render says none were declared; it cannot detect an omitted intention.

For custom selection or a static view:

```sh
sifttext tree-render STORE SELECTION.json
```

Each successful command returns `html`, `data` and `output` paths in a fresh `/tmp` directory, plus the Markdown's `o200k_base` token count. The self-contained HTML is the review surface; `data` contains the exact snapshot and `output` retains the text representation. Keep the exact candidate available.

When signed in and online, the command also saves the snapshot privately and returns `render_id` and `review_url`. Browser access requires the owner's login. In a chat with SiftText MCP, call `show_tree_delta` with that ID to display the same review inline using the existing authenticated connection. Always attach the actual URL:

Review the saved change here: [url]

During a proposal-only assignment, write `Review the proposal here: [url]` instead. Use the returned `review_url` in place of `[url]`. When no inline card appears, the link is the review surface. Do not print the tree or proposed bodies in a code block unless the user explicitly requests that representation. `show_tree_text` supplies the saved text tree for that request. Offline or when hosted saving is unavailable, open or attach the local HTML using the environment's supported file route. Never invent a hosted URL.

Rendering does not publish knowledge, approve a proposal or upload attachment bytes. Node references within the snapshot navigate its displayed nodes; external web links retain their destinations. A local filesystem path is a source description, not a remotely accessible attachment. A changed candidate requires a new render. If a saved review returns `This review has expired.`, regenerate it from the current candidate.

### Review surface example

This example is synthetic. The user asked to save, the save succeeded, and the rendering returned a private browser URL, but the chat cannot show inline cards.

**Bad example (A)**
> Here is the tree instead:
> ```text
> [!] Project
> ```

**Better example (B)**
> Saved to SiftText. The check passed.
>
> Review the saved change here: https://app.sifttext.com/renders/EXAMPLE_RENDER_ID

Use the actual returned URL and check result. An offline agent can offer the generated local HTML instead; a user asking for a text tree receives that representation. The review surface does not change who may approve or save knowledge.

## Selection JSON

Use JSON, not JSONL. A static selection needs no proposal:

```json
{"trees":[{"start":"NODE_UUID","depth":1}]}
```

Each item in `trees` accepts:

| Field | Meaning |
| --- | --- |
| `start` | Required UUID of the node to start from. `root` is a deprecated alias for this release only. |
| `depth` | Nonnegative levels below start: 0 selects start, 1 includes direct children. Default 1. |
| `ascend` | `compact` (default) or `full`. Compact names the true root and immediate parents, compressing other ancestry with an ellipsis. Full names the whole ancestor route. |
| `centres` | Extra UUIDs within the selected subtree. Each receives a one-child preview unless an explicit branch depth overrides it. No automatic sibling expansion. Default `[]`. |
| `branches` | `[{"node":"NODE_UUID","depth":0}]`. A branch's own depth replaces its inherited depth, expanding or capping descent. Default `[]`. |
| `default` | `include` (default) or `exclude` for nodes without an applicable rule. |
| `rules` | `[{"node":"NODE_UUID","action":"include","recurse":true}]`. Action is `include` or `exclude`; `recurse` is required and is `true` or `false`. Default `[]`. |

A node's direct rule wins, then its nearest recursive ancestor's rule, then the default. Conflicting rules or depths for one node and unknown fields are rejected. Explicit include rules also select centres. Ancestor context does not seed further expansion. The true tree root remains visible.

## Proposal JSON

Manual mode adds this top-level `proposal` beside `trees`:

```json
{
  "base":"FULL_ALLOCATION_COMMIT",
  "artifacts":[],
  "deletions":[{"node":"DELETED_SUBTREE_UUID","reason":"Why it is removed"}]
}
```

`base` is the full immutable commit printed by `sifttext worktree`, not the current main tip. Git compares the draft with that base. `artifacts` is required even when empty. `deletions` is optional; declare the reason for each deleted subtree root. `work_record` also exists but is outside this guide's scope.

For automatic mode, PROPOSAL.json has this same shape with `base` omitted. The command supplies the allocation base; if a base is supplied, it must match. No JSON is needed when there are no artifact declarations or deletion reasons to supply.

An artifact declaration is:

```json
{
  "owner":"OWNER_UUID",
  "operation":"add",
  "destination":"artifacts/evidence",
  "target":"/absolute/path/to/evidence",
  "reason":"What this source contributes"
}
```

`operation` is `add` or `remove`. The destination is relative to its owner and starts with `artifacts/`; the target is absolute. A replacement declares removal of the old target and addition of the new one. The renderer does not create, follow or delete these links. Removing a link does not delete its target. Complete approved associations through Commit's artifact procedure.

## Read the render

Every delta begins with:

```text
[+] added, [!] revised, [-] removed, [→][!] moved here, [←] moved away
[ ] not examined, [~] in progress, [✓] resolved, [/] blocked, [=] deferred, [x] discarded
… omitted context
```

When artifacts appear, the header also explains:

```text
artifacts/ = file links owned by the topic above it: + new link, - removed link, @1 = shorthand for a full path already shown once in this render
```

Internal artifact targets use a labelled owner in the forest; external targets retain their absolute paths. Static text renders show only the status notation, names and hierarchy. Their HTML also lets the user inspect the selected nodes, without proposal change marks.

Proposal mode includes changed nodes within the selected trees even when selection rules exclude them. Moves show both locations. Deleted subtrees are summarized with omitted-descendant counts. Overlapping selections form one forest. Changed fields and proposed bodies appear beneath it; unchanged ancestors are context, not edits.

Return the render to SiftText Commit, which shows it with the check result after the save succeeds.
