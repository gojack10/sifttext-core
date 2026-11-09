---
id: "3651b9b0-fc21-4c67-9d79-84ef3af963ce"
name: "Session Evidence"
micro_summary: "Guides session discovery, retained transcript copies, shared node links and local querying."
status: "resolved"
see_also: []
scope: "Locate, retain and query conversation evidence for materially supported knowledge while preserving original harness files."
position: 8192
---
Keep the conversations behind maintained knowledge recoverable, including details that may become relevant later.

## Find the source

Start with existing artifact links and recorded source locations. Prefer the exact session path or identity supplied by the harness over guessing from filenames or modification times.

Typical locations:

| Harness | Source |
| --- | --- |
| **Claude Code** | `${CLAUDE_CONFIG_DIR:-$HOME/.claude}/projects/<project>/<session-id>.jsonl`. Prefer the harness-provided `transcript_path`; project-directory names can be customized. |
| **Codex** | `${CODEX_HOME:-$HOME/.codex}/sessions/YYYY/MM/DD/rollout-*.jsonl` or `.jsonl.zst`; also check `archived_sessions/`. Verify the intended thread against the file's session metadata, especially for subagents. |
| **Pi** | Use the active session path supplied by Pi. Default storage is under `~/.pi/agent/sessions/`; custom session locations may differ. |

Claude Code and Codex `history.jsonl` files contain prompt history rather than complete conversation transcripts.

If the source cannot be recovered, record that limitation and continue the knowledge work.

## Retain once, link thereafter

When SiftText Commit is invoked, associate materially supported nodes with the relevant source sessions.

For a new node, cite the existing session source during review. Follow [Associate External Artifact Directories](../de7cea55-c93d-48db-a4f6-58368bf6c36f/node.md) before creating its durable copy or links.

For **Codex, Claude Code and Pi**:

- **First copy:** use `rsync` to copy the native transcript into the evidence owner's durable `artifacts/chats/` directory.
- **Already retained:** reuse that copy. Create recognizable symlinks under other relevant nodes' `artifacts/chats/`, pointing to the retained copy.
- **Preserve the original:** our retention operation must never remove, move or replace the harness's original chat file. Never use `rsync --remove-source-files` for session retention.

The user or harness may independently clean up the original later. That does not remove the retained artifact copy.

When later conversation records support another Commit, refresh the same retained destination rather than creating duplicate independent copies. Preserve the earlier capture if refreshing fails.

Use readable names such as `<harness>-<topic>.jsonl`, preserving the actual source format and compression. Record the native session identity and relevant message range or cutoff when available.

A transcript may reference separate attachments, tool outputs or subagent files. Preserve accompanying material needed by the recorded evidence and identify consequential gaps.

Honor explicit user restrictions on copying, reading or sharing. Keep private transcript contents outside knowledge Git. Verify retained files and links before reporting completion. Remote backup and sharing require separate authorization.

## Query the actual format

Use suitable tools already available, such as Python, jq or DuckDB. Discover the file's keys and value types before writing a query; harness formats can change.

For example, when jq is available, this inventories nested field paths and observed value types in readable JSON or JSONL:

```sh
jq -r '
  paths as $p |
  [
    ($p | map(if type == "number" then "[]" else . end) | tojson),
    (getpath($p) | type)
  ] | @tsv
' /path/to/chat.jsonl | sort -u
```

Array positions are grouped as `[]`. Use the discovered structure to select the messages, tool activity or metadata needed for the question. Compressed sources require a compatible reader or decompression stream.

Preserve message ordering, branch relationships and tool-call identifiers when they matter. Keep filename and record or line locators for quoted evidence. Report malformed or incomplete records rather than silently dropping them.

Locating or querying a chat does not itself trigger retention or SiftText Commit.
