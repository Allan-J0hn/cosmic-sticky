# PLAN — cosmic-sticky

Status: **Approved 2026-11-04.** Open questions were delegated to me; decisions are in §7.
Source of truth for behavior: SPEC.md. Spike evidence: `spike/README.md` (logs in `spike/logs/`,
screenshots in `spike/shots/`).

## 0. Environment (verified 2026-11-04)
- Pop!_OS 24.04, COSMIC 1.11 packages; cosmic-comp 0.1~1792508925 (commit 8cb1802). Wayland session.
- Outputs: eDP-1 1920x1080 at (0,0) and HDMI-A-1 1920x1080 at (1920,0), both scale 1.0.
- `zwlr_layer_shell_v1` **v5** (runtime `set_layer`, `on_demand` keyboard). Also present:
  `zcosmic_overlap_notify_v1`, `ext_background_effect_manager_v1`, `xdg_activation_v1`,
  `wp_fractional_scale_manager_v1`, `ext_image_copy_capture_manager_v1`.
- cosmic-comp only offers layer-shell to **non-sandboxed** clients (`client_not_sandboxed`
  filter in `state.rs`), so a Flatpak build is impossible. Install natively only.
- Toolchain: libcosmic needs Rust **1.93** (system default is 1.88). I installed 1.93 via rustup
  (user-level) and the project will pin it in `rust-toolchain.toml`.
- `libxkbcommon-dev` and `just` are installed (2026-11-04).
- libcosmic HEAD does not resolve from a fresh lockfile: `dbus-settings-bindings` renamed its
  package. Like every COSMIC app, we pin through a committed `Cargo.lock`. It is seeded from
  cosmic-notifications, giving libcosmic `430f3d48`, the same rev the installed COSMIC apps use.

## 1. Architecture decision

**One process. Per output, one full-output transparent layer surface on the `Bottom` layer
(the "desk surface"). All notes for that output are drawn inside it as widgets, and the input
region is the union of the note rectangles. Keyboard interactivity is `OnDemand`. Software
renderer (no wgpu). libcosmic's auto-blur is disabled for layer surfaces.**

### Evidence from the spike (real cosmic-comp session, real input via /dev/uinput)
| Question | Finding |
|---|---|
| a) Behind windows? | Bottom and Background both render above the wallpaper and behind windows. The dock (Top layer) draws over them. Screenshots: `a-bottom`, `a-background`, `a-top`. |
| b) Pointer on **Background** layer | **No pointer events at all**, though the note is visible and the cursor is drawn on it (`r3`, `r3b`). Cause unconfirmed (likely cosmic-bg's full-screen Background surface is hit-tested first). **Background is unusable for interactive notes.** |
| b) Pointer on **Bottom** layer | Enter, press and implicit grab all work (motion keeps arriving outside the surface while the button is held). |
| c) Keyboard on Bottom + `OnDemand` | Click gives keyboard focus; typing reaches `text_editor` and `text_input` (`r1b`, `r2`). `Exclusive` behaves like `OnDemand` on Bottom (cosmic-comp only enforces exclusivity on Top/Overlay). `None` never gets focus (control, `r4`). |
| c) Fallback raise-on-edit | `set_layer(Top)` + `Exclusive` works at runtime and Escape lowers it again (`r6`). Tradeoff: an Exclusive Top surface steals keyboard focus back from everything, including other notes. **Not needed**, because Bottom + OnDemand already works. |
| d) Multiple notes / monitors | Notes render on both outputs, and focus moves between notes. Interaction was tested on eDP-1 only (HDMI-A-1 was covered by your terminal). |
| e) One surface **per note** | Resize via `set_size` works, but **moving via `set_margin` is unusable**: the compositor applied margins 250–600 ms late, pointer coordinates are relative to the lagging position, and the note ran away from the cursor (`r1b`). A 300 ms throttle still overshot (`r7`). |
| e) One **full-output surface + input region** | Drag exact ((200,200)→(700,500) in 41 smooth steps; a longer drag landed exactly at (1140,688)). Resize exact. Clicks outside notes fall through to the desktop. Per-note focus and typing work (`r2`). |
| Blur | With "frosted system interface" on, libcosmic requests blur behind every layer surface, which **blurs the whole desktop** behind a full-output surface. Fixed with `core.set_auto_blur(..)` without `Auto::System`. |
| Focus release | Clicking bare desktop does **not** remove keyboard focus from a note, and setting interactivity to `None` doesn't revoke it either (confirmed in `focus_target_is_valid`: a mapped layer surface stays focused). |

