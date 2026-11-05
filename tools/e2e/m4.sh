#!/bin/bash
# M4 acceptance: category/color override/reset, checklist editing, "+" with category choice,
# fonts. Uses a throwaway data dir. Needs bare desktop on eDP-1 (left output).
set -u
cd "$(dirname "$0")"
OUT=$PWD/out/m4; rm -rf "$OUT"; mkdir -p "$OUT"
LOG=$OUT/app.log
source ./lib.sh
BIN=${BIN:-$ROOT/target/release/cosmic-sticky}
export XDG_DATA_HOME=$OUT/data
NOTES=$XDG_DATA_HOME/cosmic-sticky/notes
mkdir -p "$NOTES"
P=aaaaaaaa-0000-4000-8000-000000000001
printf -- '---\ntitle: parent\ncategory: reminders\noutput: eDP-1\nx: 200\n"y": 200\n---\nx' > "$NOTES/$P.md"
for i in 1 2 3 4; do
  font=$(echo "sans serif mono handwriting" | cut -d' ' -f$i)
  printf -- '---\ntitle: %s title\ncategory: reminders\nfont: %s\noutput: eDP-1\nx: %d\n"y": 640\n---\nThe quick brown fox jumps over the lazy dog.' \
    "$font" "$font" $((200 + (i-1)*280)) > "$NOTES/aaaaaaaa-0000-4000-8000-00000000010$i.md"
done

field() { grep -E "^\"?$2\"?: " "$NOTES/$1.md" | head -1 | sed -E "s/^[^:]+: //; s/^[\"']//; s/[\"']$//"; }
body() { awk 'n>=2{print} /^---$/{n++}' "$NOTES/$1.md"; }
settle() { sleep 0.9; }
bg() { pixel 430 400 eDP-1; }

# Layout of a 260x240 note at (200,200); see note_view.rs / pickers.rs.
NX=200; NY=200
PLUS=(213 214); CAT=(375 214); COLOR=(397 214); FONT=(421 214); MODE=(445 214)
row_y() { echo $((NY + 66 + 30 * $1)); }        # picker list row i
swatch() { echo "$((NX + 22 + 30 * $1)) $((NY + 66 + 32 * $2))"; }  # palette col, row
RESET_Y=$((NY + 130))
item_y() { echo $((NY + 69 + 24 * $1)); }        # checklist row i
CHECK_X=$((NX + 21))

RUST_LOG=warn,cosmic_sticky=debug "$BIN" >"$LOG" 2>&1 & PID=$!
sleep 3
e2e_start_injector
trap 'kill $PID 2>/dev/null; e2e_stop_injector' EXIT
park_left; sleep 0.3

# ctl X Y: hover the parent note, then click (X, Y) inside it
ctl() {
  hover $((NX + 130)) $((NY + 100)) $P || abort "pointer not on parent note"
  inj "abs $1 $2" "sleep 0.15"
  click_note $P || abort "click at ($1,$2) did not reach the parent note"
}

echo "== 1) category: color follows it"
expect "initial background" "$(bg)" "#ffd54f"
ctl "${CAT[@]}"; CROP=150,150,700,400 shot panel-category; ctl 300 "$(row_y 2)"; settle
expect "category" "$(field $P category)" "todos"
expect "override" "$(field $P color_override)" "null"
expect "background follows To-do's" "$(bg)" "#90caf9"

echo "== 2) override sticks while category changes; reset"
ctl "${COLOR[@]}"; CROP=150,150,700,400 shot panel-color; ctl $(swatch 4 0); settle
expect "override" "$(field $P color_override)" "#f48fb1"
expect "background" "$(bg)" "#f48fb1"
ctl "${CAT[@]}"; ctl 300 "$(row_y 0)"; settle
expect "category" "$(field $P category)" "self-care"
expect "override kept" "$(field $P color_override)" "#f48fb1"
expect "background kept" "$(bg)" "#f48fb1"
ctl "${COLOR[@]}"; ctl 300 $RESET_Y; settle
expect "override reset" "$(field $P color_override)" "null"
expect "background back to Self care" "$(bg)" "#a5d6a7"

echo "== 3) checklist: toggle, add, type, check and uncheck"
ctl "${MODE[@]}"; settle
expect "mode" "$(field $P mode)" "checklist"
expect "body" "$(body $P)" "- [ ] x"
ctl $((NX + 60)) "$(item_y 1)"   # "Add item"
has_focus || abort "no keyboard focus for typing"
inj "type milk" "key enter" "type eggs"; settle
CROP=150,150,700,400 shot checklist
ctl $CHECK_X "$(item_y 1)"; ctl $CHECK_X "$(item_y 0)"; ctl $CHECK_X "$(item_y 0)"; settle
expect "task list" "$(body $P | tr '\n' '|')" "- [ ] x|- [x] milk|- [ ] eggs|"
inj "key esc"

echo "== 4) \"+\" then choose To-do's"
before=$(ls "$NOTES" | wc -l)
ctl "${PLUS[@]}"; settle
NEW=$(grep "note created" "$LOG" | tail -1 | sed -E 's/.*note=([0-9a-f-]+) parent.*/\1/')
expect "files" "$(ls "$NOTES" | wc -l)" "$((before + 1))"
NNX=$(field "$NEW" x); NNX=${NNX%.*}
expect "placed right of parent" "$NNX,$(field "$NEW" y)" "476,200.0"
hover $((NNX + 130)) $((NY + 150)) "$NEW" || abort "pointer not on new note"
inj "abs $((NNX + 100)) $(row_y 2)" "sleep 0.15"
click_note "$NEW" || abort "click did not reach the new note"; settle
expect "new note category" "$(field "$NEW" category)" "todos"
expect "new note mode" "$(field "$NEW" mode)" "checklist"
inj "type shopping"; settle
expect "new note title (focus moved to title)" "$(field "$NEW" title)" "shopping"
inj "key esc"

echo "== 5) font picker: choose Handwriting on the parent"
ctl "${FONT[@]}"; CROP=150,150,700,400 shot panel-font; ctl 300 "$(row_y 3)"; settle
expect "font" "$(field $P font)" "handwriting"
inj "abs $EMPTY_X $EMPTY_Y"; sleep 1
CROP=150,150,1300,780 shot fonts
echo "failures: $FAILS; screenshots in $OUT"
