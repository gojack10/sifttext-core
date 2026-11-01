---
id: "c5dd1192-f75f-4369-bd9e-852f86b55121"
name: "Connect Hosted MCP"
micro_summary: "Connects a remote AI client to synced knowledge and bootstraps its agent through hosted Bash."
status: "resolved"
see_also: []
scope: "Connect a remote client to the synced knowledge workspace."
warnings: "Obtain permission for uploads and proposed writes."
position: 1024
---
## Choose local or hosted access

An agent with file and terminal access can use local Core without MCP or a subscription.

Hosted MCP gives a remote client access to the synced server copy of your knowledge.

Connection requirements:

- An active Plus or Pro subscription to SiftText.
- An AI client that supports custom remote MCP connections.

## Prepare the server copy

On the computer holding the knowledge home:

1. Run `sifttext login` if needed. The human completes browser authorization.
2. Run `sifttext account`.
3. Read the installed account/sync guide at the receipt's `guide` path.

Confirm active Plus or Pro access from the account receipt.

If an active subscription is not recognized, retain local work and contact SiftText support.

| Entitlement symptom | What needs resolution |
| --- | --- |
| Missing or unrecognized `subscription_plan` | Account plan recognition |
| HTTP 503 reporting an unavailable storage plan | Storage plan availability |

Use the knowledge-home path for `HOME` in the commands below. The application directory is a different path.

Inspect `sifttext sync status HOME` before changing sync. Preserve its existing consent and account binding.

For a first upload, propose the whole knowledge home:

> May I sync your knowledge to SiftText so your connected agents can read and update it, excluding anything you choose to keep local?

After the human approves the whole home, use the default selection of every node.

If the user chooses exclusions:

Review what would upload using the account/sync guide's policy format.

Apply the reviewed selection with `sifttext sync policy HOME FILE.json` before enabling transfers.

A declined or disabled sync stays that way until the human requests a change.

Within upload permission, use these commands:

| Command | Effect |
| --- | --- |
| `sifttext sync enable HOME` | Installs scheduling, which can start uploading immediately. |
| `sifttext sync once HOME` | Requests one cycle. |
| `sifttext sync status HOME` | Shows transfer state and any required recovery. |

Wait for pending server processing to finish before reporting that the server copy is ready.

After successful processing, exclusions remove the selected nodes from the current server copy and stop their future uploads.

Earlier versions can remain in uploaded Git history or material a client has already read, including after later exclusions.

While sync is disabled, transfers stop and the existing server copy stays accessible, growing stale as local knowledge changes.

## Connect the client

### Connection settings

| Setting | Value |
| --- | --- |
| Server URL | `https://app.sifttext.com/mcp` |
| Authentication | OAuth, with dynamic client registration when offered by the client. |
| Account | The same SiftText account used for local sync. |

The client discovers OAuth settings and registers dynamically without a manually supplied client secret.

The human completes the SiftText authorization screen in the client connection flow.

Keep native CLI credentials out of chats and connector fields.

### Claude web

1. Open Settings and its Connectors area.
2. Add a custom connector named SiftText with the server URL above.
3. Complete its connection and OAuth authorization flow.

### ChatGPT

1. Open Settings and its Apps area. If custom apps require developer mode, enable it within the user's permission.
2. Create a custom app named SiftText with the server URL above and OAuth authentication.
3. Review any trust prompt, then sign in with SiftText.
4. Select the connected app in the conversation when required.

Client labels and availability vary by plan and organization policy.

If the custom connection option is missing, report that capability as unavailable.

After connecting, inspect the conversation's available tools to establish actual access.

## Bootstrap the remote agent

Read the available tools' descriptions. The synced workspace uses this catalog:

| Tool | Responsibility |
| --- | --- |
| `workspace_prepare` | Start workspace synchronization. |
| `workspace_prepare_continue` | Continue a failed preparation after its original state has been inspected and corrected. |
| `workspace_status` | Read operation status. |
| `workspace_result` | Retrieve the original result and delivery state. |
| `workspace_finish` | Resume delivery of a retained Bash result. |
| `bash` | Start an asynchronous command in the hosted workspace. |

If the client still lists older node-CRUD tools, refresh its tool discovery before continuing.

Prepare the workspace:

1. Call `workspace_prepare` and retain its operation ID.
2. Read `workspace_status` for that operation. While pending, wait at least three seconds between polls.
3. Retrieve the original result with `workspace_result`. Continue only after success.
4. If preparation failed, inspect its failure before using the documented continuation.

Through the hosted `bash` tool, run:

```sh
sifttext instructions /workspace/knowledge
```

Supply the `request_id` required by the current Bash tool schema.

Keep that request ID with the returned operation ID so status and result calls can recover the command's original outcome.

After a lost response, preserve the same request ID and arguments. Never rerun an uncertain command with a new ID. For a retained Bash result whose delivery is pending, use `workspace_finish` to resume delivery without rerunning the command.

Read the returned [[Local Core Agent Entrypoint|5e565245-14fe-406c-977c-cf94a9be246b]] file using ordinary file commands through hosted Bash.

Before interpreting knowledge, read [[SiftText Read|5c2f91e4-9bd5-4b45-bbc4-e69c15c1d14d]] through the same hosted files.

Before an authorized save, read [[SiftText Commit|c72d552d-3499-445b-a8d5-05d0ff7824f2]] and its required procedures there.

## Approval before writes

| Control | Responsible actor |
| --- | --- |
| Workspace authentication | Server |
| Submitted-knowledge checks | Core binary |
| Human approval of each conversational proposal | Agent, with no per-proposal approval gate in hosted Bash |

Because hosted Bash has no per-proposal approval gate, the agent is responsible for following [[SiftText Commit|c72d552d-3499-445b-a8d5-05d0ff7824f2]]'s approval rules. Show the proposed change and obtain the required approval before writing.

After approval, save through Core's private worktree and checked submit procedure so Core can validate the candidate before accepting it.

## Confirm which copy was read

Ask the connected agent to read a known, permitted node from `/workspace/knowledge` and report its UUID and a short distinguishing passage. Compare that passage with the selected local accepted version. Avoid creating a node merely to check access.

If a selected accepted node differs:

| Recovery step | Action |
| --- | --- |
| Inspect | Read local `sifttext sync status HOME` and the hosted operation's delivery result. |
| Repair | Follow retained instructions while preserving both histories. |
| Reconfigure, if needed | Confirm the account binding and sharing policy before changing configuration. |

Unsubmitted local drafts and excluded nodes remain local. For a genuine conflict, preserve the original home and both histories rather than overwriting either copy or enrolling another home to bypass recovery.

Report setup's actual result and return to the user's task.

If no connector works, discuss only permitted excerpts. A file-capable agent can save approved results.
