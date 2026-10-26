//! Exact ordinary o200k_base counts without CoreBPE's decoder or token-ID output.
use fancy_regex::Regex;
use rustc_hash::FxHashMap;
use std::sync::Mutex;

static COUNTER: Mutex<Option<Counter>> = Mutex::new(None);
use tiktoken_rs::{Rank, byte_pair_split};

struct Counter {
    encoder: FxHashMap<Vec<u8>, Rank>,
    regex: Regex,
}

impl Counter {
    fn new() -> Self {
        // Generated and checked by build.rs from tiktoken-rs =0.6.0's public
        // decoder: u32 LE count, then u32 LE length + bytes, in ordinary rank order.
        let mut packed = include_bytes!(concat!(env!("OUT_DIR"), "/o200k-ordinary.bin")).as_slice();
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
    counter.regex.find_iter(text).fold(0usize, |total, matched| {
        let piece = matched.unwrap().as_str().as_bytes();
        let count = if piece.len() == 1 || counter.encoder.contains_key(piece) {
            1
        } else if piece.is_empty() {
            0
        } else {
            byte_pair_split(piece, &counter.encoder).len()
        };
        total.checked_add(count).expect("ordinary token count overflow")
    })
}
