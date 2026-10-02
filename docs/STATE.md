# Project State

The living record. Read this first; update it in the same commit as your code.

Conventions and locked decisions live in [MEMORY.md](MEMORY.md) and
[HANDOFF.md](HANDOFF.md) — this file is only for what is happening now.

---

**Last updated:** 2026-09-07 by `claude`
**Phase:** 1 — it runs on real macOS; the pet is on the desktop
**Next milestone:** import a user's photo and wear it as the pet

**North star (owner, 2026-09-07):** the point of this app is that you upload a
photo and the thing in the photo becomes your pet. Matting, not generation —
the subject is cut out of the photo and animated procedurally. Local-first;
a cloud model may be offered as an opt-in, never as the default path.

## In progress

| Task | Agent | Notes |
|---|---|---|
| Transparent `NSPanel` on real macOS | `claude` | Focus + drag verified. Transparency / click-through / Spaces still unverified. |
| Import pipeline | `claude` | End to end and working. Matte backend is still the placeholder. |

Claim a task by adding a row before you start.

## Next up

Ordered. The first item gates everything else.

1. **Finish verifying the pet window on real macOS** — owner.
   Verified: clicking the pet does not steal keyboard focus (the one that
   gated the architecture), and the pet drags. Still unverified: transparency
   over a mid-tone background, click-through in the empty corners, and
   following across Spaces. Minutes of work now that the app runs.
2. **A real matte.** Everything else in the import pipeline works; the cutout
   is the placeholder `BorderFloodMatte`, which floods inward from the border
   and only survives a plain background. Backend and hosting are now settled in
   `HANDOFF.md`: BiRefNet_lite ONNX (MIT, 224 MB, pinned by revision and
   hash), run from a separate `crates/assetpipe` binary that the app spawns per
   import. macOS Vision was considered and dropped: DeskPet is meant to be
   published, and one backend everywhere beats a faster one on half the
   machines.
3. **Edge decontamination** — unpremultiply, colour decontamination, guided
      filter, ~1px feather. Matting alone leaves a halo of the original
      background and the cutout reads as pasted on.
   — unpremultiply, colour decontamination, guided filter, ~1px feather.
   Matting alone leaves a halo of the original background and the cutout reads
   as pasted on. Only meaningful once the matte produces real soft edges; the
   placeholder's edges are binary.
4. **First end-to-end webhook** — `curl` → the pet reacts.
5. **Gaze tracking** — needs eyes as separate art, so it depends on how the
   import pipeline decomposes a photo. Deferred until then.

Done in this pass: settings UI, anchor detection from alpha, breathing frames
from a single still, pack writing, and hot-swapping the active pack.

## Open questions

- **The frontend has no test runner.** The one-shot-state trap (a `poke` with
  no `next` handling and a quiescence check that parked the loop inside it)
  is exactly the kind of regression a test would catch in a second. Adding a
  runner is a small, separate task; doing it inline would have buried the fix.

- **This agent cannot execute on macOS.** Re-confirmed 2026-09-06: the shell
  available to the agent is a Linux VM with the repo mounted — no `cargo`, no
  Xcode, no AppKit. Screen control exists but Terminal is granted in
  click-only mode, so the agent cannot type commands into it either. The loop
  is therefore: agent edits files, owner runs `./tools/run-macos-dev.sh`, agent
  reads `logs/latest.log` from the mount. No output relaying by hand.

- **Memory Hub `project_id` is unknown.** The project does not exist yet and
  the MCP server has no create endpoint. Needed: the exact id string the API
  accepts, which may not equal the display name. Until then, git is the memory
  layer.
- **Codex MCP wiring is unwritten** — needs the hub's real launch command and
  whether it is stdio or HTTP.
- **Apple Developer account** ($99/yr) not yet decided. Without it,
  notarization is impossible and first-launch requires right-click → Open.
  Fine for personal use; a real obstacle for public distribution.

## Deliberately not doing

Do not reopen these without talking to the owner. Each was evaluated and
rejected for a reason recorded in `HANDOFF.md`.

- Linux support
- Custom `winit` + `wgpu` renderer
- Mac App Store distribution
- Reading OS notifications on macOS
- LLM conversation, part-based mesh animation (post-v0.1.0)

## Log

Newest first. One line per session. Never edit another agent's entry.

- **2026-10-02 `codex`** — Settings now reads the active pack on open, shows configured material directories for fixed slots, and labels their buttons “更改”; empty slots remain “上传”. Newly selected files show “待应用” until import succeeds, then settings reloads the installed pack. Original upload paths are not recorded in existing packs, so directories refer to the actual runtime sprites. Frontend build passed; installed macOS app has not been replaced.

- **2026-09-07 `claude`** — First run on real macOS hardware. Four bugs, each
  invisible in the one before it:
  (1) `NSWindowStyleMaskNonactivatingPanel` on a plain `NSWindow` raises
  `NSInternalInconsistencyException`. Fixed by `object_setClass` to `NSPanel`
  *before* setting the mask — in Tauri's `setup`, on the main thread, which the
  code now asserts rather than assumes. **Clicking the pet does not steal
  keyboard focus; the architecture holds.**
  (2) `set_click_through` mutated AppKit from the 30 Hz hitmask worker. All
  window mutation now goes through `MacPetWindow::on_main`.
  (3) PixiJS needs `pixi.js/unsafe-eval` under our CSP or the renderer never
  initialises — on a transparent window that is indistinguishable from the
  window not appearing.
  (4) A `<meta>` CSP in `index.html` with no `style-src` silently blocked the
  inline `<style>`, so the window kept the UA 8px body margin and grew
  scrollbars, and every CSS edit appeared to do nothing. One CSP, in
  `tauri.conf.json`.
  (5) `startDragging()` is a silent no-op on a non-activating panel:
  `performWindowDragWithEvent:` needs `NSApp.currentEvent` to still be the
  mouse-down, and an async IPC hop on a window that deliberately never becomes
  key guarantees it is not. Dragging is now ours (`src/ui/drag.ts`), computed
  from pointer screen coordinates against the window origin sampled at press.
  (6) The hitmask loop toggled click-through mid-drag — cursor and window
  origin are sampled at different instants and disagree while the window
  moves, so one bad sample cut the window off from the mouse and the drag
  hung. New rule, in the trait as `mouse_button_down`: never change
  interactivity while a button is held.
  (7) The tray item had no icon at all and the app icon list was empty, so the
  menu bar showed a blank slot — which reads as a broken app. Menu bar icon is
  a template image (black + alpha, tinted by the system); the app icon follows
  Apple's grid, since macOS does *not* round app icons the way iOS does.
  Also: the app is now an accessory app — no Dock slot, no Cmd-Tab, no
  application menu. Quit is in the tray menu; `open_settings` explicitly
  activates, because an accessory app is not brought forward by a click.
  Also: `load_active_pack` implemented (`core::pack`, with validation and
  hitmask building), the default CC0 pack generated (12 frames), the sprite
  renderer and contact shadow written, `AppState` collapsed to one `Arc`, and
  `capabilities/default.json` created — the app had none, so every core API
  was denied.

- **2026-09-06 `claude`** — Scaffolding generated: platform seam
  (`PetWindow` trait + macOS/Windows impls), `core::hitmask` with tests,
  `core::webhook`, `core::config`, behaviour engine, Tauri config, dual-platform
  CI with a grep check that fails the build if `cfg(target_os)` escapes
  `platform/`. Nothing has been compiled or run yet — the container has no
  network access, so `cargo` could not fetch dependencies.
