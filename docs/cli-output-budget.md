# CLI output budget: non-SQL commands

## What went wrong

`check` answers whether a knowledge home is valid, but a successful answer includes a
`crystallization_tokens` row for every node. Each row repeats node and rule identifiers.
The 54,592-token figure in the reported incident measures that report's text, not model
inference or the amount of knowledge read. The report is built before the output limiter
checks it. `-y` permits printing it; it does not make validation succeed.

The old warning mentioned SQL because one shared formatter served all large-output
commands. The warning now names the actual command, without SQL advice. The subsequent
`worktree` cut removes its branch field and identifies the possible draft and baseline
on allocation failure. Validation rules, output limits and exit codes are unchanged.

## Evidence and how to reproduce it

Run this safe serializer-only probe:

```sh
sh scripts/with-tools.sh cargo run --locked --example cli-output-budget
```

It prints bytes and exact `o200k_base` ordinary token counts using the existing Rust
tokenizer and production renderers. It never opens a knowledge home, runs Git, contacts
a server or performs an update. Identifiers are deterministic synthetic UUIDs; names
and summaries are short English samples. Counts include final newlines where the CLI
adds them. The submit envelope and other small receipts are illustrative source-shaped
samples, not end-to-end operation results. The check, nested validation projection and
outline rendering use their production types/renderers.

The 904-node size is chosen from the reported incident, not an assertion about the
current local home. Actual identifiers and text change the exact count. These measured
samples are **not global worst-case bounds**.

| Output shape | 1 node | 100 nodes | 904 nodes |
|---|---:|---:|---:|
| Successful `check` | 188 | 6,121 | 54,644 |
| `check`: one status error per node, plus token-count rows | 132 | 6,949 | 62,992 |
| Structural rejection: one two-endpoint diagnostic per edge | 135 | 7,443 | 67,786 |
| Successful `accepted`, including nested validation | 292 | 9,820 | 87,509 |
| Successful `submit` receipt shape, including nested validation | 403 | 9,931 | 87,620 |
| `normalize`, no nodes changed | 65 | 3,653 | 33,102 |
| `normalize`, every ID in both changed/reopened lists | 119 | 8,705 | 79,364 |
| Wide `outline`, shallow (root + every direct child) | 105 | 4,653 | 41,920 |
| Wide `outline`, full | 99 | 4,448 | 40,107 |

The normalization measurements above are stdout only. Each reopened node also produces
`[UUID] set to in_progress.` on stderr. Budget roughly another 25–40 tokens per reopening;
that makes the 904-node all-reopened case approximately **102k–116k tokens combined**.
The all-ID normalization sample measures the envelope, not a claim that every such
transition can be produced by one particular valid before/after pair.

Additional measured stress shapes:

- `search`: the 20 deepest hits in a 904-node chain produce **17,890 rows**, **2,157,217
  bytes**, **757,690 tokens**, despite the 20-hit limit. Every hit repeats its ancestor
  chain. This is a source-shaped renderer measurement, not an end-to-end performance
  or successful deep-store qualification test.
- `read`: a small body with a 16,384-token warning in its header returns **16,442 tokens**.
  The 5,000-token ceiling applies to the body, not the complete file or header.
- `update`: listing one refreshed node path is **50 tokens**; 904 paths is **24,717**.
  Conflict messages and other embedded receipts can be much larger.

## Worst-case thinking: no invented universal maximum

There is no useful fixed token ceiling for the **full** outputs of most commands.
They depend on the number of nodes, links, ancestry, authored text, supplied descriptors,
paths, remote responses and external program errors. Graph-wire limits, memory and
execution deadlines can reject an operation before a huge report is produced; they are
not a documented customer output-token budget or an unlimited-scale promise.

Separate three quantities:

1. **Full output:** what would be emitted with the output override, or on an unguarded path.
2. **Default visible output:** guarded returns above 15,000 tokens are replaced by a short
   notice and exit 1. Exactly 15,000 is allowed. Suppression does not establish validity.
3. **Combined stdout + stderr:** reopening notices, ordinary errors and automatic update
   notices are not all covered by that limiter.

### A deliberately pessimistic counting heuristic

For rough discussion, use **100 tokens per compact UUID-heavy row**, **150 per pretty-JSON
validation row**, plus the text fields. These are rounded planning allowances, not
mathematical upper bounds. Long warnings or names invalidate a flat per-row allowance.
Never use the English-prose `characters / 4` rule as a worst-case estimate for UUID tables,
code, arbitrary Unicode or escaped strings.

For a conservative byte-based bound with the ordinary tokenizer:

> **tokens <= UTF-8 bytes in the final rendered output.**

Count *after* JSON/LEAN escaping, indentation and repeated ancestors. A character can
occupy multiple UTF-8 bytes; JSON escaping can expand input. Byte count is deliberately
loose but, unlike a prose ratio, safe for a concrete rendered string. It does not bound
an unknown-size output before generation.

