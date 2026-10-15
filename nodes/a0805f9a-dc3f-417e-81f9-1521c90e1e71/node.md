---
id: "a0805f9a-dc3f-417e-81f9-1521c90e1e71"
name: "Common Questions"
micro_summary: "FAQ catch-all for confusions that don't map to a single concept node: deleting vs discarding, privacy, export, plus how SiftText differs from auto-memory and from Obsidian. Open when a question doesn't fit a concept node cleanly."
status: "in_progress"
see_also: [{"target":"fbaa3012-2235-49cb-88c0-67fdbc366c36","reason":"Explains the human's control over reading and saving memory."},{"target":"5f146883-03c8-416c-be6b-c52cbcb055f2","reason":"Offers the binder metaphor for explicit, inspectable knowledge."}]
scope: "FAQ-shaped content for questions that do not map to a single concept node elsewhere. Receives links from other nodes. Catch-all for edge cases and common confusions."
warnings: ""
---
Common questions about local SiftText Core. For a question not answered here, read the relevant local nodes or ask the agent working with you; no built-in hosted assistant is required.

**Can I delete a node?**
Yes, in an authorized private draft, but consider discarding it instead. Discarding keeps the reasoning visible in the current tree. Deletion removes it from the current tree; committed history remains recoverable through Git. Preserve relationships, check the complete candidate and publish through Worktree Publication. Do not delete accepted files directly or reset history to hide an attempt.

**Can other people see my trees?**
Knowledge is stored as ordinary files in your independent Git repository, not in a hosted account. Core does not upload it automatically. Anyone with access to those files, backups, a shared remote or an agent's permitted tools may read it. Filesystem and repository permissions, and what you send to a model provider, determine privacy; Core's cooperative worktrees are not an access-control sandbox. A reference grants no access to another repository.

**Can I export my trees?**
The authored knowledge already consists of Git-tracked native files: `nodes/`, `trees/` and their relationships. Use ordinary Git cloning or a file copy that preserves symlinks to retain it. Generated SQLite and outlines are rebuildable views, not the authoritative export. The delivered local CLI does not supply a hosted export or automatic cloud-sync service. Sharing or pushing a knowledge repository requires the owner's permission and a content/privacy review.

**What happens if I disconnect a model?**
Local Core requires no model connection. Your knowledge files and Git history remain available to ordinary file tools and the local command. Ending an agent session does not remove them. Model-provider credentials, sessions and permissions belong to that agent's harness, not Core; stopping Core does not revoke access another program already has to your files.

**Can an agent make mistakes?**
Yes. An agent can misunderstand evidence or propose poor wording and structure. Read its proposed changes and their rationale before approving them. Native checks validate structure, fields, statuses, token limits and source consistency; they do not establish truth or the usefulness of an answer. Keep incorrect drafts inspectable and repair or undo them through the checked private-worktree/publication procedure.

**How many trees or nodes can I have?**
There is no fixed node-count ceiling, but that is not an unlimited-capacity promise. Each authored file is bounded to 1 MiB and each crystallization body to 5000 `o200k_base` tokens; complete operations also have resource guards. See Knowledge Layout for the delivered limits. A refusal requires diagnosis, not truncation or weakened checks. Outlines and scoped lexical search help navigation without proving that every store size is affordable.

**How is this different from automatic model memory?**
Core keeps explicit, inspectable knowledge files and Git history. You or an authorized agent choose what to read, propose changes and save them through the agreed gate. It does not silently select and inject memory into every conversation. The control principle is explained in You Govern the Memory; another product's behavior depends on its own implementation and settings.

**How is this different from Obsidian or Notion?**
Core emphasizes UUID-stable nodes, authored parent/child order, explicit references, local retrieval and checked Git publication for human–agent work. An agent can help maintain the knowledge under your approval; it is not guaranteed to keep itself current. The Binder explains the intended interaction metaphor, not a measured superiority claim about another product.