### Resources (2 notes/output, 2 outputs)
| Variant | RSS | Idle CPU |
|---|---|---|
| single surface, software, **release** (chosen) | **~58 MB** | **0 ticks / 20 s** |
| single surface, software, debug | ~80 MB | 0 ticks / 20 s |
| per-note surfaces, software, debug | ~46 MB | 0 ticks / 20 s |
| single or per-note, wgpu, debug | ~130 MB (+ GPU memory) | 0 ticks / 20 s |

CPU *while* dragging (single, software, ~60 motion events/s): ~37% of one core in release
(~108% in debug), dropping back to 0 as soon as the drag ends. Acceptable for a transient
interaction. In M3 I'll check whether limiting redraw to the damaged note area lowers it.

The full-output surfaces cost about 16 MB per 1080p output (double-buffered ARGB). That is the
price of exact, smooth dragging. When notes are hidden, the desk surfaces are **destroyed**,
which frees that memory.

### Consequences of the decision
- Drag, resize, z-order and pickers are all in-app widget logic in one coordinate space. Menus
  and pickers can be in-surface overlays, so no popup surfaces are needed.
- The input region must be recomputed on every create, move, resize, delete, show and hide.
- Moving a note to another monitor: during the drag, implicit grab keeps events on the source
  surface with out-of-bounds coordinates. On release, convert to global layout coordinates
  (output logical positions) and hand the note to the output containing its center. While
  dragging, the note is clipped at the monitor edge. To verify in M3.
- Keyboard focus can't be handed back to the compositor (see table). Escape, or clicking a
  note's header, unfocuses the note's text widgets, so stray keys are ignored. Note text is
  only ever entered into a widget the user clicked.
- `exclusive_zone = -1`: the desk surface covers the full output, so stored positions are
  stable even if the panel or dock changes. A note dragged under the top panel is clamped so
  its header stays reachable.
- Bottom-layer content is not shown or hit-tested on an output with a fullscreen window
  (cosmic-comp design). That's fine for wallpaper-like notes.

## 2. Storage format
**Recommendation: one Markdown file per note with YAML frontmatter.** Reasons: it's readable and
editable in any editor or phone app via Syncthing, checklists map naturally to Markdown task
lists, and diffs and conflict files are human-readable. Cost: **one new crate** for YAML
(`serde_yaml` is deprecated; chosen in M2: `serde-saphyr`, see M2 storage details). Alternative: JSON,
with zero new crates because `serde_json` is already in libcosmic's tree. It is safer to parse
but unpleasant to hand-edit, since the body is an escaped string.

```
~/.local/share/cosmic-sticky/
  notes/<uuid>.md          one note per file
  trash/<uuid>.md          deleted notes (same format + trashed_at)
  categories.yaml          [{id, name, color}]
  settings.yaml            {notes_visible: bool}
  corrupt/                 unparseable files are moved here, never deleted
```
```markdown
---
id: 6f1c…            # uuid v4
title: Groceries
category: todos       # category id
color_override: null  # or "#ffd54f"
font: handwriting     # sans | serif | mono | handwriting | <5th>
mode: checklist       # text | checklist
x: 700                # logical px, relative to the output's top-left
y: 500
width: 260
height: 240
output: eDP-1         # connector name; plus output_desc (make/model) for robustness
created: 2026-11-04T17:00:00Z
modified: 2026-11-04T17:05:00Z
---
- [ ] eggs
- [x] milk
```
- In text mode the Markdown part is the body. In checklist mode it is a task list
  (`- [ ]` / `- [x]`). There is one representation; toggling mode converts (see Q6).
