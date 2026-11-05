#!/bin/bash
# M5 acceptance: delete + undo, restore from the list's trash, search and filter, reveal,
# deleting a category with notes. Uses a throwaway data dir. Needs bare desktop on eDP-1.
set -u
cd "$(dirname "$0")"
OUT=$PWD/out/m5; rm -rf "$OUT"; mkdir -p "$OUT"
LOG=$OUT/app.log
source ./lib.sh
BIN=${BIN:-$ROOT/target/release/cosmic-sticky}
export XDG_DATA_HOME=$OUT/data
DATA=$XDG_DATA_HOME/cosmic-sticky
NOTES=$DATA/notes; TRASH=$DATA/trash
mkdir -p "$NOTES" "$TRASH"

A=aaaaaaaa-0000-4000-8000-00000000000a   # groceries, deleted then undone
B=aaaaaaaa-0000-4000-8000-00000000000b   # dentist, deleted then restored from the list
C=aaaaaaaa-0000-4000-8000-00000000000c   # stretch (self care)
D=aaaaaaaa-0000-4000-8000-00000000000d   # under the list window, revealed
T=aaaaaaaa-0000-4000-8000-0000000000ff   # already in the trash
fixture() { # FILE TITLE CATEGORY X Y MODIFIED BODY [EXTRA]
  printf -- '---\ntitle: %s\ncategory: %s\noutput: eDP-1\nx: %d\n"y": %d\nmodified: %s\n%b---\n%s' \
    "$2" "$3" "$4" "$5" "$6" "${8:-}" "$7" > "$1"
}
fixture "$NOTES/$A.md" groceries reminders 200 200 2026-01-01T00:00:01Z "milk and eggs"
fixture "$NOTES/$B.md" dentist reminders 200 600 2026-01-01T00:00:02Z "call on tuesday"
fixture "$NOTES/$C.md" stretch self-care 1500 200 2026-01-01T00:00:03Z "ten minutes"
fixture "$NOTES/$D.md" hidden todos 830 420 2026-01-01T00:00:04Z "milk run"
fixture "$TRASH/$T.md" old reminders 600 600 2026-01-01T00:00:00Z "gone" \
  "trashed_at: $(date -u +%FT%TZ)\n"

field() { grep -E "^\"?$2\"?: " "$1" | head -1 | sed -E "s/^[^:]+: //; s/^[\"']//; s/[\"']$//"; }
files() { find "$NOTES" "$TRASH" -name '*.md' | wc -l; }
settle() { sleep 0.9; }
last_list() { grep "list_window: list " "$LOG" | tail -1 | sed -E 's/.*list_window: list //'; }

start() { RUST_LOG=warn,cosmic_sticky=trace "$BIN" "$@" >>"$LOG" 2>&1 & PID=$!; sleep 3; }
start
e2e_start_injector
trap 'kill $PID 2>/dev/null; e2e_stop_injector' EXIT
park_left; sleep 0.3
FILES0=$(files)

# Note at (200,200): "+" at 213, delete at 239 in the header row.
echo "== 1) delete, then undo from the chip"
hover 330 300 $A || abort "pointer not on note A"
inj "abs 239 214" "sleep 0.15"
inj "click"; sleep 0.4
[ "$(count "note trashed note=$A")" = 1 ] || abort "delete click did not reach note A"
settle
expect "A in trash" "$([ -f "$TRASH/$A.md" ] && [ ! -f "$NOTES/$A.md" ] && echo yes)" "yes"
CROP=150,150,500,200 shot chip
inj "abs 300 300" "sleep 0.1" "abs 412 222" "sleep 0.3"
grep -q "over undo chip" "$LOG" || abort "pointer not over the undo chip"
inj "click"; settle
expect "A restored" "$(count "note restored note=$A")" "1"
expect "A back in notes" "$([ -f "$NOTES/$A.md" ] && [ ! -f "$TRASH/$A.md" ] && echo yes)" "yes"
expect "A position" "$(field "$NOTES/$A.md" x),$(field "$NOTES/$A.md" y)" "200.0,200.0"
expect "A drawn" "$(pixel 330 400 eDP-1)" "#ffd54f"

echo "== 2) delete B (restored from the list later)"
hover 330 700 $B || abort "pointer not on note B"
inj "abs 239 614" "sleep 0.15"
inj "click"; sleep 0.4
[ "$(count "note trashed note=$B")" = 1 ] || abort "delete click did not reach note B"
settle
expect "B in trash" "$([ -f "$TRASH/$B.md" ] && echo yes)" "yes"

echo "== 3) restart with the list window; find it"
kill $PID; wait $PID 2>/dev/null
start --list
size=$(grep "list size" "$LOG" | tail -1)
[ -n "$size" ] || abort "list window size unknown"
W=$(sed -E 's/.*width=([0-9]+).*/\1/' <<<"$size"); H=$(sed -E 's/.*height=([0-9]+).*/\1/' <<<"$size")
for probe in "960 540" "300 400" "1500 700"; do
  inj "abs $EMPTY_X $EMPTY_Y" "sleep 0.1" "abs $probe" "sleep 0.4"
  pos=$(grep "list cursor" "$LOG" | tail -1)
  [ -n "$pos" ] && break
