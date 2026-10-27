---
id: "de7cea55-c93d-48db-a4f6-58368bf6c36f"
name: "Associate External Artifact Directories"
micro_summary: "Links directories materially used by nodes while preserving external storage and artifact custody."
status: "resolved"
see_also: []
scope: "Complete per-node directory associations during SiftText Commit without moving external projects, recursively inventorying them, or claiming artifact backup."
---
Identify each external directory materially used by an affected knowledge node.

Reuse a valid association or create a recognizable symlink under that node's durable artifacts/ directory.

Link the directory from each node that materially uses it.

Keep the external directory in place unless the user explicitly requests moving it beneath the node.

Use the durable artifact_home returned by allocation for files and links.

For a new owner, preserve files outside the disposable draft until the knowledge-only node is accepted.

Then create and verify the durable association before reporting it complete.

Preserve a different existing file or link when a name collides.

Check that a link resolves without recursively inventorying its target.

Obtain permission before examining private target contents.

Record the artifact entrance and actual target in the owning node.

Keep artifact contents out of knowledge Git.

Arrange backup separately when requested.

Return verified associations and exact gaps to [[Save and Restore Continuation|a609e77c-8164-40d2-b391-e675194fd2ad]].
