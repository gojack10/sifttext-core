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

After `sh scripts/build.sh`, focused checks are:

```sh
cargo test --locked --lib process_framing_tests -- --nocapture --test-threads=1
cargo test --locked --lib policy_deadline_tests -- --nocapture --test-threads=1
SIFTTEXT_SERIAL_PROCESS_TEST=1 cargo test --locked --lib \
  process_framing_tests::repeated_process_rejection -- --nocapture --test-threads=1
```

These checks do not establish full native capacity, platform qualification or main
acceptance. Keep the current maintained acceptance and affected native consumers.
