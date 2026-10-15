---
id: "fba3b413-d79c-4a42-91df-7f2e67ae3027"
name: "Visual Technical Reports"
micro_summary: "Skill for turning technical evidence into polished single-file HTML reports: conclusion-first, evidence-backed, visually explanatory, and safe to share."
status: "resolved"
see_also: []
scope: "Reusable skill for generating polished, self-contained technical explanation artifacts, especially single-file HTML reports with diagrams, timelines, evidence tables, callouts, and conclusion-first structure."
warnings: "Diagrams must explain causal, resource, timing, or dependency structure. Do not add visuals as decoration.\nDo not invent measurements, precision, commands, org details, or root causes. Label schematic diagrams as schematic when proportions are illustrative.\nPrefer standalone single-file HTML with inline CSS and no external network dependencies unless the user asks otherwise.\nRedact secrets, tokens, credentials, and irrelevant private context before producing a shareable report.\nKeep conclusion and evidence separate: show the answer first, then the proof trail, then remediation or next actions."
---
## Skill: Visual Technical Reports

Use this skill when the user wants a polished report, diagram, failure explanation, performance story, or executive-readable technical artifact from messy evidence.

The target artifact is usually a single self-contained HTML file with inline CSS: no external network dependencies, readable in a browser, and easy to send to a human. The report should make the technical conclusion obvious first, then show the proof trail.

## Execution Protocol

1. **Find the decision or thesis.** Start from the conclusion the reader needs: what failed, what bottleneck dominates, what path is critical, what changed, or what action is safe.
2. **Extract evidence.** Pull concrete facts from commands, logs, code, measurements, metadata, or user-provided artifacts. Separate observed facts from interpretation.
3. **Choose the report shape.** Incident reports often need a conclusion callout, identity/evidence table, execution path, non-causes ruled out, commands used, and remediation notes. Performance reports often need a verdict, resource/stage map, critical-path timeline, measured comparison table, bottleneck explanation, and remaining levers.
4. **Design diagrams around causality.** Use visual structure only where it explains the system: dependency chains, timelines/Gantt lanes, stacked bars, before/after comparisons, flow order, resource ownership, or failure boundaries. Keep scales honest; mark schematic visuals as schematic when exact timings are not available.
5. **Write for scanability.** Lead with the conclusion, use short sections, callouts, tables, code blocks, muted colors, and labels that stand on their own. The reader should understand the answer before reading every detail.
6. **Protect shareability.** Inline the CSS, avoid external assets unless requested, redact secrets/tokens, and avoid leaking private context not needed for the report.
7. **Produce the artifact.** In a coding harness, save the HTML file and tell the user the path. In chat-only contexts, provide the complete artifact or a compact draft the user can paste into a file.

## Style Anchors

The founder examples behind this skill had two recurring strengths: conclusion-first investigation reports with evidence tables and non-causes ruled out; and critical-path performance reports with resource lanes, timeline comparisons, measured tables, and explicit bottleneck reasoning.

The common pattern is not “make it pretty.” The common pattern is: make the causal structure visible so a human can trust the conclusion quickly.