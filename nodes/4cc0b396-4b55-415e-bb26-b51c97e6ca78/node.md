---
id: "4cc0b396-4b55-415e-bb26-b51c97e6ca78"
name: "SQLite Queries"
micro_summary: "Explains read-only discovery and filtering of the local derived index, including freshness checks and its distinction from hosted PostgreSQL."
status: "in_progress"
see_also: []
scope: "Use the actual local SQLite schema safely, including parent/root filtering, and return verifiable source identities."
warnings: ""
---
## Use

Use SQLite queries for precise field filters, counts and relationships in the derived index.

Confirm the knowledge-home path before querying it.

The wrapper refreshes the current parseable files, including an uncommitted draft.

Submit only one SELECT statement, which rejects mutation and more than 1000 rows.

Use count(*) when the question needs a count, and select only needed fields with a small LIMIT.

## Schema

```text
nodes(id, name, micro_summary, scope, content, status, children, record)
links(source, target, reason, position)
metadata(key, value)
```

The children column holds an ordered JSON UUID array.

Parentage derives from that column, and a root has no parent.

The knowledge-base root UUID is 38a9fc79-8a84-494d-a962-2ec5134bc856.

## Examples

```sh
sifttext sql "$store" 'SELECT id,name,status FROM nodes ORDER BY name LIMIT 20'
sifttext sql "$store" 'SELECT p.id AS parent_id,c.value AS child_id FROM nodes p,json_each(p.children) c LIMIT 20'
sifttext sql "$store" "SELECT n.id,n.name FROM nodes p,json_each(p.children) c JOIN nodes n ON n.id=c.value WHERE p.id='5e565245-14fe-406c-977c-cf94c9be246b' ORDER BY CAST(c.key AS INTEGER)"
```

Use [[Retrieve SiftText Knowledge|526ed0f5-4e5d-42b3-9e3f-b7ad429f2df8]] for subtree navigation.

## Output

Query results are LEAN.

Read the relevant node body before treating a row as current guidance.

Return the query, the relevant rows and the returned version with the answer.
