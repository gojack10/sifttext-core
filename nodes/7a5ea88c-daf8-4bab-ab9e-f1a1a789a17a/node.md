---
id: "7a5ea88c-daf8-4bab-ab9e-f1a1a789a17a"
name: "Grounded Node Examples"
micro_summary: "Shows how observations support an attributed conclusion."
status: "resolved"
see_also: []
scope: "Write knowledge records whose reasoning a new reader can inspect."
position: 1024
---
Use these synthetic examples when following [[Write Grounded Stable Nodes|d0549d1b-e8b2-4160-81ed-62115597464a]].

## 1. Derive a requirement from the actual statement

### Input

Maya says about a payroll export: “I need to use it on the train when there's no signal. I don't care about a fancy dashboard.”

An agent then opens the CSV with networking disabled and observes that the hosted dashboard cannot load.

### A — bad record

An unsupported response would impose a 50 MB release limit based on Maya's request for offline use.

### B — good record

    ## Observed conditions

    Payroll Export Interview, April 4 preserves Maya's complete statement
    about using the export on a train without a signal.

    The CSV opened with networking disabled in the April 5 prototype run.

    The hosted dashboard required a connection.

    ## Inference and response

    I select the CSV export because it worked under the connectivity
    condition Maya described.

## 2. Reconsider a method when its conditions change

### Input

An agent selected serial execution after two builds writing `/tmp/report-out/report.json` replaced each other's output.

The new configuration gives the builds separate output directories.

### A — bad response

An unsupported response would run every build concurrently solely because the old serialization rule was written earlier.

### B — good record

    ## Observed conditions

    Serialize Shared Directory Writes records a wrong-request result
    caused by two builds writing the same report filename.

    The new configuration gives each build a separate report directory.

    ## Inference and response

    I propose a concurrent trial with the new paths to determine whether
    other shared resources still require serial execution.

An explicit user instruction to serialize this production host remains binding until the user changes it.

## 3. Explain why a proposed change follows

### Input

A procedure that requests incident facts and an action produced a note recommending a rewrite from the single observation “Export Q7 failed.”

### A — bad record

An unexplained response would require a new format without identifying how the current procedure allowed the unsupported rewrite.

### B — good record

    ## Observed conditions

    The authoring and review procedures accept a note that lists an
    incident followed by an action, leaving its reasoning unexplained.

    ## Inference and response

    I recommend requiring the author to explain how the incident evidence
    supports the proposed action, so a reviewer can evaluate that connection.

## 4. Preserve uncertainty about a cause

### Input

Export Q7 returns `permission denied: /reports/Q7.csv`. The available log omits the process identity and directory permissions.

### A — bad response

Calling the exporter defective from this error alone would skip inspection of the refused file operation.

### B — good record

    ## Observed conditions

    The request log records “permission denied: /reports/Q7.csv” without
    identifying the process or showing the destination's permissions.

    ## Inference and response

    I will inspect which user ran the export and whether that user can
    write to the destination before choosing a repair.

## Quoting sources

Identify who supplied consequential wording and where the full source can be recovered.

Mark an omission with `[…]` so the reader can recognize where words were removed.

Use square brackets to make a correction visible within a quotation.

For a substantial rewrite, use an attributed paraphrase beside its source reference.

Return the draft to its author or reviewer.
