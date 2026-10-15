# Third-party software in `src/vendor/leviathan`

This directory holds the only upstream Leviathan code in this repository slice. It exists so the
successor Core can reuse pinned query, ranking, snippets, cards and lexical retrieval.

## Vendored upstream code

| Item | Value |
|---|---|
| Project | [Leviathan](https://github.com/elstongun/leviathan) |
| Upstream author | Joshua Baker <joshuapbak@gmail.com> |
| Pinned revision | `702ea92dc4695de00c488a66b43d75419bbb78e8` |
| Pinned upstream file | `src/text.rs` |
| Pinned upstream file SHA-256 | `1999633b4ad97903849d63c6654910b645470bd444c7e6a6126df50f499dc570` |
| License | Apache-2.0 (see `LICENSE`) |
| Copyright notice | `NOTICE` (unmodified upstream copy) |

`query_text.rs` is an **extracted and modified** upstream file, not a whole-file copy.

- Copied verbatim from the pinned `src/text.rs`: the `TOKEN` static, the `STOPWORDS` list,
  `words_in`, `query_words`, the `Query` struct with its `Debug, Clone, Default, PartialEq`
  derives, and `Query::parse`, `Query::is_empty`, `Query::fts`, `Query::not_fts` and
  `Query::highlight` — the upstream query region from `TOKEN`/`STOPWORDS` through
  `Query::highlight`, including their doc comments.
- Removed relative to the upstream file: `normalize_name`, `fnv1a`, `group_token`, `facet_token`,
  `truncate`, `ISO`, `US`, `normalize_date`, `normalize_bound`, `format_unix` and the upstream
  `#[cfg(test)]` module. Those are date, group, facet, card and rendering concerns that this pure
  query prerequisite does not consume, so they are not ported and are not claimed here.
- Added relative to the upstream file: the header comment above naming the pinned revision, the
  pinned file hash and the license, and the statement that the query region is verbatim.

No query behaviour was changed. Token grammar, stopword list, lowercasing, trailing-punctuation
trim, word deduplication, source phrase/exclusion handling, the 64-positive and 32-exclusion
expression caps and the source highlight order are exactly the pinned upstream ones. Any later
deviation from upstream must be listed here as a new adaptation, not silently introduced.

## Crates that carry this code

`regex` `1.13.1` (MIT OR Apache-2.0) is the crate the vendored `TOKEN` grammar needs. It was
already resolved in this repository's `Cargo.lock` as a transitive dependency of `tiktoken-rs`
`0.6.0`, with checksum `f020237b6c8eed93db2e2cb53c00c60a8e1bc73da7d073199a1180401450218d` — the
same checksum upstream pins for the same version. This change registers it as a direct dependency
at `=1.13.1` and adds no new package, no version change and no upgrade of any existing dependency.
Its own transitive crates (`aho-corasick`, `memchr`, `regex-automata`, `regex-syntax`) were already
locked for the same reason.

The integrated search uses the existing locked `rusqlite =0.40.2` (MIT) with
bundled `libsqlite3-sys`/SQLite (public domain), plus existing `serde`, `serde_json`,
`sha2`, `tempfile`, `tiktoken-rs` and `libc`. This search addition changes no
package versions or Cargo lock entries.

## Scope and limits

The already accepted `snippet.rs`, `relevance.rs` and `card.rs` extract modified
regions of pinned `src/card.rs`, `src/query.rs`, `src/fields.rs` and `src/text.rs`;
their headers identify source hashes and complete changes. Integrated
`search.rs` now adapts `src/index.rs:63-73` and `src/query.rs:438-566`, plus the
single-part `text::fnv1a/group_token` expression, from the same pinned revision.
It retains porter/unicode61 FTS5, upstream BM25 weights/candidate cutoff/count,
boost multiplication, query preparation, relevance paging and card snippets.

Changed-file notice: `search.rs` is not an unmodified whole upstream file.
UUID-scoped/readable checked transactional documents replace flat source
records; hashed inert synthetic names replace natural-language groups without
removing authored text. Postings now use content-backed FTS5 with native integrity
verification and checked-source row comparison. Stable UUID-to-rowid mappings
update only changed text, without sorted-rowid suffix rewrites on insert/delete.
Empty-query browse explicitly retains reverse logical document order; exact-score
tie order remains upstream unspecified. Permissions are an invocation-only rowid predicate, with constant inert
group columns; title weighting is an invocation-only bm25 argument. No persisted
ranking/scope setting can leak to another search. The same-version SQLite
transaction and ordinary final source/cache validation replace file stamps.
No fuzzy resolution, outside fallback, dates/facets, upstream CLI/MCP service or
build-file settings are included.
`LICENSE` remains the verbatim upstream copy; `NOTICE` retains upstream attribution
and records the modified retrieval region. Verified prepared projections use this
same SQLite-maintenance path; opaque postings/query settings are never imported.

This notice records attribution for the vendored code. It is not a license audit or clearance for
the packaged dependency set of any release, and it is not permission to publish SiftText.
