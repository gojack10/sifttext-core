// Vendored upstream snippet selection, extracted from Leviathan.
//
// Pinned source: revision 702ea92dc4695de00c488a66b43d75419bbb78e8, files `src/card.rs`
// (SHA-256 ae608eab99f6c0a82c8be976d2945951d6acd00a6e007df3bc9788431cd9beda) and `src/text.rs`
// (SHA-256 1999633b4ad97903849d63c6654910b645470bd444c7e6a6126df50f499dc570).
// License Apache-2.0; the unmodified upstream `LICENSE` and `NOTICE` sit beside this file and
// `THIRD_PARTY.md` records the vendored query region. This file is an extracted and modified
// upstream region, not a whole-file copy: `one_line`, `stem`, `words`, `sentences` and the body of
// `snippet` are copied verbatim from `card.rs`, and `truncate` is copied verbatim from `text.rs`.
// The modifications are the documented adaptation in `select` — the searched strings arrive as an
// ordered slice instead of being extracted from a record through `Mapping` — and the two
// `Card::build` lines this module reproduces: the at-least-40 cap and the empty-highlight rule.
//
// Nothing here parses a query, ranks a candidate, reads a file, maps a record or builds a card.

use std::collections::HashSet;

/// Truncate on a char boundary, marking the cut with an ellipsis.
pub fn truncate(value: &str, max_chars: usize) -> String {
    let value = value.trim();
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let cut: String = value.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{}\u{2026}", cut.trim_end())
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn stem(word: &str) -> &str {
    let n = word.chars().count();
    let keep = if n <= 4 { n } else { (n - 2).max(4) };
    let end = word.char_indices().nth(keep).map(|(i, _)| i).unwrap_or(word.len());
    &word[..end]
}

/// Alphanumeric runs with their starting char offset.
fn words(s: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start: Option<(usize, usize)> = None;
    for (ci, (bi, c)) in s.char_indices().enumerate() {
        if c.is_alphanumeric() {
            start.get_or_insert((ci, bi));
        } else if let Some((sc, sb)) = start.take() {
            out.push((sc, &s[sb..bi]));
        }
    }
    if let Some((sc, sb)) = start {
        out.push((sc, &s[sb..]));
    }
    out
}

/// Sentence-sized pieces, split after `.!?;` only when whitespace follows, so
/// versions, decimals and hostnames stay whole.
fn sentences(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if matches!(c, '.' | '!' | '?' | ';') && chars.peek().is_some_and(|(_, n)| n.is_whitespace()) {
            out.push(s[start..=i].trim());
            start = i + 1;
        }
    }
    out.push(s[start..].trim());
    out.retain(|p| !p.is_empty());
    out
}

/// Best sentence-sized segment of the searched text by distinct query words
/// matched, trimmed to `cap` around the first match.
fn snippet(highlight: &[String], texts: &[String], shown: &[String], cap: usize) -> Option<String> {
    let stems: Vec<&str> = highlight.iter().map(|w| stem(w)).collect();
    let mut best: Option<(usize, String, usize)> = None;
    for text in texts {
        let flat = one_line(text);
        if shown.iter().any(|s| s.contains(&flat)) {
            continue;
        }
        for seg in sentences(&flat) {
            if seg.is_empty() || shown.iter().any(|s| s.contains(seg)) {
                continue;
            }
            let lower = seg.to_lowercase();
            let mut hits = HashSet::new();
            let mut first = None;
            for (pos, word) in words(&lower) {
                if let Some(k) = stems.iter().position(|s| word.starts_with(s)) {
                    hits.insert(k);
                    first.get_or_insert(pos);
                }
            }
            if !hits.is_empty() && best.as_ref().is_none_or(|(n, _, _)| hits.len() > *n) {
                best = Some((hits.len(), seg.to_string(), first.unwrap_or(0)));
            }
        }
    }
    let (_, seg, first) = best?;
    if seg.chars().count() <= cap {
        return Some(seg);
    }
    let lead = first.saturating_sub(40);
    let tail: String = seg.chars().skip(lead).collect();
    let cut = truncate(&tail, cap);
    Some(if lead > 0 { format!("\u{2026}{cut}") } else { cut })
}

/// The snippet selection `Card::build` performs, over already-collected searched strings instead
/// of a record and its `Mapping`.
///
/// `highlight` is the real upstream `Query::highlight` output, `texts` are the searched strings in
/// the caller's display order, `shown` is what the result already displays and `max_chars` is the
/// requested card cap. Like `Card::build`: an empty highlight list yields no snippet, and the cap
/// is clamped to at least 40 characters.
pub fn select(
    texts: &[String],
    highlight: &[String],
    shown: &[String],
    max_chars: usize,
) -> Option<String> {
    let cap = max_chars.max(40);
    if highlight.is_empty() {
        return None;
    }
    snippet(highlight, texts, shown, cap)
}
