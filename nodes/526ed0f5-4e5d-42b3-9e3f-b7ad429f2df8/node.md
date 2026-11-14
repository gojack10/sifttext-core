---
id: "526ed0f5-4e5d-42b3-9e3f-b7ad429f2df8"
name: "Retrieve SiftText Knowledge"
micro_summary: "Finds relevant source nodes efficiently with direct file reads, rg, fd and bounded outlines."
status: "resolved"
see_also: []
scope: "Guide a fresh agent from an unfamiliar question to source-grounded understanding without unnecessary scanning or unbounded outline reads."
position: 1024
---
## Choose a retrieval path

Read a known file directly.

`sifttext search STORE "query"` ranks nodes by their words, for when you do not know where the answer lives. rg finds exact wording, names and IDs. fd finds files and links. The outline command shows tree structure. These are independent, so they can run in the same batch, alongside any other method that suits the task.

## Search by words

Search returns 20 ranked matches: the first 3 include their complete bodies; the other 17 are cards. Ancestor names and the first hit's children come in the same return.

It is SQLite FTS5 with BM25 ranking, so it scores words, not meaning. Every query word is optional: a node containing any one of them is a candidate, and each distinct query word it contains adds to its score. Rare words add the most, and a word in the node's name counts double. Word endings are folded, so "removed" also matches "remove" and "removal". Filler such as "the", "what" and "why" is ignored. `total_matches` reports how many nodes were candidates.

Build the query from the user's own words for the thing they are asking about. One call accepts several phrasings, `sifttext search STORE "first phrasing" "second phrasing"`, and merges their rankings, so a node that ranks well under more than one rises.

This example is synthetic. The user asks: "Why did we stop offering the annual plan, who made that call, and what bigger effort was it part of?"

**Bad example (A)**
> `sifttext search . "plan"` returns `total_matches:1900`. The annual-plan decision is not among the 20.
> `sifttext search . "Why did we stop offering the annual plan, who made that call, and what bigger effort was it part of?"` returns `total_matches:11000`. The decision is not among the 20.

**Better example (B)**
> `sifttext search . "stop offering annual plan" "annual plan discontinued"` returns `total_matches:310`. The decision node is first, with its body.

"Plan" alone is in most nodes. The whole question adds "made", "call", "bigger", "effort" and "part", which match thousands more. Both phrasings in B name the subject.

## If rg or fd is missing

The `sifttext instructions` receipt gives each tool's path, or `missing`.

Install a missing executable with the platform's package manager when that is permitted.

On macOS or Linux without a package manager, install official prebuilt binaries with curl and tar.

Add that directory to the current PATH, then run rg --version and fd --version to confirm.

Use an available equivalent when installation is blocked, and report the obstacle.

## Search and read

`rg -l` and fd print bare paths. `sifttext outline STORE -` accepts node IDs or paths from any producer and returns each once, in input order, without ranking or a cap, so one call both finds the nodes and names them. Rows carry ID, name, status and micro-summary; nonempty status reasons and warnings remain as optional fields.

```sh
rg -l -i "phrase|alternative wording" --glob node.md nodes | sifttext outline . -
rg -l -m1 '^name: "Project Rules"$' --glob node.md nodes | sifttext outline . -
fd -HI -t l "^FULL_NODE_UUID$" nodes --exclude artifacts | sifttext outline . -
```

Base conclusions on the node bodies, not cards alone. Search already supplies the first three bodies, so those do not need another read.

Each node's folder has `parent`, a link to its parent's folder, and it chains: `nodes/UUID/parent/node.md` is the parent's body and `nodes/UUID/parent/parent/node.md` is the grandparent's. A node and its ancestors' bodies can be read in one batch. A root has no `parent`.

Exclude attached datasets unless they are relevant.

Account for ignore rules and symlinks before treating an empty result as absent.

Run `sifttext outline STORE UUID` for a node and its direct children. Add `full` to read the whole subtree, or `asc` to read the selected node, its parent and every ancestor through the root in one call. The modes are alternatives; do not combine `full` and `asc`.

The command returns LEAN. With `asc`, `coverage:asc` identifies the complete upward chain; rows start at the selected node and `depth` counts parent hops from zero. Ancestor rows do not expand siblings or descendants, and child counts still describe the actual nodes. If a large return is suppressed, add `-y` to receive it in full rather than truncated.

Read a shallow outline first and expand only the branches the question needs.

State the displayed depth and any omitted pages.

## Stop and return

Batch independent reads when the interface supports it.

Return the answer with decisive node references and the remaining uncertainty.

Report a failed lookup honestly.

Stop when the evidence supports the answer.

Retrieval does not authorize knowledge changes or additional agents.
