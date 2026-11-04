# Shared helpers for the e2e scripts. Source it; needs $OUT and $LOG set.
# Safety: clicks only after our app reported the pointer over the expected note; typing only
# after that click reached our note and one of our surfaces holds keyboard focus.

E2E_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$E2E_DIR/../.." && pwd)
WLSHOT=$E2E_DIR/wlshot/target/release/wlshot

e2e_start_injector() {
  FIFO=$(mktemp -u "${XDG_RUNTIME_DIR}/cosmic-sticky-e2e.XXXX"); mkfifo "$FIFO"
  python3 -I "$E2E_DIR/fakeinput.py" --fifo "$FIFO" &
  FIPID=$!
  exec 3>"$FIFO"
  sleep 2.5
}

e2e_stop_injector() {
  echo quit >&3 2>/dev/null; exec 3>&-
  wait "$FIPID" 2>/dev/null; rm -f "$FIFO" "$FIFO.done"
}

# inj CMD...: send commands and block until the injector has run all of them
inj() {
  local mark="$FIFO.done"; rm -f "$mark"
  for c in "$@"; do echo "$c" >&3; done
  echo "mark $mark" >&3
  for _ in $(seq 400); do [ -e "$mark" ] && break; sleep 0.05; done
}

count() { grep -c -- "$1" "$LOG"; }

# Small relative steps: cosmic-comp ignores a relative move whose target is off every output.
park_left()  { local s=(); for _ in $(seq 80); do s+=("rel -40 0"); done; inj "${s[@]}"; }
park_right() { local s=(); for _ in $(seq 80); do s+=("rel 40 0"); done; inj "${s[@]}"; }

# hover X Y NOTE_ID: move from an empty spot onto (X, Y); true only if the app saw the pointer
# enter that note. The empty spot must be bare desktop.
EMPTY_X=${EMPTY_X:-1300}; EMPTY_Y=${EMPTY_Y:-950}
hover() {
  local before; before=$(count "hover note=Some($3)")
  inj "abs $EMPTY_X $EMPTY_Y" "sleep 0.15" "abs $1 $2"; sleep 0.4
  [ "$(count "hover note=Some($3)")" -gt "$before" ]
}

# click_note NOTE_ID: click and confirm the press reached that note
click_note() {
  local before; before=$(count "press cursor=.*note=Some($1)")
  inj "click"; sleep 0.4
  [ "$(count "press cursor=.*note=Some($1)")" -gt "$before" ]
}

has_focus() {
  grep "keyboard focus" "$LOG" | tail -1 | grep -q "focused=true"
}

abort() { echo "ABORT: $1"; exit 1; }

cpu_ticks() { awk '{print $14+$15}' "/proc/$1/stat"; }

shot() { CROP=$CROP "$WLSHOT" "$OUT" "$1-" >/dev/null; }
