# spike/ — THROWAWAY feasibility experiments (not app code)

Run on: Pop!_OS 24.04, COSMIC 1.11 packages (cosmic-comp 0.1~1792508925, commit 8cb1802),
two outputs (eDP-1 1920x1080 @0,0 and HDMI-A-1 1920x1080 @1920,0, both scale 1.0),
libcosmic pinned at 430f3d48 (same rev as cosmic-notifications / cosmic-files), Rust 1.93,
software renderer (tiny-skia) unless noted.

## Contents
- `notes-spike/` — libcosmic app; layer-shell notes. Flags:
  `--arch per-note|single --layer background|bottom|top --kb none|ondemand|exclusive
   --raise-on-edit --notes N --at X,Y`. Logs every event as `[ms] EVENT ...` to stderr.
- `tools/wlshot/` — screenshot of every output via `ext-image-copy-capture-v1`
  (the portal refuses non-interactive capture). `CROP=x,y,w,h`, `PAINT_CURSOR=1`.
- `tools/fakeinput.py` — real pointer/keyboard events through `/dev/uinput`
  (user has an ACL on it). `--fifo` daemon mode keeps the devices alive.
- `tools/itest.sh` — scripted interactive test on eDP-1: click+type, drag header,
  resize grip, title edit, focus switch, click-through on empty desktop.
  Every click is gated on the spike logging POINTER_ENTER; typing is gated on our
  surface holding keyboard focus.
- `tools/measure.sh` — screenshot + RSS + idle CPU ticks.
- (an `env.sh` pkg-config shim was used while `libxkbcommon-dev` was missing; removed)
- `Cargo.lock` seeded from cosmic-notifications: libcosmic HEAD does not resolve from
  scratch (dbus-settings-bindings renamed its `cosmic-settings-daemon` package).

## Compositor facts (wl_registry dump)
`zwlr_layer_shell_v1` v5 (set_layer, on_demand keyboard), `zcosmic_overlap_notify_v1`,
`ext_background_effect_manager_v1`, `ext_image_copy_capture_manager_v1`,
`xdg_activation_v1`, `wp_fractional_scale_manager_v1`, `wp_viewporter`.
Source (cosmic-comp `state.rs`): layer-shell global is filtered with
`client_not_sandboxed` → **Flatpak/sandboxed clients cannot use layer-shell**.

## Results (logs in logs/, screenshots in shots/)
| Test | Result |
|---|---|
| a) Bottom layer renders behind windows, above wallpaper; dock (Top) drawn over it | PASS (a-bottom) |
| a) Background layer renders above cosmic-bg wallpaper, behind windows | PASS (a-background) |
| b) pointer on Bottom-layer surface: enter/press/implicit grab outside bounds | PASS |
| b) **pointer on Background-layer surface** | **FAIL: no pointer events at all** (r3, r3b: cursor drawn on top of note, zero events) |
| c) Bottom + OnDemand: click gives keyboard focus, typing reaches text_editor/text_input | PASS (r1b, r2) |
| c) Bottom + Exclusive | same as OnDemand (exclusivity only enforced for Top/Overlay) (r5) |
| c) Bottom + None | pointer works, never gets keyboard focus (control) (r4) |
| c) clicking bare desktop does **not** take keyboard focus away from a note | observed (r2 step 6) |
| c) fallback raise-on-edit (set_layer Top + Exclusive, Esc lowers) | works mechanically; Exclusive steals focus back from other notes (r6) |
| d) multiple notes on both outputs | PASS (render on both; interaction tested on eDP-1 only) |
| e) per-note surfaces: drag by set_margin | **FAIL/unusable**: margin applied with ~250–600 ms lag; pointer coords relative to the lagging position → note runs away (r1b) even with 300 ms throttle (r7) |
| e) per-note resize by set_size | works |
| e) single full-output surface + input region: drag | PASS exact: (200,200)→(700,500) in 41 smooth steps |
| e) single: resize | PASS exact 260x240→360x340 |
| e) single: clicks outside notes go through to desktop (input region) | PASS |
| e) single: focus moves between notes on same surface; title + body typing | PASS |
| libcosmic auto-blur on layer surfaces (theme "frosted system interface") | blurs the **whole desktop** behind a full-output transparent surface; fix: `core.set_auto_blur(..)` without `Auto::System` |

## Resource use (software renderer, 2 notes per output, 2 outputs)
| arch | RSS | idle CPU (10 s windows) |
|---|---|---|
| per-note (4 surfaces) | ~46 MB | 0 ticks |
| single (2 full-output surfaces) | ~80 MB | 0 ticks |
| single, **release** build | ~58 MB | 0 ticks; ~37% of one core only *while* dragging (~60 events/s) |
| any arch, **wgpu** renderer (debug) | ~130 MB + GPU memory | 0 ticks |

## Extra observations
- Keyboard focus: setting keyboard interactivity to `None` does not revoke focus (r8; cosmic-comp
  `focus_target_is_valid` keeps any mapped layer surface valid). Only clicking a window or
  unmapping the surface does.
- Typed letters arrive uppercase in the logs (`HELLO`): likely Caps Lock or the session's
  modifier state. Irrelevant to the findings.
- Per-note drag fix attempts (grab-point formula + 20 ms / 300 ms throttle, forced redraw on
  move) did not help: the margin is applied hundreds of ms late.
- Input injection pitfalls (found during M1): cosmic-comp ignores a relative motion whose target
  lies outside every output, so cross-output moves need small steps; absolute coordinates map to
  the output under the cursor; the kernel drops an absolute event identical to the previous one.
