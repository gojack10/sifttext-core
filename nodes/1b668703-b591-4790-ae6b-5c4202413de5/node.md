---
id: "1b668703-b591-4790-ae6b-5c4202413de5"
name: "Epistemic Check"
micro_summary: "Handles hosted review of knowledge before submit."
status: "resolved"
see_also: []
scope: "Use epistemic checking within Commit's approval boundary and recover from findings or an unchecked save."
---
## Before submit

Read this procedure from [[SiftText Commit|c72d552d-3499-445b-a8d5-05d0ff7824f2]] before submitting an approved change.

Review the proposed text against its sources. Distinguish observations from interpretations and preserve the context of consequential quotations, including rejected historical claims.

For a logged-in paid account with credits, Core's binary checks changed node Markdown in the immutable combined candidate before accepting submit. The binary sends those changes itself. An agent's claim that a check passed cannot satisfy this gate.

| Result | Submit behavior |
| --- | --- |
| Passing check | Continue the checked publication procedure. |
| Findings | Refuse acceptance and preserve the draft for repair. |
| Credit shortage or technical failure | Save marked unchecked and explain the missing check. |
| Logged-out or free offline use | Keep the existing local save path. |

Hosted acceptance must independently enforce its checking requirement.

## Check supplied text on demand

```sh
sifttext epistemic-check FILE
sifttext epistemic-check --packet PACKET.json
```

Text mode reviews the supplied text. Packet mode stages the review as follows:

1. Establish the work record.
2. Review the proposed outline.
3. Review complete node bodies against that context.

Include the decisive source context in the input. The checker reviews only the material supplied.

A packet has this shape:

```json
{
  "work_record": "What happened and the sources supporting the proposal.",
  "outline": "Node identities and proposed relationships.",
  "nodes": [{"id": "NODE_UUID", "markdown": "Complete node Markdown."}],
  "artifact_plan": "Who owns the artifacts and where they belong, if applicable."
}
```

Send only material authorized for hosted processing.

Core rejects encoded JSON requests above 128 KiB. Reduce the supplied text or packet to leave room for JSON fields and escaping.

## Handle findings

Compare each finding with the affected text and its evidence. Repair a supported defect within the user's grant. If a proposed correction would lose supported meaning, explain that with evidence. Never invent facts to satisfy a checker.

| Failed stage | Repair |
| --- | --- |
| Work record | Correct the account of the work while retaining sound drafts. |
| Outline | Repair the organization and regenerate the affected bodies. |

Report the classifications or explanations actually returned. Do not turn a reason code into a purported model-written explanation.

Ask for approval of any correction beyond the existing grant.

Continue Commit's publication procedure after review.

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

## Explain an unchecked save

Unchecked means epistemic review remains pending. Report the reason for the missing check, keeping the saved knowledge's quality unassessed.

| Reason | Explain or offer |
| --- | --- |
| Credits ran out | Offer renewal or an explicit top-up. |
| Technical failure | Explain that the check consumed zero customer credits. |

Check the backlog only with the user's permission, including after credits or service return.

Return to [[SiftText Commit|c72d552d-3499-445b-a8d5-05d0ff7824f2]] with the actual result and unfinished work.
