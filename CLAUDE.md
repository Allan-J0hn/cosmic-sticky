# CLAUDE.md

Read SPEC.md fully before doing anything. It is the source of truth for what this app does.
If PLAN.md exists, read it too; it is the source of truth for architecture and milestones.

## Project
cosmic-sticky: sticky notes pinned to the desktop for the COSMIC desktop on Pop!_OS.
Rust + libcosmic. Lightweight; idle CPU ~0%.

## Workflow rules
- Work one milestone at a time, in the order given in PLAN.md. Don't start the next one
  until told to.
- Don't write real app code before PLAN.md exists and has been approved. Throwaway
  experiments go in spike/ only.
- Don't assume Wayland/layer-shell behavior. Test it on this machine's real COSMIC session
  and report what you observed.
- Don't add features outside SPEC.md. If something seems missing or ambiguous, ask or
  note it in PLAN.md rather than guessing.
- If you can't visually verify UI behavior yourself, say so and tell me exactly what to
  check by eye.

## Definition of done (every milestone)
1. `cargo build` succeeds with no warnings
2. `cargo test` passes (write tests for all non-UI logic: storage, categories, colors, contrast)
3. `cargo clippy` is clean
4. The milestone's acceptance checks from PLAN.md have been run and results reported
5. Changes committed with a clear message

## Code conventions
- Follow libcosmic and cosmic-app-template patterns; use the COSMIC theme rather than
  hardcoded styling, except for user-chosen note colors.
- Keep dependencies minimal; justify any new crate in the commit message.
- Storage writes must be atomic (temp file, then rename). Never corrupt or lose user notes.
- Build and install via `just`. Keep the justfile working.
- No polling loops; use events/watchers.

## Reporting
End every task with a short summary: what's done, what isn't, deviations from spec, and
anything I need to decide or verify.
