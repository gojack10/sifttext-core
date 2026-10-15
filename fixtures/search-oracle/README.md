# Independent search expectations

`leviathan.json` records 252 upstream Leviathan CLI observations on eight synthetic records: 12 queries × three offsets × seven scope/permission/ranking configurations. No Core executable produced these answers. The fixture records the independently verified upstream binary and source-archive SHA-256 identities. Exact SQLite score ties are observed, not promised as an upstream total-order guarantee.

The ordinary Rust test uses these immutable answers, validates real native results, retains per-call latency/RSS, and measures returned JSON with pinned o200k_base. It invokes no Node, Python, TypeScript, or private absolute path. Build checked native assets first:

```sh
sh scripts/build.sh
sh scripts/with-tools.sh cargo test --locked --test search_differential_native
```

To independently revalidate every recorded expectation against an acquired upstream executable, provide its absolute path; the same Rust test reconstructs the seven upstream indexes and compares all 252 answers before invoking Core:

```sh
SIFTTEXT_TEST_SEARCH_ORACLE=/absolute/path/to/leviathan \
  sh scripts/with-tools.sh cargo test --locked --test search_differential_native
```

`SIFTTEXT_TEST_EVIDENCE` retains raw requests, outputs, elapsed times and memory-tool stderr instead of temporary outputs. Memory observations require `/usr/bin/time` (`-l` on macOS, GNU `-v` on Linux); absent resource evidence fails rather than being reported as zero. The separate `search_working_view_native` target owns live edit, Git restore/revert, sparse, race, interruption and cache-recovery controls. Synthetic measurements do not qualify full-store or aggregate performance.
