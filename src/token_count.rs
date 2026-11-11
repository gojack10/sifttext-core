//! Exact ordinary o200k_base counts without CoreBPE's decoder or token-ID output.
use fancy_regex::Regex;
use rustc_hash::FxHashMap;
use std::sync::{Mutex, OnceLock};

static COUNTER: Mutex<Option<Counter>> = Mutex::new(None);
const VOCABULARY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/o200k-ordinary.bin"));
use tiktoken_rs::{Rank, byte_pair_split};

pub(crate) fn vocabulary_identity() -> &'static str {
    static IDENTITY: OnceLock<String> = OnceLock::new();
    IDENTITY.get_or_init(|| crate::sha256(VOCABULARY))
}

struct Counter {
    encoder: FxHashMap<Vec<u8>, Rank>,
    regex: Regex,
}

// The first two alternatives match a word with uppercase letters before
// lowercase letters, optionally preceded by one non-letter, non-number byte.
// Defer to the regex when a contraction or Unicode letter or mark could extend it.
fn ascii_word_end(text: &[u8], start: usize) -> Option<usize> {
    let first = text[start];
    let prefix = first.is_ascii() && !first.is_ascii_alphanumeric()
        && !matches!(first, b'\r' | b'\n');
    let word = start + usize::from(prefix);
    let mut end = word;
    while text.get(end).is_some_and(u8::is_ascii_uppercase) {
        end += 1;
    }
    while text.get(end).is_some_and(u8::is_ascii_lowercase) {
        end += 1;
    }
    if end == word {
        return None;
    }
    text.get(end)
        .is_none_or(|byte| byte.is_ascii() && !byte.is_ascii_alphabetic() && *byte != b'\'')
        .then_some(end)
}

impl Counter {
    fn new() -> Self {
        // Generated and checked by build.rs from tiktoken-rs =0.6.0's public
        // decoder: u32 LE count, then u32 LE length + bytes, in ordinary rank order.
        let mut packed = VOCABULARY;
        fn word(packed: &mut &[u8]) -> u32 {
            let (bytes, rest) = packed.split_at(4);
            *packed = rest;
            u32::from_le_bytes(bytes.try_into().unwrap())
        }
        let count = word(&mut packed);
        let mut encoder = FxHashMap::with_capacity_and_hasher(count as usize, Default::default());
        for rank in 0..count {
            let len = word(&mut packed) as usize;
            let (token, rest) = packed.split_at(len);
            encoder.insert(token.to_vec(), rank);
            packed = rest;
        }
        // The exact seven alternatives in tiktoken-rs =0.6.0 openai_public.rs.
        let regex = Regex::new(&[
            "[^\\r\\n\\p{L}\\p{N}]?[\\p{Lu}\\p{Lt}\\p{Lm}\\p{Lo}\\p{M}]*[\\p{Ll}\\p{Lm}\\p{Lo}\\p{M}]+(?i:'s|'t|'re|'ve|'m|'ll|'d)?",
            "[^\\r\\n\\p{L}\\p{N}]?[\\p{Lu}\\p{Lt}\\p{Lm}\\p{Lo}\\p{M}]+[\\p{Ll}\\p{Lm}\\p{Lo}\\p{M}]*(?i:'s|'t|'re|'ve|'m|'ll|'d)?",
            "\\p{N}{1,3}",
            " ?[^\\s\\p{L}\\p{N}]+[\\r\\n/]*",
            "\\s*[\\r\\n]+",
            "\\s+(?!\\S)",
            "\\s+",
        ].join("|")).expect("pinned o200k_base ordinary regex");
        Self { encoder, regex }
    }

    fn fits_word_bound(&self, text: &str, ceiling: usize) -> bool {
        let bytes = text.as_bytes();
        let mut upper_bound = bytes.len();
        let mut start = 0;
        for word in bytes.split(|byte| *byte == b' ') {
            let end = start + word.len();
            // The pinned splitter includes the last leading space in a
            // lowercase ASCII word piece. A following space ends that piece.
            // Each such vocabulary hit replaces word.len() + 1 bytes with one token.
            if start > 0 && end < bytes.len() && word.len() > 1
                && word.iter().all(u8::is_ascii_lowercase)
                && self.encoder.contains_key(&bytes[start - 1..end])
            {
                upper_bound -= word.len();
                if upper_bound <= ceiling {
                    return true;
                }
            }
            start = end + 1;
        }
        false
    }

    fn count(&self, text: &str) -> usize {
        let bytes = text.as_bytes();
        let mut offset = 0;
        let mut total = 0usize;
        while offset < bytes.len() {
            let (start, end) = if let Some(end) = ascii_word_end(bytes, offset) {
                (offset, end)
            } else if let Some(matched) = self.regex.find_from_pos(text, offset).unwrap() {
                (matched.start(), matched.end())
            } else {
                break;
            };
            let piece = &bytes[start..end];
            let count = if piece.len() == 1 || self.encoder.contains_key(piece) {
                1
            } else {
                byte_pair_split(piece, &self.encoder).len()
            };
            total = total.checked_add(count).expect("ordinary token count overflow");
            offset = end;
        }
        total
    }
}

/// Release the phase-local dictionary before unrelated Git or SQLite work.
pub(crate) fn release() {
    COUNTER.lock().expect("token counter lock").take();
}

/// Count ordinary tokens exactly as tiktoken-rs =0.6.0 `encode_ordinary`.
/// Special-token spellings remain ordinary text; regex errors panic upstream too.
pub fn ordinary(text: &str) -> usize {
    let mut holder = COUNTER.lock().expect("token counter lock");
    let counter = holder.get_or_insert_with(Counter::new);
    counter.count(text)
}

/// Return the exact count when the word bound cannot prove that the body fits.
pub(crate) fn count_if_needed(text: &str, ceiling: usize) -> Option<usize> {
    let mut holder = COUNTER.lock().expect("token counter lock");
    let counter = holder.get_or_insert_with(Counter::new);
    (!counter.fits_word_bound(text, ceiling)).then(|| counter.count(text))
}
