---
id: "a0805f9a-dc3f-417e-81f9-1521c90e1e71"
name: "Common Questions"
micro_summary: "Answers common questions about using SiftText."
status: "resolved"
see_also: [{"target":"fbaa3012-2235-49cb-88c0-67fdbc366c36","reason":"Explains the human's control over reading and saving memory."},{"target":"5f146883-03c8-416c-be6b-c52cbcb055f2","reason":"Introduces SiftText using the binder metaphor."}]
scope: "Answer a user's question and direct them to further guidance when needed."
warnings: ""
---
## Can I delete a node?

Use a discarded node when you want to keep the reason an approach was rejected.

Committed Git history keeps earlier versions recoverable.

## Where is my knowledge stored?

Your knowledge lives in your local Git repository.

Enabling cloud sync creates a server copy of the selected knowledge that SiftText then keeps updated through scheduled transfers.

SiftText's cloud MCP tools access the server copy. Follow [[Connect Hosted MCP|c5dd1192-f75f-4369-bd9e-852f86b55121]] to connect a remote client.

By default, sync includes every node in that knowledge home once you approve uploading it. You do not need to select trees individually to share the whole home.

If you choose to keep anything local, set those exclusions before enabling sync.

Later exclusions take effect in the current server copy, while earlier uploaded versions remain in its Git history.

## Can I export my trees?

Clone it or copy it while preserving symlinks.

Generated SQLite and outline files are rebuildable views.

Sharing a repository requires the owner's permission and a privacy review.

## What happens if I disconnect a model?

Core needs no model connection.

Knowledge files and Git history remain available to ordinary tools and the CLI.

## Can an agent make mistakes?

Yes, so read a proposed change and its evidence before approving it.

Keep incorrect drafts inspectable and repair them through the checked workflow.

## How many trees or nodes can I have?

There is no fixed node-count ceiling.

## How is this different from automatic model memory?

Core keeps explicit knowledge files with Git history.

You or an authorized agent choose what to read and what to save.

Core does not inject memory into a conversation automatically.

## How is this different from Obsidian or Notion?

The current agent maintains the knowledge under your approval.

See [[The Binder|5f146883-03c8-416c-be6b-c52cbcb055f2]] for the intended interaction metaphor.