- Atomic writes: write to `.<name>.tmp` in the same directory, `fsync`, `rename`, then `fsync`
  the directory. Temp files are ignored by the loader and the watcher.
- Autosave is debounced (500 ms after the last edit, one timer and no polling). Writes are
  skipped when serialized content is unchanged.
- The watcher uses `notify` (inotify; already in the dependency tree). Our own writes are
  suppressed by remembering the content hash we wrote. External changes are applied live. If
  the note is being edited at that moment, the external version is applied when editing ends.
- Trash: deleting moves the file to `trash/` with `trashed_at` added. Undo moves it back.
  Purge happens at startup and from a single timer that fires when the next item expires,
  after 7 days.
- Defaults are created on first run: categories "Self care", "Reminders", "To-do's" (the last
  one starts notes in checklist mode).

## 3. Module layout
```
Cargo.toml  Cargo.lock  rust-toolchain.toml (1.93)  justfile  build.rs (xdgen, per template)
i18n/en/cosmic_sticky.ftl          (template pattern; English only)
resources/ app.desktop, autostart.desktop, metainfo.xml, icon.svg, fonts/<OFL handwriting>.ttf + OFL.txt
src/
  main.rs            parse CLI → run_single_instance / one-shot commands
  cli.rs             flag parsing → Action enum                        [unit tested]
  config.rs          paths (XDG data dir), constants
  model/
    note.rs          Note, Mode, ChecklistItem, text<->checklist conv  [unit tested]
    category.rs      Category, defaults, CRUD + reassign-on-delete     [unit tested]
    color.rs         hex parse/format, WCAG luminance → text color     [unit tested]
    geometry.rs      clamp-to-output, cross-output handoff, hit tests  [unit tested]
  storage/
    format.rs        frontmatter (de)serialize                          [unit tested]
    fs.rs            atomic write, load dir, quarantine corrupt files  [unit tested, tempdir]
    trash.rs         move/restore/purge                                 [unit tested]
    watch.rs         notify → iced Subscription, self-write suppression
  app/
    mod.rs           cosmic::Application impl, Message, update routing
    desk.rs          per-output desk surfaces, input region, show/hide
    note_view.rs     note widget: header (hover controls), title, body/checklist, grip
    drag.rs          drag/resize state machine                         [unit tested]
    pickers.rs       category / color / font overlays
    list_window.rs   list view (xdg toplevel): search, filter, reveal, new, trash, categories
  fonts.rs           the ≤5 font choices, bundled font loading
tools/e2e/           (if you agree, see Q9) uinput + screenshot acceptance scripts, dev-only
```
Dependencies: libcosmic (features: `winit`, `wayland`, `tokio`, `multi-window`,
`single-instance`; **no** `wgpu`) plus template defaults (`i18n-embed`, `i18n-embed-fl`,
`rust-embed`, already in the tree). New crates: `serde` (in tree), the YAML crate (if
Markdown is chosen), and `uuid`/`jiff`/`notify` (all already in the tree). No `clap`: four
flags are parsed by hand.

## 4. Milestones
Every milestone also needs the CLAUDE.md definition of done: `cargo build` with no warnings,
`cargo test`, `cargo clippy` clean, acceptance checks run and reported, and a commit.
"Automated" checks use real input (uinput) and screenshots (ext-image-copy-capture) on an empty
workspace, the same way as the spike, and only with your OK each time (Q9). "Eye check" items
are for you.

