---
id: "d04227c1-0ccd-4d52-98dc-dba4d4c7f0e3"
name: "Choosing an External Model"
micro_summary: "Keeps model capability, cost and source-access decisions separate from Core's local knowledge operations."
status: "in_progress"
see_also: []
scope: "How to choose external reasoning support without assuming Core includes an assistant or subscription."
warnings: ""
---
Local Core does not select, launch or supply an AI model. You can use it manually, with a local model, or with a hosted model through a separately configured agent harness that can read files and execute commands.

Choose reasoning support for the task: bounded reversible work may be independently checked; ambiguous, security-sensitive or consequential decisions need appropriate judgment and scrutiny. Bigger models are not automatically better for every task. Compare actual useful results, errors, latency and cost rather than assuming a vendor or model size guarantees quality.

The harness determines access and provider costs. A hosted agent may transmit the files you give it to its provider; local Core does not prevent that. Obtain permission before sharing sensitive knowledge, keep edits in private worktrees and use the same native checks regardless of model. No particular subscription or hosted connector is required by Core itself.
