#!/bin/bash
# Idle CPU over the minute after typing stops (covers the autosave and idle-unfocus timers).
set -u
cd "$(dirname "$0")"
OUT=$PWD/out/idle-cpu; rm -rf "$OUT"; mkdir -p "$OUT"
LOG=$OUT/app.log
source ./lib.sh
export XDG_DATA_HOME=$OUT/data
NOTES=$XDG_DATA_HOME/cosmic-sticky/notes; mkdir -p "$NOTES"
A=aaaaaaaa-0000-4000-8000-000000000001
printf -- '---\ntitle: idle\noutput: eDP-1\nx: 200\n"y": 200\n---\n' > "$NOTES/$A.md"
RUST_LOG=warn,cosmic_sticky=debug "${BIN:-$ROOT/target/release/cosmic-sticky}" >"$LOG" 2>&1 & PID=$!
sleep 3
e2e_start_injector
trap 'kill $PID 2>/dev/null; e2e_stop_injector' EXIT
park_left; sleep 0.3
hover 300 300 $A || abort "pointer not on note"
click_note $A || abort "click did not reach note"
has_focus || abort "no keyboard focus"
inj "type the quick brown fox jumps over the lazy dog" "key esc" "abs $EMPTY_X $EMPTY_Y"
sleep 2
for i in 1 2 3; do t0=$(cpu_ticks $PID); sleep 15; echo "   window $i: $(( $(cpu_ticks $PID) - t0 )) ticks / 15 s"; done
echo "   body: $(awk 'n>=2{print} /^---$/{n++}' "$NOTES/$A.md")"
