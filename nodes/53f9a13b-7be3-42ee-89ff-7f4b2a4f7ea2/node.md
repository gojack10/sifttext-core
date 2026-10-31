---
id: "53f9a13b-7be3-42ee-89ff-7f4b2a4f7ea2"
name: "Claude Web and Local Core"
micro_summary: "Routes Claude web to hosted MCP setup while distinguishing synced knowledge from local files."
status: "resolved"
see_also: []
scope: "Connect Claude web to synced knowledge or use permitted excerpts when a connector is unavailable."
warnings: "Do not upload sensitive local knowledge without permission."
---
Check the current Claude web environment's available integrations before assuming it can access local knowledge.

For access to synced knowledge, follow [[Connect Hosted MCP|c5dd1192-f75f-4369-bd9e-852f86b55121]]. The hosted workspace gives access to synced server files.

If no working connector is available, supply only excerpts you are permitted to share.

Review the resulting suggestions and save approved changes through the hosted workspace or a local file-capable agent using [[SiftText Commit|c72d552d-3499-445b-a8d5-05d0ff7824f2]].
