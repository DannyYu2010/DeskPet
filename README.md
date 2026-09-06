# DeskPet

A desktop pet that looks like it belongs on your desktop, not pasted on top of it.

macOS 12+ and Windows 11. MIT licensed.

> Status: pre-alpha scaffolding. Nothing works yet.

## Goals

- **Looks attached to the desktop.** Contact shadows, per-pixel click-through, gaze tracking, no focus stealing.
- **Bring your own character.** Upload a few images; the asset pipeline mattes, cleans edges, aligns anchors and packs them.
- **Quiet.** Under 0.5% idle CPU and 60 MB resident. Never steals focus, hides during fullscreen apps, respects Do Not Disturb.
- **One codebase.** ~85-90% shared between macOS and Windows.

## Quick start

```bash
cd apps/desktop
npm install
npm run tauri dev
```

## Notifications

DeskPet does not read your other apps' notifications. It listens on localhost instead, so anything can push to it:

```bash
curl -X POST http://127.0.0.1:7423/notify \
     -H "Authorization: Bearer $(cat ~/.config/deskpet/token)" \
     -H "Content-Type: application/json" \
     -d '{"title":"Build passed","body":"main @ a1b2c3","source":"ci"}'
```

This is a deliberate scope cut, explained in [ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Making a pack

See [PETPACK_SPEC.md](docs/PETPACK_SPEC.md). A pack is a zip with a `manifest.json` and some WebP frames. A single static PNG is a valid pack — the runtime adds breathing and blinking on top.

Packs declare appearance; the runtime owns behaviour. That means behaviour improvements apply to every existing pack at once.

## Working on this with agents

Claude, Codex and WorkBuddy all touch this repo. Until a Memory Hub project
exists, **git is the shared memory layer**.

- Read [`docs/STATE.md`](docs/STATE.md) first — what's in progress, what's next.
- Read [`docs/HANDOFF.md`](docs/HANDOFF.md) — locked decisions and known gaps.
- Conventions in [`docs/MEMORY.md`](docs/MEMORY.md).

Update `STATE.md` in the same commit as your code.

## Contributing

One rule matters more than the rest:

> `#[cfg(target_os = ...)]` belongs **only** in `src-tauri/src/platform/`. Never in `core/`, never in TypeScript.

CI enforces this. If you need platform-specific behaviour elsewhere, add a method to the `PetWindow` trait and implement it on both sides.

Development is macOS-first, but both platforms must compile and lint clean on every PR.

## Known constraints

- Transparent windows on macOS require Tauri's `macOSPrivateApi`, which makes the app **ineligible for the Mac App Store**. We self-distribute.
- macOS distribution needs an Apple Developer account ($99/yr) for signing and notarization. Without it, users must right-click → Open on first launch.
- Reading system notifications is not supported on macOS (impossible post-SIP) and not yet implemented on Windows (needs MSIX package identity).

## Licenses

Code is MIT. Bundled packs are original work or CC0.

Model weights are downloaded on first use, not vendored. Note for contributors: **RMBG-2.0 is CC BY-NC and cannot be used here** — we use BiRefNet (MIT) for matting and FILM (Apache-2.0) for interpolation. Check the license of any model before adding it; several popular ones carry non-commercial clauses that are incompatible with this project.
