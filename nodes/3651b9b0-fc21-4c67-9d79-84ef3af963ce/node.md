---
id: "3651b9b0-fc21-4c67-9d79-84ef3af963ce"
name: "Session Evidence"
micro_summary: "Associates session evidence during Commit and distinguishes links, retained copies and opt-in backup."
status: "resolved"
see_also: []
scope: "Document shipped opt-in session-retention behavior, verified configuration fields and commands, and the private-data and backup boundaries."
position: 8192
---
## Associate evidence
Use during Commit when a session materially supports the knowledge being recorded.

Record its actual source location and relevant message range when the harness exposes them.

Give each materially supported node a recognizable JSONL entrance under artifacts/. Name one evidence owner so related nodes link to that owner’s artifact path rather than retain duplicate copies.

Use only the source and sharing permissions actually supplied.

State consequential provenance gaps when session access is unavailable.

## Retain or back up
Obtain opt-in permission before retaining an independent transcript copy.

Obtain separate permission for remote backup.

Use the installed harness's documented capture and association interfaces.

Core's session commands are listed in its installed help.

Keep private transcripts and archive credentials outside knowledge Git.

Review tracked associations in the private knowledge draft and publish them through [[Worktree Publication|fb1e0c9f-8c04-4baf-91e6-4688856a14af]].

Verify the link, retained copy or remote backup separately according to what was requested.

When retention is enabled and authorized, retain one real copy under its evidence owner and link the other nodes to it. Preserve the file an active harness is writing.

When retention is disabled, use an association to the available durable source and link affected nodes through it. A symlink is not a retained copy or remote backup.

A narrower no-read or no-snapshot grant still controls. Continue ordinary Commit when retention is disabled.
