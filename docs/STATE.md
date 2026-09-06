# Project State

The living record. Read this first; update it in the same commit as your code.

Conventions and locked decisions live in [MEMORY.md](MEMORY.md) and
[HANDOFF.md](HANDOFF.md) — this file is only for what is happening now.

---

**Last updated:** 2026-09-06 by `claude`
**Phase:** 0 — scaffolding complete, nothing runs yet
**Next milestone:** transparent `NSPanel` floating on real macOS hardware

## In progress

| Task | Agent | Notes |
|---|---|---|
| _(none claimed)_ | | |

Claim a task by adding a row before you start.

## Next up

Ordered. The first item gates everything else.

1. **Verify the transparent `NSPanel` on real macOS** — `claude` / owner.
   Draggable, floats above normal windows, follows across Spaces, and clicking
   it does **not** steal keyboard focus from the frontmost app.
   *Do not delegate.* If this fails, the architecture changes, and learning
   that now is much cheaper than learning it in week six.
2. **Collapse the duplicated `AppState`** in `src-tauri/src/lib.rs` — currently
   constructed twice, once for the worker threads and once for `.manage()`.
   Should be a single `Arc`.
3. **Implement `load_active_pack`** — read `packs/<active>/manifest.json`,
   decode frames, build hitmasks via `core::hitmask`, populate `MaskSet`. The
   frontend already calls this on boot and will fail without it.
4. **Draw a default pet** — `packs/default/` has a manifest but no sprites.
   Must be original or CC0.
5. **First end-to-end webhook** — `curl` → bubble appears.

## Open questions

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

- **2026-09-06 `claude`** — Scaffolding generated: platform seam
  (`PetWindow` trait + macOS/Windows impls), `core::hitmask` with tests,
  `core::webhook`, `core::config`, behaviour engine, Tauri config, dual-platform
  CI with a grep check that fails the build if `cfg(target_os)` escapes
  `platform/`. Nothing has been compiled or run yet — the container has no
  network access, so `cargo` could not fetch dependencies.
