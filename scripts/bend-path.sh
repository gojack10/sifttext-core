#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
. "$ROOT/scripts/tool-pins.sh"
platform_pins
BEND=${BEND_BIN:-"$ROOT/.tools/bend-$PLATFORM/bend/bin/bend"}
case "$BEND" in /*) ;; *) BEND=$ROOT/$BEND ;; esac
[ -f "$BEND" ] && [ -x "$BEND" ] || { echo 'Pinned Bend missing; run sh scripts/setup-tools.sh' >&2; exit 1; }
[ "$(tool_sha256 "$BEND")" = "$BEND_SHA256" ] || { echo 'Bend binary digest mismatch' >&2; exit 1; }
[ "$("$BEND" version)" = 'bend 2.0.27' ] || { echo 'Bend version mismatch' >&2; exit 1; }
printf '%s\n' "$BEND"
