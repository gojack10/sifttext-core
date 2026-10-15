// Vendored upstream lexical query preparation, extracted from Leviathan.
//
// Pinned source: revision 702ea92dc4695de00c488a66b43d75419bbb78e8, file `src/text.rs`,
// SHA-256 1999633b4ad97903849d63c6654910b645470bd444c7e6a6126df50f499dc570.
// License Apache-2.0; the unmodified upstream `LICENSE` and `NOTICE` sit beside this file and
// `THIRD_PARTY.md` identifies the copied region, the omitted region and the reason.
//
// The query region below is copied verbatim: the `TOKEN` grammar, the `STOPWORDS` list,
// `words_in`, `query_words` and `Query` with `parse`, `is_empty`, `fts`, `not_fts` and
// `highlight`. No token grammar, stopword, cap, ordering or deduplication rule is changed here;
// this is reuse of the required upstream query code, not a second parser.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

static TOKEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\p{L}\p{N}][\p{L}\p{N}_.-]*").unwrap());

/// Words that match nearly every record and only slow BM25 down.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "been", "but", "by", "did", "do", "does", "for", "from",
    "had", "has", "have", "how", "i", "if", "in", "into", "is", "it", "its", "last", "me", "my", "of", "on",
    "or", "our", "so", "that", "the", "their", "then", "there", "this", "time", "to", "was", "we", "were",
    "what", "when", "where", "which", "who", "why", "will", "with", "you",
];

fn words_in(text: &str) -> impl Iterator<Item = String> + '_ {
    TOKEN.find_iter(text).map(|m| m.as_str().trim_end_matches(['.', '-', '_']).to_lowercase())
}

/// Lowercased searchable words, stopwords removed.
pub fn query_words(text: &str) -> impl Iterator<Item = String> + '_ {
    words_in(text).filter(|t| !t.is_empty() && !STOPWORDS.contains(&t.as_str()))
}

/// A free-text query: words (ranked, any may match), `"quoted phrases"`, and
/// `-excluded` words or phrases. Everything is re-quoted for FTS5, so query
/// syntax in user input is inert.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Query {
    pub words: Vec<String>,
    pub phrases: Vec<String>,
    pub excluded: Vec<String>,
}

impl Query {
    pub fn parse(text: &str) -> Self {
        let mut q = Query::default();
        let mut seen = HashSet::new();
        let mut rest = text;
        while let Some(start) = rest.find(|c: char| !c.is_whitespace()) {
            rest = &rest[start..];
            let negate = rest.starts_with('-') && rest.len() > 1;
            let body = if negate { &rest[1..] } else { rest };
            let (chunk, phrase, next) = if let Some(inner) = body.strip_prefix('"') {
                let end = inner.find('"').unwrap_or(inner.len());
                (&inner[..end], true, &inner[(end + 1).min(inner.len())..])
            } else {
                let end = body.find(char::is_whitespace).unwrap_or(body.len());
                (&body[..end], false, &body[end..])
            };
            rest = next;
            let tokens: Vec<String> =
                if phrase { words_in(chunk).collect() } else { query_words(chunk).collect() };
            if tokens.is_empty() {
                continue;
            }
            if negate {
                q.excluded.push(tokens.join(" "));
            } else if phrase && tokens.len() > 1 {
                q.phrases.push(tokens.join(" "));
            } else {
                q.words.extend(tokens.into_iter().filter(|t| seen.insert(t.clone())));
            }
        }
        q
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.phrases.is_empty()
    }

    /// Positive FTS5 expression: any word or phrase.
    pub fn fts(&self) -> Option<String> {
        let terms: Vec<String> =
            self.words.iter().chain(&self.phrases).take(64).map(|t| format!("\"{t}\"")).collect();
        (!terms.is_empty()).then(|| terms.join(" OR "))
    }

    pub fn not_fts(&self) -> Option<String> {
        let terms: Vec<String> = self.excluded.iter().take(32).map(|t| format!("\"{t}\"")).collect();
        (!terms.is_empty()).then(|| terms.join(" OR "))
    }

    /// Words to highlight in snippets.
    pub fn highlight(&self) -> Vec<String> {
        let mut out: Vec<String> = self.words.clone();
        out.extend(self.phrases.iter().flat_map(|p| p.split(' ').map(str::to_string)));
        out
    }
}
