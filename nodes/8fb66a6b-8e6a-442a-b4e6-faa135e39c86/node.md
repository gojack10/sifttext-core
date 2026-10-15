---
id: "8fb66a6b-8e6a-442a-b4e6-faa135e39c86"
name: "Local Core and AI Agents"
micro_summary: "Separates local knowledge storage and checked operations from externally supplied AI reasoning."
status: "in_progress"
see_also: []
scope: "What local Core provides, what an external agent contributes, and which client capabilities are needed."
warnings: ""
---
Local Core stores native Markdown knowledge, checks authored changes and supplies derived retrieval through a CLI. It does not include a SIFT assistant, model runtime, web interface or MCP server. You can work directly with files and commands without using any AI.

An external agent may help you reason, propose structure and edit authorized private drafts if its harness can read local files and execute commands. The user chooses the model and controls permissions; installing Core grants neither agent access nor model subscriptions. The local agent entrypoint supplies the required reading and checked publication workflow.
