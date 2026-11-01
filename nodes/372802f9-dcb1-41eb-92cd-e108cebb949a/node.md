---
id: "372802f9-dcb1-41eb-92cd-e108cebb949a"
name: "Migrate Hosted Knowledge"
status: "resolved"
micro_summary: "Guides an authorized, source-accounted move from hosted SiftText into a new local knowledge home."
see_also: []
position: 6144
---
## Export the source

Obtain permission to export those records.

Hosted export and conversion currently require maintainer assistance.

Contact jack@sifttext.com to arrange an authorized export and native conversion.

Keep the export in private storage and keep credentials out of public code.

Agree a source cutoff and retain the original export.

## Check the conversion

Ask the converter to map each source UUID to its native file.

Check each record's body and status against the source.

Verify each parent's child order and each reference's target and reason.

Return discrepancies for resolution before accepting the conversion.

Inspect the converted files and conversion manifest directly.

Preserve a broken source and its refusal diagnostics.

## Import

```sh
sifttext init "$new_home" --bundle "$bundle"
```

Initialization validates the supplied bundle and creates an independent Git repository.

For an existing destination, obtain a reviewed merge plan that accounts for UUID collisions and local edits.

Follow [[Worktree Publication|fb1e0c9f-8c04-4baf-91e6-4688856a14af]] to apply only the authorized converted files.

## Verify

```sh
sifttext index "$new_home"
```

Read representative nodes and search phrases from their bodies.

Compare the indexed records and relationships with the conversion manifest.

Rehearse Git recovery on a private copy.

## Cutover

Confirm the cutover with the user.

Give the current agent the migrated home's path and the applicable local instructions.

Keep the former system read-only for recovery.

Identify the migrated snapshot by its cutoff and export hash.

Describe any unresolved discrepancy.
