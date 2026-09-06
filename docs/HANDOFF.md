# Agent Handoff

Read this before picking up any DeskPet task. It exists so an agent can start
work without re-reading the original design conversation.

If a decision below conflicts with what you would naturally do: the decision
wins, or you raise it with the owner first. Several of these were made after
evaluating and rejecting the obvious alternative.

---

## What this is

An open-source desktop pet. A character lives on the desktop, reacts when
notifications arrive, responds to clicks, and can be replaced with artwork the
user supplies.

Repository layout, resource budget, and the full rationale are in
[ARCHITECTURE.md](ARCHITECTURE.md). The asset pack contract is in
[PETPACK_SPEC.md](PETPACK_SPEC.md).

## Locked decisions

| Decision | Rationale | Status |
|---|---|---|
| macOS 12+ and Windows 11 only | Wayland forbids self-positioning windows; GNOME refuses `wlr-layer-shell`. Cost was disproportionate. | **Do not reintroduce Linux.** |
| Tauri v2 + PixiJS | An earlier plan used `winit` + `wgpu` to dodge WebKitGTK's ~150 MB idle cost on Linux. Dropping Linux removed that constraint — WebView2 (~40 MB) and WKWebView (~30–50 MB) both fit budget. Custom renderer buys nothing, costs weeks. | Locked |
| Mac App Store is not a goal | Owner confirmed. Frees us to set `macOSPrivateApi: true`, which window transparency requires. | Locked |
| Localhost webhook for notifications | Reading other apps' notifications is impossible on macOS post-SIP and needs MSIX identity on Windows. Webhook is identical on both, needs no permissions. | Locked for v0.1.0 |
| MIT license | — | Locked |

## The one architectural rule

`#[cfg(target_os = ...)]` belongs **only** in
`apps/desktop/src-tauri/src/platform/`. Never in `core/`. Never in TypeScript.

CI greps for violations and fails the build. If you need platform-specific
behaviour elsewhere, add a method to the `PetWindow` trait and implement it on
both sides. Do not work around the check.

Development is macOS-first, but **both platforms must compile and lint clean on
every PR**. The Windows implementation exists from day one specifically so that
macOS assumptions cannot quietly bake into the architecture.

## Known gaps in the scaffold

These are deliberate placeholders, not oversights. First `cargo tauri dev` will
hit some of them.

1. **`lib.rs` constructs `AppState` twice** — once for the worker threads, once
   for `.manage()`. Collapse into one `Arc`.
2. **`load_active_pack` command is not implemented.** The frontend calls it on
   boot. Needs: read `packs/<active>/manifest.json`, decode frames, build
   hitmasks via `core::hitmask`, populate `MaskSet`.
3. **`macos.rs::is_fullscreen_app_active` returns `false`.** Left unimplemented
   on purpose — needs real-hardware verification of
   `currentSystemPresentationOptions` behaviour. Writing it blind is guessing.
4. **`macos.rs::is_dnd_active` returns `false`.** Parse
   `~/Library/DoNotDisturb/DB/Assertions.json`. Apple has moved this path
   across releases; verify on the target OS version before committing.
5. **`webhook::generate_token` uses a timestamp.** Predictable. Replace with
   `rand::rngs::OsRng` before v0.1.0.
6. **`packs/default/` has a manifest but no sprite files.** Needs original or
   CC0 artwork.

Windows equivalents of (3) and (4) are already implemented via
`SHQueryUserNotificationState`.

## Licensing landmines

Check before adding any model or asset:

- **RMBG-2.0 is CC BY-NC. Do not use it.** Use **BiRefNet (MIT)** for matting.
- Some **RIFE** forks carry non-commercial clauses. Use **FILM (Apache-2.0)**
  for frame interpolation.
- Bundled packs must be original work or CC0. Do not ship packs of copyrighted
  characters — it is the most common takedown cause for projects in this
  category.
- Model weights are never committed. Download on first use, verify by hash.

## What makes this feel good vs. cheap

Worth knowing even for backend tasks, because these are the parts most likely
to get cut under time pressure and they are the whole point of the project:

- **Contact shadow.** Generated from frame alpha, flattened ellipse, blurred,
  drawn under the baseline. Its absence is the single biggest reason a desktop
  pet reads as a sticker.
- **Anchor alignment on foot baseline + horizontal centroid**, never canvas
  centre. Bounding-box centring makes the character bob whenever a limb
  extends.
- **Edge decontamination after matting.** Matting alone leaves a halo of the
  original background colour. Unpremultiply → colour decontamination → guided
  filter → ~1 px feather.
- **Gaze tracking.** Pupils shift a few pixels toward the cursor. Trivial cost,
  disproportionate effect.
- **Boredom-driven transitions.** Random state switching is instantly
  recognisable as random. A slowly rising drive that gates transitions is
  barely more code and reads as intent.
- **Never steal focus.** macOS: `NSPanel` with `.nonactivatingPanel`. Windows:
  `WS_EX_NOACTIVATE`. A pet that eats a keystroke gets uninstalled.

## Resource budget (non-negotiable)

Idle CPU < 0.5%, resident memory < 60 MB.

The render loop **parks itself** when the brain reports a quiescent state — it
does not merely drop framerate. That is the difference between "low CPU" and
"no CPU". Unfocused caps at 10 fps. ONNX models load only during asset import
and unload immediately; the resident process carries no ML dependency.

## Next milestone

v0.1.0 ships when both platforms can: install, display a pet, drag it, receive
a webhook notification, and import a user's own images.

Immediate next step is narrow: get a transparent `NSPanel` floating on real
macOS hardware — draggable, and clicking it must not steal keyboard focus. If
that works the rest follows; if it doesn't, the architecture changes, and it is
much cheaper to learn that now.
