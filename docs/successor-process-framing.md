# Checked Rust process framing

The adapter derives reply byte limits from the request's graph cardinality and
integer envelope. It checks exact decoded cardinality, counts and integers before
applying operations. Decimal serialization and pipe buffering reserve fallibly.
Reader refusal terminates and reaps the direct producer; it never accepts partial
output or stderr. Output-budget refusal takes precedence over consequent SIGPIPE.

The frozen backend reads at most 1,048,576 input bytes. Every invocation rejects a
longer request before spawning. Transport beyond the old reply limit is exercised
with a zero-padded helper reply, not claimed as larger compiled graph acceptance.
The landed graph-derived deadline (10-second floor, 300-second cap) and 1ms polling
are retained. No frozen law, authored-file cap, tokenizer or status rule changes.
Direct-child reap is not descendant isolation; inherited pipes can outlive a
malicious producer, but the receive deadline still refuses the operation.

The process-framing and deadline checks retained in source are historical
artifacts, not current development execution instructions. After the pinned
assured build (`sh scripts/build.sh`, including its required formal checks),
exercise the actual CLI on disposable native drafts and inspect refusals,
producer lifetimes and resource use with native profiling/debugger tools.
Do not infer the synthetic helper cases above from ordinary CLI success.

These observations do not establish full native capacity, platform qualification
or main acceptance. Keep the maintained submit gate and affected native consumers.
