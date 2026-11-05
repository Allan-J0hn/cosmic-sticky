# stickies

Sticky notes pinned to the desktop for the COSMIC desktop (Wayland), on Pop!_OS.
Notes behave like wallpaper widgets: they sit on the desktop BEHIND normal windows
and are visible when the desktop is showing. Simple and lightweight.

## Tech
- Rust + libcosmic (iced) so it matches the COSMIC theme. Start from pop-os/cosmic-app-template.
- Pinning uses Wayland layer-shell (Background or Bottom layer). Verify what the
  running compositor (cosmic-comp) actually supports before assuming anything.
- Build/install with `just`. Verify the environment first (/etc/os-release, COSMIC version).
- Idle CPU should be ~0% (no polling), modest memory. No Electron/web stack.

## Data model
Note: id, title, body, category, color_override (optional), font, mode (text|checklist),
checklist items (text + checked), position, size, monitor/output, created/modified times.
Category: id, name, color.
Default categories: "Self care", "Reminders", "To-do's".

## Behavior
Colors
- A note's color comes from its category by default.
- The user can override the color per note; the override sticks until reset to category color.
- Text color switches light/dark automatically so it stays readable on any color.

Categories
- User can create, rename, recolor, and delete categories (at least one must always remain).
- Deleting a category asks where to move its notes; notes are never silently lost.
- Changing a note's category later is supported.

Notes
- Editable title and body; category picker; color picker; font picker (max 5 fonts;
  use system fonts with fallbacks, or bundle OFL-licensed fonts; include a handwriting-style option).
- Checklist mode: lines with checkboxes. Notes created in "To-do's" start in checklist mode;
  any note can toggle between text and checklist.
- A "+" button on every note creates a new note on the desktop, with a category choice at creation.
- Move by dragging the header; resize from a corner handle. Header controls can be
  hidden until hover to keep notes clean.
- Deleting a note moves it to a trash with undo; trash is emptied after ~7 days.

App-level
- Show/hide all notes: one toggle. Expose as a CLI command (e.g. `cosmic-sticky --toggle`,
  also `--new`, `--list`) routed to the running instance (single instance, via D-Bus or socket),
  so the user can bind them to COSMIC keyboard shortcuts.
- List view: a normal window listing all notes with search, category filter, click to
  reveal/focus a note, "New note" button, and trash/restore.
- Autostart on login (~/.config/autostart .desktop file).
- Everything autosaves; positions, sizes, and monitor are restored after reboot.
- Multi-monitor aware.

## Storage
- Plain files under ~/.local/share/cosmic-sticky/: one file per note (Markdown with YAML
  frontmatter, or JSON; propose which in PLAN.md) plus categories and settings files.
- Atomic writes (write temp then rename). Watch the folder so external changes
  (e.g. Syncthing) are picked up live.

## Out of scope
Due dates/reminders, rich text, images, built-in sync, accounts.

## Acceptance checklist
- [x] Notes render pinned behind normal windows and survive logout/login
- [x] Can click into a note, type in title and body, check/uncheck items
- [x] Drag and resize work; position/size persist
- [x] Create note via "+" and via CLI, choosing a category
- [x] Category CRUD with reassign-on-delete; per-note color override and reset
- [x] Font picker (<=5), auto text contrast
- [x] Trash + undo; show/hide all; list view with search and filter
- [x] Plain-file storage with live reload of external edits
- [x] `just install` works; autostart works; idle CPU ~0%
