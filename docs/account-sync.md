# Account and synced knowledge

Use this guide when setting up paid services or responding to the guide path returned by `sifttext login` or `sifttext account`. Set `HOME` in the commands below to the knowledge-home path returned by setup.

Persistent harness instructions should point to the installed entrypoint so future sessions can retrieve the current guidance.

## Read the account receipt

Run `sifttext account` to obtain the server's current account facts.

| Field | Meaning |
| --- | --- |
| `paid` | Current paid-access decision. |
| `subscription_status` | Current subscription state. |
| `subscription_plan` | Exact plan when known. A missing, null or unrecognized value remains unknown. |

Use the current receipt to establish access. Retain local work if account confirmation is unavailable and resolve authorization before enabling transfers.

Follow the installation guide for account setup. Browser authorization and payment are completed by the human.

Plus and Pro accounts can use both checkers.

Hosted MCP and knowledge sync are also included.

Confirm browser availability from the connected client's tools.

Claim official integration support only with maintainer confirmation.

For remote-client OAuth setup and the hosted instruction bootstrap, follow [Connect Hosted MCP](../nodes/c5dd1192-f75f-4369-bd9e-852f86b55121/node.md).

## Checker credits

Use `sifttext credits` to read the current checker balance.

With permission for one $10 purchase, run `sifttext credits --buy-10` for a Stripe checkout URL. Credits are granted after the human completes payment in Stripe.

Automatic top-ups are disabled.

If the response is lost, retry with the printed request key to recover the same checkout.

Before explaining checker billing, read [Epistemic Check](../nodes/1b668703-b591-4790-ae6b-5c4202413de5/node.md). Its credit and recovery sections also explain how to retrieve or delete saved results.

## Establish upload permission

MCP works on a server copy of the user's knowledge, so it requires permission to sync that knowledge. Inspect `sifttext sync status HOME` before asking about setup.

| Recorded decision | Action |
| --- | --- |
| Enabled | Preserve consent and the existing account binding. |
| Enabled legacy enrollment | Migrate that enrollment while preserving its repairs. |
| Declined or disabled | Wait for the human to request a change. |
| No decision, active paid account | Ask for first-upload permission. |

For the first upload, ask:

> May I sync your knowledge to SiftText so your connected agents can read and update it, excluding anything you choose to keep local?

Wait for the answer before enabling sync. Whole-home knowledge is the proposed scope, and the human can choose exclusions before any transfer.

| Answer | Configuration |
| --- | --- |
| Yes, with exclusions | Record the policy first, then run `sifttext sync enable HOME`. |
| Yes, everything | Confirm the include policy, then run `sifttext sync enable HOME`. |
| No | Run `sifttext sync decline HOME` within configuration permission. |

Enabling installs native scheduling, which can start the first upload on its next tick.

A scheduling failure leaves setup incomplete until the schedule is repaired within permission or sync is disabled.

A login or plan change preserves the recorded sharing decision. Resolve account-binding conflicts within the existing home instead of enrolling another home to bypass them.

## Choose what is shared

The home's `.sifttext/config.json` stores sharing settings alongside the other local settings.

| Setting | Values |
| --- | --- |
| `sync.mode` | `unanswered`, `declined`, `disabled`, `enabled` |
| `sync.default` | `include` or `exclude` |
| `sync.rules` | Node rules with `node`, `action` and `recurse` fields |

Use `sifttext sync policy HOME FILE.json` to change selection while preserving consent and the rest of the config. Supply a regular JSON file such as:

```json
{
  "default": "include",
  "rules": [
    {"node": "00000000-0000-4000-8000-000000000001", "action": "exclude", "recurse": true}
  ]
}
```

A node's direct rule takes priority. Without a direct rule, selection uses its nearest ancestor's recursive rule or falls back to the default policy.

| Rule condition | Behavior |
| --- | --- |
| Single-node rule | Descendant inheritance stays unchanged. |
| Opposing actions for one UUID | Configuration is rejected. |
| Invalid configuration | Transfers stop. |
| UUID absent from current knowledge | Its rule remains saved. |

To share only one tree, set the default to `exclude` and add a recursive `include` rule for its root.

Publication uses the selected nodes' current accepted text. Review that text for private material because manually copied words and links are included.