### M1 — Skeleton and desk surfaces
From cosmic-app-template: pinned toolchain and lockfile, justfile (`build-debug`,
`build-release`, `run`, `check`, `test`, `install`, `uninstall`), no main window, one Bottom
desk surface per output (created and destroyed on output add/remove), blur off, input region
from a hard-coded demo note.
- [x] Release build: a screenshot shows the demo note on each output. A maximized window hides it (screenshot). *2026-11-04: shown on eDP-1, hidden behind the maximized terminal on HDMI-A-1.*
- [x] Clicks on bare desktop are not delivered (log), and clicks on the note are. *0 presses for a desktop click, 1 for a note click.*
- [x] Idle CPU 0 ticks over 60 s (release). RSS recorded and below 100 MB with 2 outputs. *4 ticks/60 s (~0.07%), 58 MB.*
- [x] Unplugging and replugging HDMI removes and recreates its surface without a crash (log). *2026-11-04: plug-in, unplug and replug of HDMI-A-1 each logged once; the app stayed up.*

### M2 — Domain model and storage (no UI)
- [x] Unit tests: frontmatter round trip (including unicode, empty body, CRLF); checklist and text conversion; hex colors; WCAG contrast picks the more readable of black/white for at least 20 colors (including mid-greys at the threshold); category CRUD; cannot delete the last category; reassign-on-delete moves every note.
  *Done:* 28 colors with fixed expectations, including #747474/#757575 → white and #767676/#777777 → black (the threshold is between #75 and #76), plus a sweep checking that the chosen color always wins and reaches ≥ 4.5:1.
- [x] Atomic write tests: target never partially written (simulated failure before rename), temp files cleaned up, a corrupt file is moved to `corrupt/` and never deleted.
  *Done:* also covers a stale `.tmp` from a crash, repeated quarantines of the same name (`bad.md`, `bad.1.md`, …), and corrupt `categories.yaml`.
- [x] Trash: move, restore, purge only items older than 7 days (injected clock).
  *Done:* exactly 7 days is purged and 6 d 23 h is kept. Restore refuses to overwrite a note that reappeared. Purge reports the next expiry for the single timer.

### M3 — Editable, movable, persistent notes
Load and render notes from storage; edit title and body; debounced autosave; drag the header;
corner grip resize; header controls shown on hover; raise on click; text color from contrast;
Escape unfocuses; output placement with fallback (a missing output is not rewritten);
cross-output move on release; clamp on output resize.
- [x] Automated: type into the title and body. The file on disk matches within 1 s.
  *Done* (`tools/e2e/m3.sh`): the title was saved 470 ms after the last key and the body 455 ms after.
- [x] Automated: drag and resize land on the exact expected rectangle and are persisted.
  *Done:* a header drag by (+500,+300) gave (700,500) in the file, and a grip drag by (+100,+100) gave 360×340.
- [x] Automated: kill and restart. Positions, sizes and text are identical (screenshot compare).
  *Done:* screenshots of both outputs before and after the restart are byte-identical.
- [x] Automated: drag a note from eDP-1 to HDMI-A-1. Its `output` changes, and after restart it is still on HDMI-A-1.
  *Done:* the file showed `output: HDMI-A-1` at (532,240). After the restart, the app reported the pointer over that note on HDMI-A-1.
- [x] Idle CPU 0 after editing stops. CPU while dragging recorded (release).
  *Done:* 0 ticks in 30 s after Escape, RSS 59 MB. Dragging for 2.7 s used 41 ticks (~15% of one core).
  With a note focused, the blinking caret cost 3–4 ticks per 20 s, and it kept going after a click
  on bare desktop because focus stays on our surface. Editors now unfocus after 30 s without input:
  0 ticks per 20 s afterwards (`tools/e2e/focus-cpu.sh`).
  Timers are cancelled when re-armed: one timer per keystroke caused an 18-tick burst 30 s
  after typing, because every message redraws both desk surfaces. Now it's ≤1 tick
  (`tools/e2e/idle-cpu.sh`). Clean regression run of `m3.sh`: all checks pass.

**M3 notes**
- Display clamping is non-destructive. A note whose output shrank or vanished is shown clamped
  (or on the top-left output) but its file is unchanged, so it returns when the output does.
  Only a user drag rewrites `x/y/output`.
- iced keeps widget state (focus, hover) by position in the tree, so raising a note reorders
  state between notes. After a raise, the clicked editor is re-focused by id. Hover is computed
  from the cursor, because `mouse_area` misses exits when the pointer leaves the input region.
