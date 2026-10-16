---
id: "372802f9-dcb1-41eb-92cd-e108cebb949a"
name: "Migrate Hosted Knowledge"
status: "resolved"
micro_summary: "Guides an authorized, source-accounted move from hosted SiftText into a new local knowledge home."
see_also: []
---
# Migrate from hosted SiftText

Use this guide when a user wants to move knowledge from the old hosted SiftText into local Core. Ask which account and trees they want to move. Obtain permission to export those records.

Hosted export and conversion currently require maintainer assistance. Contact jack@sifttext.com to arrange an authorized export and native conversion. Keep this work in private storage and keep account credentials out of public code.

Agree on a source cutoff and retain the original export. Ask the converter to map each source UUID to its native file and account for every changed field. Check each record's complete body and status against the source. Verify each parent's child order and each reference's target and reason. Return discrepancies to the user or maintainer for resolution before accepting the conversion.

Read the installed Local Core Agent Entrypoint and its required files. Run `sifttext check "$bundle"` on the converted native bundle. Preserve the export and report each refusal. Resolve oversized bodies or incompatible records through reviewed changes that retain their original meaning.

Choose a new knowledge-home path and run `sifttext init "$new_home" --bundle "$bundle"`. This creates an independent Git repository. Check the init receipt for an instruction pointer. If it has none, use the installed first-party knowledge home for agent guidance while working on the migrated store. For a destination that already contains knowledge, preserve it and ask the maintainer for a reviewed merge plan that accounts for UUID collisions and local edits.

Run `sifttext index "$new_home"`. Read representative nodes by UUID and search for phrases from their bodies. Compare the indexed records and relationships with the conversion manifest. Rehearse Git recovery on a private copy and verify that restored content remains retrievable.

After those checks pass, confirm the cutover with the user and give their agent the migrated home's path and the applicable local instructions. Keep the former system read-only for recovery. Identify the migrated snapshot by its cutoff and export hash. Give the new home's path and accepted commit, and describe any unresolved discrepancies. Preserve later local work if recovery becomes necessary.
