#!/bin/bash
# Idle CPU while a note keeps keyboard focus (caret visible) after clicking bare desktop.
set -u
cd "$(dirname "$0")"
OUT=$PWD/out/focus-cpu; rm -rf "$OUT"; mkdir -p "$OUT"
LOG=$OUT/app.log
source ./lib.sh
export XDG_DATA_HOME=$OUT/data
NOTES=$XDG_DATA_HOME/cosmic-sticky/notes; mkdir -p "$NOTES"
A=aaaaaaaa-0000-4000-8000-000000000001
printf -- '---\ntitle: focus\noutput: eDP-1\nx: 200\n"y": 200\n---\nbody\n' > "$NOTES/$A.md"
RUST_LOG=warn,cosmic_sticky=debug "${BIN:-$ROOT/target/release/cosmic-sticky}" >"$LOG" 2>&1 & PID=$!
sleep 3
e2e_start_injector
trap 'kill $PID 2>/dev/null; e2e_stop_injector' EXIT
park_left; sleep 0.3
measure() { local t0; t0=$(cpu_ticks $PID); sleep "$2"; echo "   $1: $(( $(cpu_ticks $PID) - t0 )) ticks / $2 s"; }

hover 300 300 $A || abort "pointer not on note"
click_note $A || abort "click did not reach note"
has_focus || abort "no keyboard focus"
inj "abs $EMPTY_X $EMPTY_Y"; sleep 1
measure "focused, pointer off note" 20
inj "click"; sleep 1
grep "keyboard focus" "$LOG" | tail -1 | sed 's/^.*DEBUG/   last focus event:/'
sleep 15
grep -q "unfocusing idle editor" "$LOG" && echo "   idle unfocus fired" || echo "   idle unfocus did NOT fire"
measure "after idle unfocus (30 s after last input)" 20
