#!/bin/bash
# THROWAWAY interactive test on eDP-1 (left output) with real uinput events.
# usage: tools/itest.sh <tag> <spike args...>
# Notes are placed at origin 200,200 on each output: note A = (200,200 260x240), note B = (520,240).
# Safety: every click is gated on the spike logging POINTER_ENTER on our surface after the
# hover; typing is gated on one of our surfaces holding keyboard focus.
cd "$(dirname "$0")/.."
tag=$1; shift
log=logs/$tag.log
fifo=$(mktemp -u "${XDG_RUNTIME_DIR}/stickies-fi.XXXX"); mkfifo "$fifo"
python3 -I tools/fakeinput.py --fifo "$fifo" &
fipid=$!
exec 3>"$fifo"
./target/debug/notes-spike --at 200,200 "$@" > "$log" 2>&1 &
pid=$!
cleanup() { echo quit >&3; exec 3>&-; kill $pid 2>/dev/null; wait $fipid 2>/dev/null; rm -f "$fifo" "$fifo.done"; }
trap cleanup EXIT
sleep 2.5
# inj: send commands and block until the injector has executed all of them
inj() {
  local mark="$fifo.done"; rm -f "$mark"
  for c in "$@"; do echo "$c" >&3; done
  echo "mark $mark" >&3
  for _ in $(seq 200); do [ -e "$mark" ] && break; sleep 0.05; done
}
n() { grep -c "$1" "$log"; }
# true only if (a) the latest click was delivered to one of our surfaces and
# (b) one of our surfaces currently holds keyboard focus (per-surface focus tracking)
focus_ok() {
  [ "$(n POINTER_PRESS)" -gt "${PRESSES_BEFORE:-0}" ] || return 1
  awk '/KEYBOARD_FOCUS/{ match($0,/surface=Id\([0-9]+\)/); sid=substr($0,RSTART,RLENGTH);
                         st[sid] = ($0 ~ /focused=true/) }
       END{ for (k in st) if (st[k]) exit 0; exit 1 }' "$log"
}
# hover X Y: move from an empty spot onto (X,Y); succeed only if our surface reports ENTER
hover() {
  local before; before=$(n POINTER_ENTER)
  inj "abs 1300 760" "sleep 0.15" "abs $1 $2"; sleep 0.5
  [ "$(n POINTER_ENTER)" -gt "$before" ]
}
abort() { echo "   ABORT: $1 (pointer not on our note; not clicking)"; exit 1; }
rect_of() { grep "DRAG_END note=$1" "$log" | tail -1 | sed -E 's/.*x: ([0-9.]+), y: ([0-9.]+), width: ([0-9.]+), height: ([0-9.]+).*/\1 \2 \3 \4/' | awk '{printf "%d %d %d %d\n",$1,$2,$3,$4}'; }

inj "rel -3000 0" "rel -3000 0" "rel -3000 0"; sleep 0.5   # park pointer on eDP-1 (left output)

echo "== $tag: 1) click note A body, type 'hello' (focus-gated)"
hover 300 330 || abort "step1"
PRESSES_BEFORE=$(n POINTER_PRESS); inj "click"; sleep 0.5
if focus_ok; then inj "type hello"; sleep 0.8; echo "   $(grep 'TEXT body' "$log" | tail -1)"; else echo "   NO KEYBOARD FOCUS -> typing skipped"; fi
grep -E "KEYBOARD_FOCUS|RAISE|LOWER" "$log" | tail -3 | sed 's/^/   /'

echo "== 2) drag note A header (260,212) -> (760,512); expect pos (700,500)"
hover 260 212 || abort "step2"
inj "down" "drag 760 512 40" "up"; sleep 0.8
grep -E "DRAG_END" "$log" | tail -1 | sed 's/^/   /'

echo "== 3) resize note A from grip by +100,+100; expect size (360,340)"
read ax ay aw ah < <(rect_of 1)
gx=$((ax+aw-10)); gy=$((ay+ah-10))
hover $gx $gy || abort "step3"
inj "down" "drag $((gx+100)) $((gy+100)) 30" "up"; sleep 0.8
grep -E "DRAG_END" "$log" | tail -1 | sed 's/^/   /'

echo "== 4) click title field of note A, type 'x' (focus-gated)"
read ax ay aw ah < <(rect_of 1)
hover $((ax+100)) $((ay+46)) || abort "step4"
PRESSES_BEFORE=$(n POINTER_PRESS); inj "click"; sleep 0.5
if focus_ok; then inj "type x"; sleep 0.5; echo "   $(grep 'TEXT title' "$log" | tail -1)"; else echo "   NO FOCUS -> skipped"; fi

echo "== 5) click note B body, type 'b' (focus moves between notes)"
hover 620 370 || abort "step5"
PRESSES_BEFORE=$(n POINTER_PRESS); inj "click"; sleep 0.5
if focus_ok; then inj "type b"; sleep 0.5; echo "   $(grep 'TEXT body note=2' "$log" | tail -1)"; else echo "   NO FOCUS -> skipped"; fi
grep -E "KEYBOARD_FOCUS" "$log" | tail -2 | sed 's/^/   /'

if [ -n "$ESC_TEST" ]; then
  echo "== 5b) press Escape while editing note B"
  if focus_ok; then inj "key esc"; sleep 0.5; fi
  grep -E "RAISE|LOWER|KEY " "$log" | tail -3 | sed 's/^/   /'
fi

echo "== 6) click empty desktop (1300,760) on eDP-1: must NOT reach our surfaces"
before=$(n POINTER_PRESS)
inj "abs 1300 760" "sleep 0.2" "click"; sleep 0.5
echo "   presses on our surfaces before=$before after=$(n POINTER_PRESS)"
grep -E "KEYBOARD_FOCUS" "$log" | tail -1 | sed 's/^/   /'

CROP=0,150,1300,800 ./target/debug/wlshot shots "$tag-" | grep eDP | sed 's/^/   /'
echo "   drag moves logged=$(n DRAG_MOVE)"
