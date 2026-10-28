---
id: "6da0b35a-955a-4ad2-bdce-2994ba26119c"
name: "Core Maintenance"
micro_summary: "Guides agents through recoverable SiftText problems while continuing the user's task."
status: "resolved"
see_also: []
scope: "Investigate and repair SiftText within the user's existing permission."
---
## Continue the task

Handle recoverable problems within the user's existing permission. Explain the repair briefly when it affects progress, so the user knows what is happening while you continue the task.

Before raising a conflict, investigate whether the available sources already settle it. When a serious consequence still requires the user's judgment, explain the affected work and what each choice would do. Ground your recommendation in those consequences before asking one concrete question.

Preserve the user's knowledge and recovery files throughout repair.

## Find the source

`sifttext source-code` returns a checkout matching the installed source revision.

Read its agent and build instructions before changing code.

Use [[Managed Releases and Updates|8b5361e1-20d7-42a9-9dca-26ba3bac9f25]] for supported update recovery.

The current assignment determines whether code changes and upstream contributions are authorized.

Keep private knowledge and credentials out of contributions.

## Example 1 — routine recovery

The following A/B examples are synthetic.

### Input

> Update my Mac to the official build.

### Situation

The updater needs a missing history record that the agent has found in a retained copy.

### A — bad response

> The update needs a missing history record. How should I repair it?

### B — good response

> I'm restoring the missing installation record from your saved history so the update can continue with your knowledge unchanged.

The agent completes the repair and continues updating.

## Example 2 — client permission required

### Input

> Set up the cloud assistant for my client work.

### Situation

The task includes client interviews whose agreement requires written approval for external storage.

The user also has an offline assistant that processes files entirely on the company's Mac.

### A — bad response

> I recommend keeping the client notes on this Mac. Should I do that?

### B — good response

To make these interviews available in ChatGPT, I would upload their files to your SiftText account. They contain medical histories linked to named participants, so this would move identifiable health information onto a service your company does not control.

The consent form requires written client approval for that transfer, which I haven't found.

I'm keeping them with your offline assistant while finishing the rest of setup. Do you have that approval?

## Return

Continue the original task after recovery and report the actual result.
