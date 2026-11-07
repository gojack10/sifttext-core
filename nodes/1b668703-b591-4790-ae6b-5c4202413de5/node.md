---
id: "1b668703-b591-4790-ae6b-5c4202413de5"
name: "Epistemic Check"
micro_summary: "Handles hosted review of knowledge before submit."
status: "resolved"
see_also: []
scope: "Use epistemic checking within Commit's approval boundary and recover from findings or an unchecked save."
position: 3072
---
## Before submit

Read this procedure from [SiftText Commit](../c72d552d-3499-445b-a8d5-05d0ff7824f2/node.md) before submitting an approved change.

Review the proposed text against its sources. Distinguish observations from interpretations and preserve the context of consequential quotations, including rejected historical claims.

Official GitHub/bundled user-facing KB nodes are durable documentation, exempt from epistemic-state checks. Core recognizes their reserved identities from its compiled bundled seed, including locally maintained copies, rather than trusting a customer-supplied exemption flag. The exemption does not cover other customer knowledge, founder development evidence or third-party material. It preserves structural, Git and file-integrity checks.

Do not request paid epistemic review of those documentation nodes or treat an old finding on them as repair work. An exempt receipt is not a PASS. Saved historical results remain available; exemption neither rewrites their verdicts nor reverses earlier charges. Mixed submissions still check non-exempt changed nodes.

## Prepare the work packet

Use the home's `sync.default` and `sync.rules` to identify the included review scope, following [Account and Synced Knowledge](../12e1e893-b0ab-4f82-b0db-37de70b95034/node.md). These upload exclusions also apply to explicit epistemic checking, independently of `sync.mode`. A proposal's display omissions do not change this policy.

Write a JSON file with a `work_record` explaining the included work and an `outline` showing only the included proposed nodes and their relationships. Include decisive `source_evidence` and an `artifact_plan` only for that scope. Leave excluded nodes, their facts, excerpts, names, paths and artifact details out of every packet field. Summarize shared events using only the included material; do not copy private context into a public summary. Included authored prose is sent as written: Core filters nodes and structured references, not the meaning of free text.

```json
{
  "work_record": "What happened, what was decided, and what supports it.",
  "outline": "The proposed nodes and how they relate.",
  "source_evidence": [{"text": "A source excerpt", "locator": "Where it came from"}],
  "artifact_plan": "Who owns any supporting artifacts and where they belong."
}
```

Core applies the home's current upload rules to the final Git candidate before collecting changed non-exempt node text. It omits excluded nodes and structured references to them from the request. The checker reviews the included work record, outline and node bodies against that context. A PASS covers this included scope only; the local receipt lists included and excluded nodes.

Mixed saves still save all approved nodes locally and apply the full structural checks. If every changed non-exempt node is excluded, submission needs no work packet, hosted request, credits or review waiver. The receipt marks review as not required, rather than passed. Changing the policy does not delete previously uploaded text or results.

## Review and submit the same draft

Commit the private draft and run:

```sh
sifttext epistemic-check HOME DRAFT COMMIT BASE WORK_PACKET.json [EXPLICIT_UUID ...]
```

This prepares the same combined candidate as submission, including merging accepted work, normalizing statuses, and applying structural checks. Use the returned `candidate` commit to render the proposal. After the user approves the proposal, submit with the same work packet, base, and explicit status IDs:

```sh
sifttext submit HOME DRAFT COMMIT BASE WORK_PACKET.json [EXPLICIT_UUID ...]
```

Core filters the packet before sending it, including when requesting reuse. The authenticated service reuses a saved PASS when the final candidate tree, filtered work packet, and checker version match. Reuse adds no model run or charge. When there is no applicable PASS, submission runs the same review. Structural checks and human approval remain required.

## When a save is blocked

| Result | Next action |
| --- | --- |
| Missing or invalid work packet | Prepare or repair the packet from available evidence. |
| Incomplete review or insufficient credits | Keep the draft. Use the recorded reason to recover the result or explain what prevented review. |
| Findings | Compare them with the draft and its evidence. Repair supported defects within the user's grant. |

For an incomplete review, explain which notes the user asked you to write, what the review checks in those notes, and why they haven’t been added. If saving without review is an option, ask the user to approve it for this save. Follow [Keeping the Human in the Loop](../6e9f23b3-3fbe-4dbd-9b58-6842438d2d01/node.md).

After explicit approval to save without review, use:

```sh
sifttext submit HOME DRAFT COMMIT BASE [WORK_PACKET.json] [EXPLICIT_UUID ...] --skip-epistemic-check
```

The receipt marks the save as unchecked and records the waiver. Check it later with the user’s permission.

If the check returned findings, explain the actual findings. Use `--accept-despite-findings` only if the user explicitly approves saving this draft with those findings. Include the work packet. The receipt preserves the findings and records the override. Both flags apply to one submission and preserve the structural, integrity, and consent gates.

Logged-out and free-account local saves follow the local save path. Own-account replication of already accepted history retains findings for repair while applying its existing integrity and consent checks.

## Understand credits

Both paid plans include both checkers:

| Purchase | Credits |
| --- | --- |
| Plus, $20/month | 2,000 per billing period |
| Pro, $50/month | 10,000 per billing period |
| Subscriber top-up, $10 | 2,500 purchased credits |
| Credits only, $10 without a subscription | 1,000 purchased credits |

Credits account for provider usage at 1,000 credits per $1, including fractional usage. The number of checks varies with their cost.

Included credits reset each billing period and are spent before purchased credits. Purchased credits carry forward.

The last 25% of the included monthly allowance is reserved for epistemic checking. Optional slop use must stop at that reserve.

Automatic top-ups are disabled.

The service reserves capacity before running a check and charges once after durably saving a completed result, including a result with findings.

Technical failures consume zero customer credits.

A saved valid result is reused at the same checker version when its text and context are unchanged. Reuse adds no charge.

Use `sifttext credits` to read the current balance.

With permission for one $10 purchase, request a checkout URL:

```sh
sifttext credits --buy-10
```

The command uses the saved login to print a Stripe checkout URL. Credits are granted after the human completes the $10 payment in Stripe.

If the response is lost, retry with the printed request key:

```sh
sifttext credits --buy-10 REQUEST_KEY
```

Reuse that key for the same purchase. A new key starts a separate checkout.

## Recover a result or delete it

After a lost connection, retrieve the request using its ID, printed before the final credit line:

```sh
sifttext check-result REQUEST_ID
```

Retrieval reads the account's saved response without a new run or charge.

SiftText retains the submitted text and results until the customer deletes them:

```sh
sifttext check-result REQUEST_ID --delete
```

Deletion ends retrieval and reuse of that result. The completed service's charge remains.
