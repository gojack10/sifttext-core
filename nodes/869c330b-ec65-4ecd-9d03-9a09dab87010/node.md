---
id: "869c330b-ec65-4ecd-9d03-9a09dab87010"
name: "AI Slop Writing Check"
micro_summary: "Checks supplied prose on demand and guides revisions without changing its facts or the user's voice."
status: "resolved"
see_also: []
scope: "Use hosted slop checking for writing the user wants reviewed, while preserving meaning, permission and credit limits."
position: 2048
---
## Choose the text and purpose

Follow this procedure when the user asks to check writing.

```sh
sifttext slop-check FILE
```

The command waits for the supplied file's result, then prints it with the request ID before the final credit line.

Obtain permission before sending private text to hosted checking.

Slop checking runs only when requested.

Obtain separate permission before rewriting.

## Read findings in context

Read each finding against the original text and its intended reader. The checker may return classifications, quoted spans or reason codes. Do not invent a prose explanation that the result did not supply.

Remove wording that contributes nothing. Make vague claims concrete using the sources.

Preserve the sources' meaning when revising. This includes exact quotations and technical terms as well as qualifications that affect the reader's decision.

Revise in the user's voice.

If the user asked for a revision, repair meaningful findings within that grant and check the revised text when authorized. Do not weaken an accurate statement merely to obtain a pass. A clean result is a checker judgment, not human approval or a guarantee of good writing.

For knowledge prose, revise with [[Write for the Reader|10f2d4ab-3318-45d8-9486-f26a817c829e]].

Before submitting revised knowledge, read [[Epistemic Check|1b668703-b591-4790-ae6b-5c4202413de5]] to handle Core's epistemic review.

## Credits and saved results

Both paid plans include this checker, with the last 25% of included monthly credits reserved for epistemic checking. Explain a reserve refusal as a credit limit that can stop slop use before the total balance reaches zero.

Read the credit and recovery sections of [[Epistemic Check|1b668703-b591-4790-ae6b-5c4202413de5]] before explaining billing.

Retrieve a saved response without another run or charge:

```sh
sifttext check-result REQUEST_ID
```

Before checking sensitive writing, explain that the submitted text and results are retained until deleted. Results may quote the text.

Return the actual findings or authorized revision to the user's writing task.

For a knowledge change, follow [[SiftText Commit|c72d552d-3499-445b-a8d5-05d0ff7824f2]]'s approval and publication procedure.