- Checklist notes render read-only until M4.
- An empty `title:` in a hand-written file is YAML null and is now read as empty; it used to
  quarantine the note. Found by the e2e fixture.

### M4 — Note features: category, color, font, checklist, "+"
Category picker; color override with "reset to category color"; font picker (at most 5,
including handwriting); checklist mode (toggle, add/remove items, check/uncheck); the "+"
button creates a note with a category choice; new "To-do's" notes start as checklists.
- [x] Automated: change category, then the color follows the category. Override, then the color sticks while the category changes. Reset returns to the category color. File fields checked each time.
  *Done* (`tools/e2e/m4.sh`): on-screen pixel checks plus file fields. Reminders #ffd54f → To-do's #90caf9 → override
  #f48fb1, which is kept after switching to Self care → reset gives #a5d6a7.
- [x] Automated: toggle checklist, check and uncheck items, and confirm the file's task list.
  *Done:* the text "x" became `- [ ] x`. Add item, typing, Enter for a new item, and check/uncheck gave
  `- [ ] x / - [x] milk / - [ ] eggs` in the file.
- [x] Automated: "+" → choose "To-do's" → new note file in checklist mode next to the parent note.
  *Done:* a new file at (476,200), 16 px right of the parent, with `category: todos` and `mode: checklist`; the
  cursor lands in its title (typing "shopping" saved the title).
- [x] Unit: auto text color on every default category color plus random colors.
  *Done:* default categories, all 12 palette swatches and 5000 pseudo-random colors; the chosen text color always wins and reaches ≥ 4.5:1.
- [x] *Eye check:* each of the 5 fonts renders and the handwriting font looks right.
  *Done by screenshot* (4 fonts, per decision 5): Sans, Serif (Noto Serif), Mono and Handwriting (Shadows Into Light) are all distinct.
  Please confirm by eye after `just run`.

**M4 notes**
- Pickers open as panels inside the note (in place of its content) instead of popups. No extra
  surfaces are needed, and they work the same on every output.
- Header controls are plain clickable areas colored with the note's contrast color; themed
  buttons would ignore the note color. The icons come from the COSMIC icon theme (Cosmic → Pop → Adwaita).
- `iced::font::load` is silently dropped while no window exists (this app starts with none), so
  the bundled font is loaded straight into the renderer's global font system at startup.
- text_editor key bindings are consulted for every editor, not just the focused one; custom
  bindings (Enter, Backspace on an empty item) check focus first.