For scaling estimates, define:

- `N`: nodes in the whole home; `V`: nodes actually displayed by an outline.
- `F`: diagnostic rows, including successful token rows retained in a rejection.
- `C`, `R`: normalized and reopened IDs.
- `A = sum(depth(hit) + 1)`: search rows across at most 20 hits.
- `M`: tokens of names, summaries, warnings and active reasons displayed, **counting repetitions**.
- `P`: listed paths; `Q`: tokens in returned descriptor fields; `X`: external error text.

| Family | Practical token estimate, not a hard bound |
|---|---|
| Successful `check` | `200 + 60–75 * N`; pessimistic planning: `200 + 100 * N` |
| Rejected `check` | `300 + 40–100 * F`; include token rows as well as errors |
| `submit` / `accepted` | `500 + 95–120 * N` on success; budget `500 + 150 * F` for diagnostics |
| `normalize` stdout | `100 + 40 * N + 30 * C + 30 * R` |
| `normalize` stderr | additional `40 * R` allowance |
| `outline` | `200 + 50 * V + M`; shallow can still have `V = N` |
| `search` | `200 + 50 * A + M`, plus up to 20 paths and matching excerpts |
| `read` | exact tokens of the complete authored file, not just the body |
| `update` | base receipt + `30–60 * P`, plus embedded validation/error text |
| Session receipts | base receipt + `Q`; not the transcript's token count |
| Any raw subprocess failure | add `X`; there is no CLI-wide cap on `X` |

For structural graph failure, diagnostic rows can scale with **bad links as well as nodes**:
dangling references, multiple placements and unrooted nodes are separate findings.
For a structurally valid graph with bad metadata/statuses, several checks can fail per
node. One-error-per-node measurements are not the maximum possible rejection.

If every displayed node has `W` tokens of warning text, add `V * W` to an outline, but
`A * W` to search. At `W=1,000`, the 17,890-row search stress shape adds about **17.9 million
tokens** just in repeated warnings, if the input is accepted and computation completes.

For an intentionally extreme engineering envelope, put `L = 1,048,576` bytes (the
complete authored-file cap). Selected metadata can consume almost all of that file;
the body ceiling does not constrain it. The current LEAN cell encoder at most doubles
selected text bytes when escaping quotes/backslashes/newlines. Allowing an additional
1,024 bytes per row for fixed fields/counters, conservative **successful-return** bounds
are:

- `read`: at most `L` tokens.
- `outline`: at most `4,096 + V * (2*L + 1,024)` tokens.
- `search`: at most `4,096 + A * (2*L + 1,024)`, plus twice the UTF-8 length of the hit
  paths and 4,096 per hit for excerpts/extra cells, in tokens.

These use `tokens <= final bytes`, so they are much looser than normal estimates. At
904 displayed nodes, the outline envelope is approximately **1.90 billion tokens**;
with 17,890 repeated search rows, the search envelope is approximately **37.54 billion**
before the small path/excerpt allowance. These are arithmetic upper allowances, **not
measured maxima, claims of achievable tokenizer density, or claims the operation can
finish**. A store, cache generation, graph resource check or filesystem can fail first.
They exclude errors and automatic update notices. They show why the current text/file
constraints are not a practical output budget. For other arbitrary input/error channels,
there is no comparable CLI-wide full-output ceiling to honestly quote.

## Complete command inventory

"Guarded" below refers to the explicit main return, not a claim that all errors or
incidental stderr are bounded. Small receipt measurements assume the short sample paths
in the probe, not arbitrary-length caller strings.

