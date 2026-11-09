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

Search node names and body phrases with rg.

Find files and directories with fd.

Prefer rg and fd when available; ordinary file tools or a better task-specific method are also valid. Check availability rather than assuming another installation has these tools.

Read a shallow outline to choose a relevant branch.

Use the outline command for tree structure.

## Install the tools

Check whether rg and fd are installed before using them.

Install a missing executable with the platform's package manager when that is permitted.

On macOS or Linux without a package manager, install official prebuilt binaries with curl and tar.

Add that directory to the current PATH, then run rg --version and fd --version to confirm.

Use an available equivalent when installation is blocked, and report the obstacle.

## Search and read

```sh
rg -l -m1 '^name: "Project Rules"$' --glob node.md nodes
rg -l -i 'phrase|alternative wording' --glob node.md nodes
rg -l -F 'FULL_NODE_UUID' --glob node.md nodes
ls -d nodes/*/children/FULL_NODE_UUID
fd -HI -t l '^FULL_NODE_UUID$' nodes --exclude artifacts
rg '^(name|micro_summary|status):' nodes/FULL_NODE_UUID/node.md
```

Open the matched node body before drawing a conclusion, because a match locates evidence rather than proving the answer.

Exclude attached datasets unless they are relevant.

Account for ignore rules and symlinks before treating an empty result as absent.

Run `sifttext outline STORE UUID` for a node and its direct children. Add `full` to read the whole subtree. The command returns LEAN.

Read a shallow outline first and expand only the branches the question needs.

State the displayed depth and any omitted pages.

## Stop and return

Batch independent reads when the interface supports it.

Return the answer with decisive node references and the remaining uncertainty.

Report a failed lookup honestly.

Stop when the evidence supports the answer.

Retrieval does not authorize knowledge changes or additional agents.
