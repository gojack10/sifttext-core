---
id: "de7cea55-c93d-48db-a4f6-58368bf6c36f"
name: "Associate External Artifact Directories"
micro_summary: "Links durable directories and places approved ad hoc artifacts under their owning nodes."
status: "resolved"
see_also: []
scope: "Propose and complete per-node artifact custody without moving durable projects or claiming unperformed transfers or backups."
---
Identify each external directory materially used by an affected knowledge node.

Reuse a valid association or create a recognizable symlink under that node's durable artifacts/ directory.

Link the directory from each node that materially uses it.

Keep durable external projects in place and link them.

For a materially cited ad hoc file, propose moving it into its owning node’s artifacts/ directory. Use `rsync --remove-source-files` rather than `mv` for approved ad hoc file transfers. Show the exact source, destination and operation in the Commit proposal.

Mark missing files unavailable. Distinguish recovered source from an original retained artifact. Do not manufacture an original file or claim that a proposed transfer happened.

Use the durable artifact_home returned by allocation for files and links.

For a new owner, preserve files outside the disposable draft until the knowledge-only node is accepted.

Then create and verify the durable association before reporting it complete.

Preserve a different existing file or link when a name collides.

Check that a link resolves without recursively inventorying its target.

Obtain permission before examining private target contents.

Record the artifact entrance and actual target in the owning node.

Keep artifact contents out of knowledge Git.

Use [[Session Evidence|3651b9b0-fc21-4c67-9d79-84ef3af963ce]] for conversation JSONL rather than treating an active harness file as disposable scratch.

Arrange backup separately when requested.

Return verified associations and exact gaps to [[Save and Restore Continuation|a609e77c-8164-40d2-b391-e675194fd2ad]].
