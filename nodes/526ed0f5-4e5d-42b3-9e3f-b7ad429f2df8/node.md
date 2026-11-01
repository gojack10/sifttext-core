---
id: "526ed0f5-4e5d-42b3-9e3f-b7ad429f2df8"
name: "Retrieve SiftText Knowledge"
micro_summary: "Finds relevant source nodes efficiently with files, rg, fd, bounded outlines and indexed queries."
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

Query the derived index for relationships and counts.

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

Prefer ordinary file tools on `nodes/<id>/outline.lean` for the node and direct children, or `nodes/<id>/outline.full.lean` for the whole subtree. If a generated file is absent or stale, `sifttext outline STORE UUID [full]` selects it and prepends its full absolute path as `outline-file`.

Read a shallow outline first and expand only the branches the question needs.

State the displayed depth and any omitted pages.

Use [[SQLite Queries|4cc0b396-4b55-415e-bb26-b51c97e6ca78]] for a direct-child listing when an outline returns too much.

## Stop and return

Batch independent reads when the interface supports it.

Return the answer with decisive node references and the remaining uncertainty.

Report a failed lookup honestly.

Stop when the evidence supports the answer.

Retrieval does not authorize knowledge changes or additional agents.
