---
id: "198d04b9-1289-4fd1-9050-4d4bf94bfd43"
name: "Unix Delta Examples"
micro_summary: "Shows the exact operations in a proposed knowledge change."
status: "resolved"
see_also: []
scope: "Present a knowledge-tree change for review."
---
These synthetic examples illustrate the notation used by [[Present the Reviewable Delta|4036d3c5-e596-4a9c-b432-1c4ccc3162b4]].

In a real proposal, use the affected nodes' IDs and source evidence.

## 1. Show what each operation means

### A — bad proposal

```text
Replace buffering with streaming and save the result.
```

### B — good proposal

```text
[~] Report Delivery
├── [!][x] Buffer Whole Report
│   Discard reason: The trial used 95 MiB against a 64 MiB limit.
├── [+][✓] Select Streaming
│   Add micro: Records the selected way to bound export memory.
│   Add scope: Choose the next implementation approach.
│   Add content: The buffering trial exceeded the memory limit,
│                leading the team to select streaming for its next trial.
│   [←] Add see_also -> Buffer Whole Report
│       Reason: Its measurement informed this choice.
└── [+][ ] Full-Size Streaming Trial
    Add micro: Measures complete output under the agreed workload.
    Add scope: Verify complete output within 64 MiB.
    Add content: Run the export and inspect correctness and peak memory.
```

The resolved node records the choice. The open trial carries the unfinished verification.

## 2. Separate a move from a deletion

The user has approved deleting a duplicate whose content is retained in Survey Evidence.

```text
[~] Research Notes
└── [-][x] Duplicate Survey Copy
    Delete reason: Reviewed content is duplicated in Survey Evidence.

[~] Improve Response Time
├── [→][✓] Survey Evidence
│   Move: Research Notes -> Improve Response Time
│   Keep: existing ID, content and status
└── [←][~] Investigate Routing
    Add see_also -> Survey Evidence
    Reason: Survey observations motivate the routing investigation.
```

Movement changes parentage while preserving the node's identity.

## 3. Distinguish postponement from an unavailable prerequisite

```text
[~] Improve Response Time
├── [!][=] Hiring Expansion
│   Deferred reason: The user postponed staffing decisions until
│                    the routing trial is reviewed.
└── [!][/] Inspect Survey Responses
    Blocker: The authorized data owner has not supplied the export.
    Update content: Inspection can resume when the export arrives.
```

The reasons identify what would allow each branch to continue.

## 4. Show a small correction exactly

```text
[~] Request Research
└── [!][~] Interview Report
    Replace content exactly:
      Before: The employee reported dalayed replies.
      After:  The employee reported delayed replies.
    Status: This edit activates the formerly unexamined record.
```

The correction preserves the employee's reported experience.

Return to [[Present the Reviewable Delta|4036d3c5-e596-4a9c-b432-1c4ccc3162b4]].
