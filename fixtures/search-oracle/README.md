# Independent search expectations

`leviathan.json` records 252 upstream Leviathan CLI observations on eight synthetic records: 12 queries × three offsets × seven scope/permission/ranking configurations. No Core executable produced these answers. The fixture records the independently verified upstream binary and source-archive SHA-256 identities. Exact SQLite score ties are observed, not promised as an upstream total-order guarantee.

The retained `search_differential_native` and `search_working_view_native` Rust targets are historical artifact consumers, not current development execution instructions. Their source records comparison, upstream-revalidation and evidence-retention mechanics; this README does not claim those checks were rerun for the current implementation.

For current qualification, first build pinned assured assets with `sh scripts/build.sh`, retaining required formal checks. Exercise actual `sifttext search STORE QUERY [UNDER_UUID]` calls on disposable native data, compare returned records with inspected source and any applicable oracle expectations, and observe live-edit/restore and refusal behavior directly. Retain requests, outputs, elapsed time and native CPU/RSS evidence; unavailable measurements are unknown, not zero. Such ad hoc observations do not revalidate all 252 historical answers or qualify full-store or aggregate performance.
