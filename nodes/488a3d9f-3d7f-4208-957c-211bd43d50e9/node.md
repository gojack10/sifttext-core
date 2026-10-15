---
id: "488a3d9f-3d7f-4208-957c-211bd43d50e9"
name: "Push vs Pull Surfaces"
micro_summary: "Distinguishes context supplied up front from references read on demand, without assuming automatic delivery to an agent."
status: "in_progress"
see_also: []
scope: "Distinguish supplied context from on-demand navigation in local files, without promising automatic push, reminders or reader compliance."
warnings: ""
---
Push and pull describe two ways a human or configured harness supplies context. Push means supplying selected text up front; pull means reading a node or reference on demand. They are interaction choices, not two automatic Core delivery services.

In local Core, warnings are authored metadata near the start of `node.md`. Ordinary full-file reads and `sifttext read` include that text; neither automatically injects it into an agent's context or attaches methodology reminders. The human or configured harness must actually supply required instructions, and the reader must inspect them.

On-demand navigation uses child symlinks and `children.order`, generated outlines, explicit `see_also` references and narrative citations. The reader chooses which files to open; an ordinary `node.md` read does not append child bodies or reference targets.

For context-critical content, place concise warnings in the node and make prerequisite reads explicit in the applicable entry procedure. A reference alone does not prove delivery, reading or compliance. Supplying context up front can reduce missed prerequisites; selecting references on demand preserves attention. Check the actual harness's behavior rather than assuming a guaranteed push channel.

When authoring, keep essential cautions near the relevant instructions and make required reads explicit. References can preserve background, related work and optional depth without loading all of it. Actual delivery and compliance still need separate evidence.