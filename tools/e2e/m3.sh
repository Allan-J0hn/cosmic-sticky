#!/bin/bash
# M3 acceptance: typing autosaves, drag/resize exact and persisted, restart identical,
# cross-output move persisted, idle CPU. Uses a throwaway data dir, never ~/.local/share.
# Needs: eDP-1 at 0,0 and HDMI-A-1 at 1920,0 (both 1920x1080), bare desktop on both.
set -u
cd "$(dirname "$0")"
OUT=$PWD/out/m3; rm -rf "$OUT"; mkdir -p "$OUT"
LOG=$OUT/app.log
source ./lib.sh
BIN=$ROOT/target/release/cosmic-sticky
export XDG_DATA_HOME=$OUT/data
NOTES=$XDG_DATA_HOME/cosmic-sticky/notes
mkdir -p "$NOTES"
A=aaaaaaaa-0000-4000-8000-000000000001
B=aaaaaaaa-0000-4000-8000-000000000002
C=aaaaaaaa-0000-4000-8000-000000000003
note() { # id output x y category mode title body
  printf -- '---\ntitle: %s\ncategory: %s\nmode: %s\nx: %s\n"y": %s\nwidth: 260\nheight: 240\noutput: %s\ncreated: "2026-11-04T10:00:00Z"\nmodified: "2026-11-04T10:0%s:00Z"\n---\n%b' \
    "$7" "$5" "$6" "$3" "$4" "$2" "${1: -1}" "$8" > "$NOTES/$1.md"
}
note $A eDP-1 200 200 reminders text "" ""
note $B eDP-1 520 240 self-care text "b" "second note"
note $C HDMI-A-1 300 300 todos checklist "list" "- [ ] eggs\n- [x] milk"

field() { grep -E "^\"?$2\"?: " "$NOTES/$1.md" | head -1 | sed -E 's/^[^:]+: //; s/^"//; s/"$//'; }
body() { awk 'n>=2{print} /^---$/{n++}' "$NOTES/$1.md"; }
# wait_for SECONDS CMD...: poll until CMD succeeds; prints elapsed ms
wait_for() {
  local limit=$1; shift; local t0; t0=$(date +%s%N)
  while true; do
    if "$@"; then echo $(( ($(date +%s%N) - t0) / 1000000 )); return 0; fi
    (( $(date +%s%N) - t0 > limit * 1000000000 )) && return 1
    sleep 0.05
  done
}

launch() { RUST_LOG=warn,cosmic_sticky=debug "$BIN" >>"$LOG" 2>&1 & PID=$!; sleep 3; }
cleanup() { kill "$PID" 2>/dev/null; e2e_stop_injector; }

launch
e2e_start_injector
trap cleanup EXIT
park_left; sleep 0.3

echo "== 1) type into title and body; file must match within 1 s of the last key"
hover 300 232 $A || abort "pointer not on note A title"
click_note $A || abort "click did not reach note A"
has_focus || abort "no keyboard focus"
inj "type groceries"
ms=$(wait_for 1 bash -c "grep -q '^title: groceries$' '$NOTES/$A.md'") && echo "   title saved after ${ms} ms" || echo "   FAIL title: $(field $A title)"
hover 300 300 $A || abort "pointer not on note A body"
click_note $A || abort "click did not reach note A body"
has_focus || abort "no keyboard focus"
inj "type milk and eggs"
check_body() { [ "$(body $A)" = "milk and eggs" ]; }
ms=$(wait_for 1 check_body) && echo "   body saved after ${ms} ms" || echo "   FAIL body: $(body $A | head -3)"

echo "== 2) drag header by (+500,+300): expect (700,500); then resize grip by (+100,+100): expect 360x340"
inj "key esc"; sleep 0.3
hover 330 210 $A || abort "pointer not on note A header"
t0=$(cpu_ticks $PID); s0=$(date +%s%N)
inj "down" "drag 830 510 120" "up"; sleep 0.6
t1=$(cpu_ticks $PID); s1=$(date +%s%N)
echo "   file: x=$(field $A x) y=$(field $A y) w=$(field $A width) h=$(field $A height) output=$(field $A output)"
echo "   CPU during drag: $((t1-t0)) ticks over $(( (s1-s0)/10000000 )) cs (100 ticks = 1 CPU-second)"
hover 952 732 $A || abort "pointer not on note A grip"
inj "down" "drag 1052 832 40" "up"; sleep 0.6
echo "   file: x=$(field $A x) y=$(field $A y) w=$(field $A width) h=$(field $A height)"

echo "== 3) idle CPU after editing stops (focus cleared, pointer parked)"
inj "key esc" "abs $EMPTY_X $EMPTY_Y"; sleep 2
t0=$(cpu_ticks $PID); sleep 30; t1=$(cpu_ticks $PID)
echo "   idle: $((t1-t0)) ticks / 30 s; RSS $(awk '/VmRSS/{print $2}' /proc/$PID/status) kB"

echo "== 4) kill and restart: screenshots must be identical"
CROP=0,0,1400,1000 shot before
kill $PID; wait $PID 2>/dev/null
launch
inj "abs $((EMPTY_X+1)) $EMPTY_Y"; sleep 1
CROP=0,0,1400,1000 shot after
for o in eDP-1 HDMI-A-1; do
  if cmp -s "$OUT/before-$o.png" "$OUT/after-$o.png"; then echo "   $o: identical"; else echo "   $o: DIFFERENT"; fi
done

echo "== 5) drag note B from eDP-1 onto HDMI-A-1"
hover 600 250 $B || abort "pointer not on note B header"
steps=(); for _ in $(seq 70); do steps+=("rel 25 0"); done
inj "down" "${steps[@]}" "up"; sleep 0.6
echo "   file: output=$(field $B output) x=$(field $B x) y=$(field $B y)"
grep "note moved" "$LOG" | tail -1 | sed 's/^/   /'
bx=$(field $B x); by=$(field $B y)
kill $PID; wait $PID 2>/dev/null
launch
park_right; sleep 0.3
cx=$(printf '%.0f' "$(echo "$bx + 100" | bc)"); cy=$(printf '%.0f' "$(echo "$by + 10" | bc)")
if [ "$(field $B output)" = HDMI-A-1 ] && EMPTY_X=1700 EMPTY_Y=950 hover "$cx" "$cy" $B; then
  echo "   after restart: note B is on HDMI-A-1 at ($bx,$by) (pointer hover confirmed)"
else
  echo "   FAIL: note B not found on HDMI-A-1 after restart"
fi
CROP=0,0,1920,1080 shot final
park_left
echo "done; artifacts in $OUT"
