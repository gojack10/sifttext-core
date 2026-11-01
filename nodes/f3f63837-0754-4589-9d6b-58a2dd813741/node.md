---
id: "f3f63837-0754-4589-9d6b-58a2dd813741"
name: "Tree Development Examples"
micro_summary: "Shows how a tree exposes the development of an investigation."
status: "resolved"
see_also: []
scope: "Choose meaningful parentage and bounded reading paths."
position: 1024
---
Use the relevant synthetic case when organizing a tree.

## 1. Preserve how a finding arose

While attempting to remove an image-loading delay, an agent's failed refactor exposed an existing deferred path that solved the problem.

### A — bad structure

```text
[✓] Speed Up Startup
├── [✓] Image Loading Blocks Startup
├── [x] Refactor the Entire Image Pipeline
├── [✓] Existing Deferred Path Was Overlooked
└── [✓] Use the Existing Deferred Path
```

### B — good structure

```text
[✓] Speed Up Startup
├── [✓] Image Loading Blocks Startup
└── [✓] Remove Image Loading From Boot
    ├── [x] Refactor the Entire Image Pipeline
    │   ├── [x] First Refactor Attempt
    │   └── [✓] Existing Deferred Path Was Overlooked
    ├── [✓] Use the Existing Deferred Path
    │   Reference -> Existing Deferred Path Was Overlooked
    └── [✓] Verify Startup and Image Quality
```

The finding belongs under the work that produced it, where it remains discoverable after that approach is discarded.

The startup request resolves after its requested outcome is verified.

## 2. Read the branch needed for the question

For “What happened to startup?”, begin with:

```text
[✓] Speed Up Startup
├── [✓] Image Loading Blocks Startup
└── [✓] Remove Image Loading From Boot
```

Open the refactor branch when investigating why it was abandoned.

## 3. Allow independent questions to remain siblings

```text
[~] What Explains Customer Drop-Off?
├── [~] Pricing Friction
├── [~] Onboarding Confusion
└── [~] Missing Team Workflow
```

Create deeper groups when findings establish a useful relationship between these independent explanations.

## 4. Reparent with evidence

The source of an attribution repair identifies Published Identity Finding as its motivation.

```text
[~] Release Investigation
└── [✓] Published Identity Finding
    └── [→][✓] Repair Attribution
        Move: Release Investigation -> Published Identity Finding
        Keep: existing ID and evidence
```

Keep the original evidence and record why the node moved.

## 5. Organize instructions by responsibility

A writing procedure can expose the branches needed for the current task:

```text
[~] Record Work
├── [~] Form the Proposed Change
│   └── [~] Present the Tree Delta
│       └── [✓] Unix Delta Examples
├── [~] Review Meaning and Reading
└── [~] Save and Restore Continuation
    ├── [~] Reconcile Affected Uses
    └── [~] Verify the Saved Result
```

The procedure states which branches are required and when an example applies.

## 6. Preserve the scope of a cancellation

A user withdraws one audit requirement:

```text
[~] Deliver the Knowledge Set
├── [+][✓] Withdraw Named Audit Requirement
├── [!][x] Perform the Withdrawn Audit
│   Reason: The owner withdrew this requirement
└── [~] Finish the Remaining Delivery Work
```

Retain completed observations and update the instructions that depended on the withdrawn requirement.

Return to the procedure that requested the structural review.