- Choosing a category while creating a note sets its mode (To-do's → checklist) only while the
  note is still blank; later category changes never convert content.

### M5 — Trash/undo, list view, category management
List view window (normal xdg toplevel): search (title and body), category filter, click to
reveal a note (see Q7), "New note", trash tab with restore and empty. Category management
(create, rename, recolor, delete with a "move notes to…" choice). Delete note → trash, with an
undo chip on the desktop (Q8).
- [x] Automated: delete a note, then undo restores it at the same place. Delete, then restore from the list view's trash.
  (`tools/e2e/m5.sh`, 2026-11-05: file moves, position 200,200 / 200,600 kept, pixel color back.)
- [x] Automated: search and filter narrow the list correctly (fixture notes). Unit tests in
  `model::search`; e2e search "milk" → 2 rows. The filter chip was confirmed by eye.
- [x] Automated: delete a category with notes; you must pick a target, all notes move, and no file is lost (file count invariant).
  Unit tests in `model::category`; confirmed by eye on fixture data: "Delete category" stays
  disabled until a target is picked, Reminders' notes (live and trashed) moved to Self care,
  file count unchanged.
- [x] *Eye check:* the list window follows the COSMIC theme (user confirmed, 2026-11-05).
- Also checked by e2e: reveal shows a note above a covering window, and Escape restores the stacking.

**M5 notes**
- The list window opens tiled (full workspace) when the workspace auto-tiles; its layout is
  width-independent. Scripted clicks into it proved brittle, so its UI wiring was checked by
  hand with a throwaway data dir; e2e covers the desk side (delete, undo, reveal).
- Reveal raises the output's desk to Top with exclusive keyboard focus and only the revealed
  note drawn. It ends on Escape, any click outside the note (including in the list window), or
  8 s without interaction with the note; typing in it extends the timer.
- The note header gains a delete control right of "+". Deleting is immediate (no confirm): the
  undo chip appears where the note was for 10 s, and the trash keeps it for 7 days.
- Deleting a category also reassigns trashed notes, so a later restore never lands in a missing
  category. The last category can't be deleted.
- New categories take the first palette color not already used.
- `--list` is only read at startup for now; M6 routes it to the running instance.

### M6 — Single instance, CLI, show/hide
libcosmic `single-instance` (D-Bus activation). `cosmic-sticky --toggle | --new [--category
NAME] | --list` is routed to the running instance, or starts one. Show/hide all destroys and
recreates the desk surfaces, and the state persists in `settings.yaml`.
- [ ] Automated: a second launch doesn't start a second process (pid count), and each flag acts on the running one.
- [ ] Automated: `--toggle` hides (no surfaces; RSS drops) and shows again. The state survives restart.
- [x] Unit: CLI parsing, including bad input (`cli` tests; bad input exits with code 2 and the usage).
- [ ] *Eye check:* bind `--toggle` and `--new` in COSMIC Settings → Keyboard → Custom shortcuts and confirm they work.
- From M6 on, the automated live checks are replaced by one quick manual pass by the user
  (decided 2026-11-05: working product first, fix issues found in use).

**M6 notes**
- CLI is parsed by hand (no new crate): `--toggle`, `--new [--category NAME|--category=NAME]`,
  `--list`, `-h/--help`. Category names match case-insensitively; an unknown name logs a
  warning and opens the category picker instead.
- libcosmic's `single-instance` feature forwards the command as a D-Bus action to the running
  instance (`ron` and zbus's blocking API were already in the lockfile). A plain second launch
  with no option does nothing.
- `--toggle` when nothing is running starts with the notes shown.
- `--new` places the note on the top-left output at the first free spot and reveals it above
  windows, so it can be typed into straight from a shortcut. If notes are hidden, it shows them.
- Hidden: desks keep their output geometry but have no surface; open panels close and unsaved
  edits are saved first.

### M7 — Live reload, autostart, install, final acceptance
`notify` watcher for external edits, adds and deletes; autostart `.desktop` file;
`just install`/`uninstall`; final pass over the SPEC acceptance checklist.
- [ ] Automated: edit a note file externally (including via atomic rename like Syncthing does). The note updates within 1 s, and no write loop happens (our file mtime doesn't change afterwards).
- [ ] `just install` from clean, `just uninstall` removes everything it installed.
- [ ] *Eye check:* log out and back in. Notes come back in the same places on both monitors.
- [ ] Idle CPU ~0% over 5 min with 10 notes (release); RSS recorded.
- [ ] Every item in the SPEC.md acceptance checklist is ticked, with evidence.

## 5. Risks
1. **libcosmic churn.** It is pinned via `Cargo.lock`, and upgrades are deliberate. HEAD
   currently doesn't resolve fresh.
2. **Background layer has no pointer input** on this cosmic-comp, so we use Bottom. If a later
   COSMIC version changes Bottom-layer input or ordering, notes stop being clickable. The e2e
   scripts would catch it.
3. **Desktop icons (cosmic-files desktop mode)** also use a full-screen Bottom surface with full
   input. If you enable them, their surface may stack above ours and swallow clicks or draw over
   notes. Not enabled here and untested.
4. **Keyboard focus lingers** in a note after clicking the bare desktop (compositor behavior).
   Mitigated by Escape and widget unfocus. It can't be fully fixed from the client.
5. **Memory:** about 16 MB per 1080p output while notes are shown. A 4K output costs about 64 MB.
6. **Fractional scaling** is untested (both outputs here are 100%).
7. **Cross-output drag:** the note is clipped at the monitor edge during the drag. Monitors
   with different scales or offsets need careful coordinate conversion.
8. **External-edit conflicts** (Syncthing plus a local edit): last writer wins, and the losing
   version is kept as `<id>.conflict-<time>.md`, never silently dropped.
9. **Reveal from the list view** must raise a surface temporarily. Any keyboard-exclusive raise
   must always be undone (Escape, click elsewhere, timeout), or it would lock the keyboard.
10. **Build environment:** the project path contains a space; avoid tools that split pkg-config
    flags on whitespace.

### M2 storage details (decided while implementing)
- **YAML crate: `serde-saphyr` 2.1** (MIT/Apache-2.0, actively maintained, panic-free parser, built on
  `granit-parser`). It is used with only its `serialize` and `deserialize` features. `serde_norway`
  and `serde_yaml_ng` haven't been updated since 2024.
- **The file name is the note's identity.** `<uuid>.md` is authoritative and the frontmatter `id`
  is informational, so a copied file can't create a duplicate id. A `*.md` whose name isn't a
  uuid is quarantined to `corrupt/` rather than guessed at.
- **Hand-written notes:** every frontmatter field is optional. Missing geometry defaults to
  260×240 at (0,0) and missing timestamps come from the file's mtime. Unknown fields are ignored
  (they are dropped on the next save).
- **Checklist parsing:** `-`, `*` or `+` followed by `[ ]`, `[x]` or `[X]`. Other non-blank
  lines become unchecked items, and blank lines are dropped when text becomes a checklist.
  Checklist → text → checklist is exact.
- **Categories:** ids `self-care`, `reminders` and `todos` for the defaults, uuids for new ones.
  The `checklist: true` flag (only on To-do's by default) is what makes new notes start as
  checklists, so it survives renaming. Names are unique, case-insensitively, because
  `--category NAME` looks them up by name. A note with an unknown category id displays the first
  category's color and keeps its id.
- The serializer quotes timestamps and the key `"y"` (YAML 1.1 boolean safety). The output is
  still valid YAML.

## 6. Deviations / additions vs SPEC (accepted)
- Layer is **Bottom**, not Background (Background gets no pointer input here).
- A `trashed_at` field is added to trashed notes, since the 7-day purge needs a timestamp.
- Z-order between overlapping notes: the last-clicked note is on top, and the order is not
  persisted (the data model has no z field).
- No Flatpak (the compositor forbids layer-shell for sandboxed apps).

## 7. Decisions (questions delegated on 2026-11-04)
1. **App ID:** `io.github.Allan_J0hn.CosmicSticky`. libcosmic derives a D-Bus object path from
   the app ID, and hyphens are invalid there, hence `_`. The binary is `cosmic-sticky` and the
   data dir is `~/.local/share/cosmic-sticky/`.
2. **Storage:** Markdown + YAML frontmatter as in §2. The YAML crate is chosen and justified in M2.
3. **`--list`** opens the list view window in the running instance.
4. **`--new` without `--category`** creates the note with its category picker open. `--new
   --category NAME` skips the picker.
5. **Fonts (4):** Sans (COSMIC UI font), Serif (Noto Serif), Mono (Noto Sans Mono), Handwriting
   (bundled *Shadows Into Light*, OFL, with license file). Missing system fonts fall back to Sans.
6. **Text ↔ checklist** is lossless: lines become items, and items become `- [ ]`/`- [x]` lines.
7. **Reveal** works as proposed: the output's desk surface goes to Top showing only that note,
   until Escape, a click elsewhere, or 8 s.
8. **Undo** works as proposed: a "Deleted — Undo" chip for 10 s, plus restore from the list view's trash.
9. **e2e helpers** live in `tools/e2e/` (dev-only). Every run is announced first and stays gated:
   clicks only after the pointer is confirmed on a note, typing only while a note has focus.
10. **Install** to `~/.local` (no sudo). `PREFIX` can be overridden.

Deviations in §6 are accepted.