done
[ -n "$pos" ] || abort "list window not found"
read -r PX PY <<<"$probe"
LX=$(sed -E 's/.*x=([0-9.]+).*/\1/' <<<"$pos" | cut -d. -f1)
LY=$(sed -E 's/.*y=([0-9.]+).*/\1/' <<<"$pos" | cut -d. -f1)
OX=$((PX - LX)); OY=$((PY - LY))
echo "   list window: ${W}x$H at $OX,$OY"
CROP=$OX,$OY,$W,$H shot list-notes

# lclick X Y: click at window-local (X, Y), only inside the window and after the app saw the
# pointer there.
lclick() {
  [ "$1" -gt 4 ] && [ "$1" -lt $((W - 4)) ] && [ "$2" -gt 4 ] && [ "$2" -lt $((H - 4)) ] \
    || abort "($1,$2) is outside the list window"
  inj "abs $((OX + $1)) $((OY + $2))" "sleep 0.3"
  grep "list cursor" "$LOG" | tail -1 | sed -E 's/.*x=([0-9.]+) y=([0-9.]+).*/\1 \2/' \
    | awk -v x="$1" -v y="$2" '{ exit !(($1-x)^2 < 2 && ($2-y)^2 < 2) }' \
    || abort "pointer not on the list window at ($1,$2)"
  inj "click"; sleep 0.5
}
TAB_Y=58; TAB_NOTES=$((16 + (W - 32) / 6)); TAB_TRASH=$((W / 2)); TAB_CATS=$((W - 16 - (W - 32) / 6))

echo "== 4) restore B from the trash tab"
lclick $TAB_TRASH $TAB_Y
CROP=$OX,$OY,$W,$H shot list-trash
expect "trash tab" "$(last_list | cut -c1-4)" "Tab("
lclick $((W - 70)) 146; settle
expect "restore message" "$(last_list)" "Restore($B)"
expect "B back in notes" "$([ -f "$NOTES/$B.md" ] && [ ! -f "$TRASH/$B.md" ] && echo yes)" "yes"
expect "B position" "$(field "$NOTES/$B.md" x),$(field "$NOTES/$B.md" y)" "200.0,600.0"

echo "== 5) search and filter"
lclick $TAB_NOTES $TAB_Y
lclick 150 102
grep "list focus" "$LOG" | tail -1 | grep -q "focused=true" || abort "list has no keyboard focus"
inj "type milk"; settle
expect "search milk" "$(grep 'list filtered' "$LOG" | tail -1 | grep -o 'count=[0-9]*')" "count=2"
CROP=$OX,$OY,$W,$H shot list-search
lclick 170 142
expect "filter message" "$(last_list)" 'Filter(Some(CategoryId("reminders")))'
expect "milk + Reminders" "$(grep 'list filtered' "$LOG" | tail -1 | grep -o 'count=[0-9]*')" "count=1"
lclick 31 142
expect "milk + All" "$(grep 'list filtered' "$LOG" | tail -1 | grep -o 'count=[0-9]*')" "count=2"

echo "== 6) reveal the note under the window"
covered=$(pixel 960 540 eDP-1)
lclick 150 196
expect "reveal" "$(grep -c "reveal note=$D" "$LOG")" "1"
sleep 0.5
CROP=700,300,600,500 shot reveal
expect "D shown above the window" "$(pixel 960 540 eDP-1)" "#90caf9"
inj "key esc"; settle
expect "reveal ended" "$(grep -c "reveal ended note=$D" "$LOG")" "1"
expect "window on top again" "$(pixel 960 540 eDP-1)" "$covered"

echo "== 7) delete Reminders, moving its notes to Self care"
lclick $TAB_CATS $TAB_Y
CROP=$OX,$OY,$W,$H shot list-categories
lclick $((W - 45)) 145
expect "delete asked" "$(last_list)" 'Edit(CategoryId("reminders"), Some(Delete { target: None }))'
CROP=$OX,$OY,$W,$H shot list-delete
lclick $((W - 91)) 240
expect "no delete without a target" "$(grep -c 'category deleted' "$LOG")" "0"
lclick 67 207
expect "target" "$(last_list)" 'DeleteTarget(CategoryId("reminders"), CategoryId("self-care"))'
lclick $((W - 91)) 240; settle
expect "category deleted" "$(grep -c 'category deleted' "$LOG")" "1"
expect "categories.yaml" "$(grep -c reminders "$DATA/categories.yaml")" "0"
expect "A moved" "$(field "$NOTES/$A.md" category)" "self-care"
expect "B moved" "$(field "$NOTES/$B.md" category)" "self-care"
expect "trashed T moved" "$(field "$TRASH/$T.md" category)" "self-care"
expect "D untouched" "$(field "$NOTES/$D.md" category)" "todos"
expect "file count" "$(files)" "$FILES0"
CROP=$OX,$OY,$W,$H shot list-after

echo "== $( [ $FAILS -eq 0 ] && echo ALL PASSED || echo "$FAILS FAILED")"
