# Architecture

## Shape

```
┌─────────────────────────────────────────────┐
│  Frontend (TypeScript + PixiJS)             │
│  rendering · state machine · gaze · bubble  │   ~90% of behaviour
│  ZERO platform branches                     │
└──────────────────┬──────────────────────────┘
                   │ Tauri IPC
┌──────────────────┴──────────────────────────┐
│  core/  (Rust, platform-independent)        │
│  hitmask · webhook · config · pack loading  │
└──────────────────┬──────────────────────────┘
                   │ PetWindow trait
        ┌──────────┴──────────┐
   platform/macos.rs     platform/windows.rs
   NSPanel, Spaces       WS_EX_*, HWND_TOPMOST
        ~200 lines            ~200 lines
```

Roughly 85–90% of the code is shared. The platform-specific portion is small,
but it is also the part that cannot be validated by reading — only by running
on real hardware.

## The one rule

`#[cfg(target_os = ...)]` appears **only** inside `src-tauri/src/platform/`.
Never in `core/`. Never in TypeScript.

CI greps for violations on every PR. This is not stylistic: platform branches
spread like ivy once the first one escapes containment, and by month three
"cross-platform" means "two codebases that share a logo."

If you need platform-specific behaviour somewhere else, the fix is to add a
method to `PetWindow` and implement it on both sides.

## Why Tauri, not a custom renderer

An earlier draft used `winit` + `wgpu` to avoid WebKitGTK's ~150 MB idle
footprint on Linux. Dropping Linux removed that constraint: Windows 11 uses
WebView2 (~40 MB, ships with the OS) and macOS uses system WKWebView
(~30–50 MB). Both fit the budget, so the custom renderer buys nothing and costs
several weeks.

Rendering is PixiJS: WebGL sprite batching, and mesh deformation later for
part-based animation.

## Window strategy

Two windows, only one of which is resident.

**Pet window** — transparent, borderless, non-activating, always created. Sized
to the sprite's bounding box, not fullscreen; a fullscreen transparent window
makes the compositor composite the entire screen every frame.

**Settings window** — normal window, created on tray-menu click, destroyed on
close. Complex UI is much faster to build in HTML, but there is no reason to
keep that webview alive while the user is not looking at it.

## Click-through

The window is rectangular; the character is not. Without per-pixel hit testing,
the transparent corners eat clicks meant for whatever is behind them.

1. At pack load, each frame's alpha becomes a downsampled bitset (1/4 scale,
   ~2 KB for 512×512).
2. A 30 Hz thread reads the cursor position, converts to window-local logical
   pixels, and tests the mask for the frame currently displayed.
3. Click-through toggles only on change.

Polling is required, not lazy: a click-through window receives no mouse events,
so there is nothing to listen to. Calling `getImageData` per mouse move is the
obvious alternative and is far too slow.

## Notifications

Localhost HTTP on `127.0.0.1:7423`, bearer token required.

This is a deliberate scope cut. Reading other apps' notifications is
impossible on macOS post-SIP, and on Windows requires MSIX package identity.
A webhook works identically on both, needs no permissions, and lets anything
push to the pet in one line of `curl`.

Windows toast interception via `UserNotificationListener` remains possible
later — it needs a sparse package for identity — but it is explicitly not a
v0.1.0 goal.

The transport layer makes no behavioural decisions. It emits an event; the
frontend decides whether to react, queue, or drop.

## Resource budget

Non-negotiable, because a resident app that makes the machine feel slow gets
uninstalled regardless of how good it looks.

| Metric | Target |
|---|---|
| Idle CPU | < 0.5% |
| Resident memory | < 60 MB |
| Idle GPU | negligible |

How it is met:

- No `requestAnimationFrame` loop while nothing moves. Redraw on frame change
  only. A fully static pet runs zero render work.
- Unfocused drops to 10 fps.
- Fullscreen app detected → window hidden, loops idle.
- ONNX models load on demand during asset import and unload immediately. The
  resident process carries no ML dependency.

## Asset pipeline

Runs only during import, in-process via the `ort` crate. No Python sidecar —
bundling Python across two platforms costs ~200 MB per target and a great deal
of packaging pain.

1. **Matting** — BiRefNet (MIT). Note: RMBG-2.0 is CC BY-NC and cannot be used
   here.
2. **Edge cleanup** — unpremultiply, colour decontamination, guided-filter
   alpha refinement, ~1 px feather. This step is the difference between "cut
   out" and "belongs there"; matting alone leaves a halo of the original
   background colour.
3. **Anchor alignment** — baseline + horizontal centroid, per `PETPACK_SPEC.md`.
4. **Interpolation** (later) — FILM (Apache-2.0) to expand 3 key poses into a
   12–24 frame loop. RIFE forks vary in licence; check before adopting.
5. **Part segmentation** (later) — SAM 2 to split head/body/limbs for mesh
   deformation, enabling breathing and tail motion from a single still.
6. **Packing** — manifest + WebP atlases + precomputed masks.

Model weights are never committed. They download on first use and are verified
by hash.

## Deliberate non-goals for v0.1.0

- Linux
- Reading OS notifications on macOS
- Mac App Store distribution (blocked by `macOSPrivateApi`, required for
  window transparency)
- LLM conversation
- Part-based mesh animation