| Command | Sample / ordinary shape | Most pessimistic source of growth; limiter coverage |
|---|---|---|
| `-h`, `--help`, no command | Fixed help text | Fixed for a given build; no knowledge-size growth, no limiter. |
| `login` | Sign-in URL + account fields + status | Remote client ID is percent-encoded into the URL; returned account values are pretty-printed. HTTP response limit is 1 MiB per response, **not** a 1 MiB printed-output guarantee. Nested values/escaping can expand output. No token guard. No live login tested. |
| `account` | 47 tokens in sample | Six known fields are selected, but their values other than `paid` are not constrained to short scalar strings. Response size/depth and pretty printing, not node count, govern the pessimistic case. No token guard. |
| `logout` | Short fixed sentence | Fixed local/remote-revocation result plus any ordinary error; no knowledge/transcript dump. No token guard. |
| `source-code` | 55 tokens | A few source paths/revision on success; failed Git clone/checkout includes captured stderr `X`. No token guard. |
| `runtime-check` | 10 tokens | Status + runtime path; runtime/source-validation errors. No token guard. |
| `update` and all its modes | Small config/status receipt, or path/conflict report | Lists of refreshed paths, conflict paths, embedded validation, build/Git stderr. Main success receipt is guarded; error output and opportunistic notices are not. `--status`, `--config`, `--grant`, `--mode`, `--pin`, `--unpin`, `--rebuild`, `--resume`, `--rollback` have different shapes; the tiny config result does not bound transactions. |
| `init` (`--empty`, default, `--bundle`) | 33 tokens | Summary stays small; validation of a bad copied candidate can return an entire rejection report. Other Git/copy errors add `X`. No token guard. Existing recognized homes are not rewritten by this audit. |
| `instructions` | 44 tokens | One entry path/version on success; whole-home rejection from `local::check` on failure. Neither path is guarded. |
| `read` | Exact `node.md` | Successful authored file is capped at **1,048,576 bytes**, hence <= that many tokens; body limit alone is insufficient. Success is guarded. Whole-home validation rejection can bypass the guard on stderr. |
| `check` | Per-node table | `N` token rows or `F` diagnostic rows; bad links can dominate. Both direct success and rejection reports are guarded. |
| `submit` | 87,620 tokens at synthetic N=904 | Nested validation repeats rows in verbose JSON; rejected receipts can contain many findings. Main receipt is guarded **after publication/completion**; raw errors are not. A suppression notice does not mean nothing was saved. Existing retry test checks recovery without publishing twice. |
| `accepted` | 87,509 tokens at synthetic N=904 | Same nested validation growth; main receipt guarded, ordinary errors not. |
| `advance` | 64 tokens | Reader/home/revisions summary, not every changed file; Git/refresh failures add `X`. No token guard. |
| `worktree` | Two fields: path and baseline | Branch stays internal. Allocation failure identifies the possible draft and baseline; files are preserved. Initial whole-home validation rejection or Git stderr can still be large. No token guard. |
| `create` | 86 tokens | UUID/path/parent summary, not the supplied node body. Path/error size rather than home size. No token guard. |
| `normalize` | 33,102 tokens even with no changes at N=904 | Every node's status is returned, then changed and reopened IDs again. Stdout receipt guarded before changes; one stderr notice per reopening is not included in that gate. |
| `index` | 31 tokens | Only counts/version on success; not all indexed rows. Retrieval refusals are compact classified errors with optional path/field context. No token guard. |
| `outline` (shallow/full) | About 40k–42k at wide N=904 | All displayed metadata, warnings and active reasons; shallow has no child-count cap. Success guarded; errors not generally guarded. |
| `search` | 757,690 in the chain stress shape | Up to 20 hits, but every ancestor chain repeats. Untruncated name/summary/warnings/reason metadata dominate; excerpts are separately bounded around 300 characters. Success guarded; errors not generally guarded. |
| `session-capture` | 35 tokens | Archive key/commit summary, **not the transcript**. Descriptor identifiers/path sizes and Git errors control growth. Unavailable source has a tiny fixed result. No token guard. |
| `session-attach` | 57 tokens | Returns `archive_key` from the descriptor. Unavailable-source path permits oversized/non-string values there; descriptor input is read without the authored-file 1 MiB cap. Therefore no fixed full-output bound. Receipt guarded before attachment. |
| `session-backup` | 35 tokens | Commit/status/configured flag; no transcript or per-file list. Git/network errors add `X`. No token guard. |
| `session-restore` | 17 tokens | Restored-link count, not a link per row; Git/network errors add `X`. No token guard. |
| `session-retention-config` | 33 tokens | Four settings including archive path; not a session list. Config/file constraints, path escaping and ordinary errors control growth. No token guard. |
| `install-home` (internal launcher operation) | Home/created/application receipt | Initialization/update error paths and Git stderr; no dedicated token guard. Included although absent from public help. |

After many successful non-account/non-update commands, `updates::opportunistic()` may
also print an update notice or error to stderr. `SIFTTEXT_NO_UPDATE=1` disables it for
isolated measurements. It must be included when budgeting a normal managed invocation.
Commands which internally call `local::check` can inherit its large rejection text even
when their successful return is tiny. A global "15,000 tokens maximum" claim is false.

## Decisions available to the owner — nothing cut yet

1. **Check:** summary/counts by default; retain per-node token rows only on explicit request?
2. **Publication:** replace successful nested validation rows with outcome/version/counts?
3. **Normalize:** emit changed nodes only; avoid returning every unchanged status and repeating IDs?
4. **Search:** show shared ancestors once, and/or introduce a depth/metadata budget?
5. **Outline/read:** investigate bounds or selective display of long headers/warnings without
   silently removing safety information?
6. **All commands:** should large-error handling retain a useful status/summary and safe retrieval
   path instead of hiding everything? Keep mutation/retry semantics visible.
7. **Lower priority:** constrain descriptor/account response fields and budget automatic update
   notices/raw external errors. These are separate trust-boundary and output-policy decisions,
   not justification for silently truncating data or weakening validation.

Implemented so far: command-specific suppression wording and the `worktree` two-field
receipt with recoverable allocation errors. Other command cuts remain proposals; no
validation or trust boundary has been weakened. Source changes are not an installed
upgrade and have not been pushed or published.
