---
id: "a8be9278-ca1b-4bf4-9fa7-027105bb6c36"
name: "Onboarding Output and Help"
micro_summary: "Reads the instructions receipt and follows its command-guide index."
status: "resolved"
see_also: []
scope: "Interpret retained onboarding state without confusing review findings, transport and update permission."
position: 10240
---
Run `sifttext instructions [HOME]` before knowledge work. Read `onboarding-entry` and follow any `agent-instructions`. The appended grouped help index routes to guide nodes; `sifttext help` lists command forms from the same registry.

`hosted-sync` reports retained transport state, not a fresh server inquiry. `last-sync-complete` describes the last recorded exchange; it does not establish current remote freshness. `not-configured`, disabled consent, pending delivery, local changes, stopped transport and `needs-repair` are different states. Use `sifttext sync status HOME` and its actual repair guide when directed.

`check-findings` and `flagged-nodes` identify affected nodes, saved request IDs and checked-text hashes when available. Retrieve each result with `sifttext check-result ID`, compare it with the node's current text, and repair supported findings through [[SiftText Commit|c72d552d-3499-445b-a8d5-05d0ff7824f2]]. Older receipts may lack a text hash and remain historical until compared. Do not infer that a later unrelated passing save cleared an earlier node's finding.

Own-device sync retains findings without refusing transport on that ground. Integrity, topology, account binding and consent checks still apply. Do not share a flagged node with another person until its findings are resolved. A checker result is not human approval.

Resolve assigned repairs within the user's grant while keeping independent work moving. Ask the user only when a genuinely significant choice remains, such as changing supported meaning or sharing permission.

For `update-available`, follow `update-guide`. Under ask permission: “Read update-guide. When the user is next present, ask for permission to update SiftText.” Under standing permission, automatic updates are enabled. Handle the update at a convenient pause and keep the user informed. An update notice is not a compatibility guarantee for a converted home.

`harness-instructions-refresh: required` means the current startup block has not been acknowledged for this installation. Read `harness-instructions-guide` and ensure that block is first in the harness's user-controlled persistent instructions, using the same procedure for setup and refresh. The guide explains how to record completion with `sifttext update --harness-instructions-applied` after applying the block or receiving the user's confirmation. The receipt reads retained update state, it does not scan harness files.

Follow [[Keeping the Human in the Loop|6e9f23b3-3fbe-4dbd-9b58-6842438d2d01]] during extended work and routine maintenance. Its synthetic examples illustrate how to stay informative without making every receipt a user interruption.

Knowledge lives at `nodes/<id>/node.md`. Generated `nodes/<id>/outline.lean` shows the node and its direct children; `nodes/<id>/outline.full.lean` shows the whole subtree. Prefer ordinary file tools on these views when present. The outline command generates or selects its view and prepends `outline-file` with the full absolute path.

Prefer rg and fd for search when available, without restricting a better task-specific method. Check tool availability in an unfamiliar installation.
