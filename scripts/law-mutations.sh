#!/bin/sh
# Convenience entry; fixture generation and checked-law assertions live in Rust.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
exec sh "$ROOT/scripts/with-tools.sh" cargo test --locked --test tool_consumers implementation_law_mutations_are_rejected -- --exact --nocapture