| Material | Publication behavior |
| --- | --- |
| Selected node with an excluded parent | Becomes a hosted root. |
| Structured references and child links to excluded nodes | Omitted from publication. |
| Full local knowledge and private Git ancestry | Retained locally. |
| Session provenance and artifacts | Excluded from new publication. |
| Local settings and credentials | Kept outside the publication. |

Excluding a previously uploaded node removes its current hosted copy once the server finishes processing that change. Check the upload and pending receipts before reporting that an exclusion has taken effect.

Earlier uploaded versions can remain in hosted Git history or copies already read elsewhere, so removing those versions requires a separately authorized history-purge operation that this release does not provide.

Configure an independent backup separately if the user requests one.

## Run or stop sync

Native scheduling runs `sync tick` every 15 seconds in the user's session.

| Platform | Scheduler requirement |
| --- | --- |
| macOS | User LaunchAgent. |
| Linux | Active systemd user manager and DBus. |

Ticks resume when the computer wakes.

| Status detail | Use |
| --- | --- |
| Saved definition path | Locate the installed schedule. |
| Active schedule | Determine whether native scheduling is running. |
| Bounded runner error | Inspect the latest failure. Identical failures reuse the same record. |

| Command | Effect |
| --- | --- |
| `sifttext sync status HOME` | Reads local state without logging in or transferring knowledge. |
| `sifttext sync once HOME` | Runs one cycle under existing consent. |
| `sifttext sync once HOME --quiet` | Suppresses successful status output while keeping errors visible. |
| `sifttext sync run HOME` | Runs the compatibility foreground loop, checking consent between cycles. |
| `sifttext sync disable HOME` | Blocks further transfers and removes native scheduling. |

| Disable condition | Behavior |
| --- | --- |
| A cycle holds the home lock | Waits for that cycle to finish. |
| Removing the schedule fails | Reports the unfinished teardown and keeps consent disabled. |

Disabling retains the selection and both knowledge copies, along with recovery work. The hosted copy can become stale while transfers are stopped.

## Understand an upload receipt

A successful upload acknowledges durable server storage of the exact Git commit, after which the server owns completing the save. While `remote_pending` shows processing, the client waits without resending that contribution or building another publication.

For a conflict or blocked completion shown in status, follow its retained recovery instructions before reporting the sharing change as finished.

## Compatibility and recovery

The service must supply these values before transfer:

| Requirement | Value |
| --- | --- |
| Protocol | `1` |
| Operation | `snapshot-import-v1` |
| Publication | `selected-current-v1` |
| Delivery | `durable-receipt-v1` |
| Account home | Stable authenticated `home.id` |
| Decoded bundle limit | 32 MiB |
| JSON limit | 44 MiB |

During a compatibility pause, continue local knowledge work while preserving the server-confirmed home binding for recovery.

| Account history | First selected publication |
| --- | --- |
| Empty | A parentless snapshot of the selected current knowledge. |
| Already hosted | A descendant of the hosted history after local import or reconciliation. |

These publication histories keep private local ancestry out of uploads.

| Publication generation | Session provenance |
| --- | --- |
| Earlier client uploads | Remains retained in hosted history. |
| New selected publication | Excluded. |

When Git has no common ancestor, retain both histories for reconciliation before publishing.

### Moving from an older client

Migration changes the old client's saved `enabled` flag to `false`, keeping that client stopped while the new client follows the local sharing settings.

Keep older full-history clients disabled for this home.

For custom sync jobs left in place by migration, identify the schedule and stop the job within permission before removing that schedule.

### Receiving hosted edits

| Receiving step | Action |
| --- | --- |
| Retain | Save hosted edits in the separate Git publication repository. |
| Translate | Apply their changes to the recorded full-local version. |
| Check | Submit the resulting contribution through Core's combined-candidate gate. |

The client preserves local-only nodes and omitted relationships when preparing changes for Core's reconciliation.

Publication pauses for the following ambiguities so the retained incoming history can be reviewed:

| Ambiguity | Required review |
| --- | --- |
| Deletion or move affecting private knowledge | Preserve and reconcile the local structure. |
| Missing baseline | Establish the correct relationship between the retained histories. |
| Topology that would change sharing permission | Review the permission change locally. |
| UUID collision | Resolve the conflicting identities without replacing either history blindly. |

Inspect `sifttext sync status HOME` and follow the exact repair guide under knowledge-write authority. Preserve the original request and both histories throughout recovery.

After setup, report the actual result and return to the user's task.
