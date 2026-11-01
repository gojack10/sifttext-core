---
id: "f47c31ba-8037-4a76-976c-75d4469d7cf0"
name: "Explain SiftText From Public Sources"
micro_summary: "Explains SiftText for a user's project without installing it or reading private knowledge."
status: "resolved"
see_also: []
scope: "Give a read-only product explanation with one illustrative tree and a source-grounded account of current capabilities."
position: 4096
---
## Read only what the explanation needs

This route only explains the product.

Obtain separate permission for operations outside that explanation:

- Installing SiftText.
- Accessing an account.
- Saving knowledge.

For an explanation adapted to the user's project, start with these public sources:

| Read | Purpose |
| --- | --- |
| [Local Core and AI Agents](../8fb66a6b-8e6a-442a-b4e6-faa135e39c86/node.md) | Distinguish SiftText's work from the agent's. |
| [Tree Thinking / Problem Solving](../d3234392-c9ce-4ac7-bc08-88456ea34a30/node.md) | Explain how related questions form a tree. |
| [Messy In, Structured Out](../ec26e21e-bd1c-40e6-89b1-a1daafc10851/node.md) | Start from rough input the user can correct. |
| [Probing the Unknown](../42b1cde8-5478-4b14-938f-c10869ffda03/node.md) | Keep failed approaches useful to later work. |

Follow a relevant branch when the question needs more detail. Do not read the whole bundle by default.

Before showing any tree delta, including a fictional, read-only example, follow the required reads and selected procedures of:

- [SiftText Commit](../c72d552d-3499-445b-a8d5-05d0ff7824f2/node.md).
- [Dialogue](../d00ab6fd-b475-49ef-b37a-812807879308/node.md).

Reuse unchanged reads.

## Explain the division of work

The agent investigates the human's chosen question within their permission and proposes an interpretation for review.

SiftText keeps the reviewed reasoning in linked Markdown nodes with Git history. Later sessions can read that record and reconsider the interpretation.

Each node holds one meaningful part of the work, with children for the smaller questions needed to understand it.

Use source links to make evidence elsewhere reachable without moving it into the branch.

The agent can gather material through available, authorized browser or data tools and remains responsible for interpreting what it finds.

## Use one concrete example

Adapt one example to the user's actual work. Label invented sources, measurements and results as fictional. Distinguish the example from this real instruction tree and from saved user knowledge.

For a research question, a fictional starting tree could be:

```text
Does earnings-call language add useful information?
├── Define the signal and comparison
├── Find filings and earnings materials
│   └── Record when each document became available
└── Evaluate the signal without future information
```

The following fictional delta preserves a finding that some transcripts appeared after the simulated trade, rejecting that timing rule and leaving the corrected result unknown.

### Example labels

- `demo-*`: fictional existing nodes, not real UUIDs.
- `new:*`: temporary labels for fictional creations.

Only affected ancestry is shown.

```text
[~] Earnings Research (demo-project)
└── [~] Fair Historical Test (demo-test)
    ├── [!][x] Original Timing Rule (demo-old)
    │   micro: Defines when transcripts enter the simulation.
    │   Set status: discarded.
    │   Set discard_reason: Some transcripts were unavailable
    │                       at the simulated entry time.
    ├── [+][✓] Transcript Timing Finding (new:timing)
    │   Add micro: Records availability of the analyzed text.
    │   Add scope: Establish when these transcripts were public.
    │   Add body:
    │     Observed conditions: In the fictional source records,
    │     some transcripts were published after trade entry.
    │     Inference and response: Reject the original timing rule
    │     because it used information unavailable at trade entry.
    └── [+][ ] Corrected Timing Test (new:retest)
        Add micro: Tests the signal using available information.
        Add scope: Evaluate the signal without future documents.
        Add body:
          Observed conditions: The original timing rule failed.
          The corrected result is unknown.
          Inference and response: Verify availability before using
          each document. Flag unknown publication times. Rerun
          with the same comparison and costs, keeping the
          hypothesis unproven until the new result is reviewed.
```

The retained record lets a later agent trace the correction back to the timing evidence and continue the unfinished investigation.

Explain current capabilities from the matching release documentation or an explicit maintainer clarification, confirming tool availability before proposing a workflow.

A SiftText plan and the user's AI-client subscription are separate purchases.

Return to the user's question. Do not create this fictional project unless asked.
