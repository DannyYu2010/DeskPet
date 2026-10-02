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

## PixiJS and the CSP

PixiJS generates its shader and uniform sync functions with `new Function`.
Our CSP forbids `unsafe-eval`, so `Application.init()` rejects with *"Current
environment does not allow unsafe-eval"* and the pet window renders nothing —
which, on a transparent borderless window, is indistinguishable from the window
failing to appear at all.

The fix is the side-effect import at the top of `apps/desktop/src/main.ts`:

```ts
import "pixi.js/unsafe-eval";   // before any renderer is constructed
```

**Do not instead add `'unsafe-eval'` to the CSP.** Packs are user-supplied
content; this app has no business being able to eval strings.

## One CSP, in tauri.conf.json

Do not add a `<meta http-equiv="Content-Security-Policy">` tag to any HTML file
in this app. The scaffold shipped with one, and it cost several debugging
sessions: it declared `default-src 'self'` with no `style-src`, which blocks
inline `<style>`. The pet window kept the user-agent default 8px body margin
and grew scrollbars, and — this is the part that made it expensive — **nothing
errored**. The styles simply never applied, so every CSS edit looked like it
had no effect.

When a meta CSP and the configured CSP both apply, the browser enforces the
intersection. The stricter one wins, and `tauri.conf.json` quietly stops
describing reality. There is one CSP and it lives in `tauri.conf.json`.

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

## The matte model

Locked 2026-09-07, after the owner confirmed DeskPet is meant to be published
rather than kept personal. One backend on every platform, so the same photo
produces the same cutout everywhere — two backends means "it looks different on
my other machine", which is close to undiagnosable from a bug report.

| | |
|---|---|
| Model | `onnx-community/BiRefNet_lite-ONNX`, `onnx/model.onnx` |
| Revision | `de15b22ba131738a16dff04aab8bdf8dc32e3ac1` (pin it; `main` is mutable) |
| SHA-256 | `5600024376f572a557870a5eb0afb1e5961636bef4e1e22132025467d0f03333` |
| Size | 224 MB |
| Licence | MIT |
| Input | 1024x1024, bilinear, ImageNet mean/std, NCHW float32 |
| Output | logits — apply sigmoid to get alpha |

**fp32, not fp16.** The fp16 build is half the download (114 MB) and the
obvious saving, but ONNX Runtime's CPU execution provider has no native fp16
path and converts per operator, which is slower than fp32. fp16 only pays off
behind CoreML or DirectML. Revisit when execution-provider selection exists;
until then this trades correctness for bytes.

Rejected: full BiRefNet (940 MB — an unacceptable first-run download for a
desktop toy), RMBG-1.4 and 2.0 (CC BY-NC).

## Where the model runs

**A separate binary, `crates/assetpipe`, spawned for one import and then
gone.** Not linked into the app.

This is the resource budget being taken literally: the resident process must
carry no ML dependency, and "we load the model and then drop it" is a promise
about an allocator's behaviour that nobody can verify from the outside. A
process that exits releases everything, provably. It also keeps a crash in a
560 MB inference run from taking the pet down with it, and keeps `ort` out of
the app's dependency tree entirely.

The cost is a process boundary: images and masks cross it as files, and the app
has to handle the binary being missing.

## Generated actions

Locked 2026-09-07. The owner's goal is a pet that does things the photo does
not contain — sticks its tongue out, rolls over, scratches. No amount of
deforming a cutout produces geometry that is not in the picture, so those come
from an image-to-video model at import time.

**The pack format already fits this.** A `states` entry is a name and a list of
frames; a generated clip, matted and aligned, is exactly that. Generated
actions are ordinary pack states — no new format, no special runtime path.

Shape of it:

1. The cutout is sent as the **first frame**, with a prompt per action. Seeding
   from the cutout is what keeps the animal recognisably the same one; a clip
   generated from the prompt alone drifts into a different dog.
2. Each returned clip is matted frame by frame through the existing pipeline
   and aligned on the anchor, so a generated state sits on the floor the same
   way a drawn one does.
3. The 2.5D runtime layer stays. Between actions the pet still breathes, tilts
   and tracks the cursor — otherwise it is a video player that shows a still
   image most of the time.

### Revised 2026-09-07: the app is not the generator

The owner has to be able to publish this, and generating per user means either
a bill per import or a multi-gigabyte model in the download. Neither survives
distribution, so **DeskPet accepts assets rather than producing them**.

Users bring photos and short clips; the pipeline mattes, aligns and packs them.
A clip made with a paid tool and a clip shot on a phone are the same input.
This costs us nothing per user, has no privacy story to explain, and puts no
ceiling on quality — someone willing to spend on generation gets a
correspondingly better pet, and everyone else gets a good one for free.

In-app generation stays possible as an opt-in with the user's own API key. It
is a convenience, not the product. Requirements for what can be imported are in
`docs/IMPORTING.md`.

**Provider (only for the opt-in path): fal.ai**, behind a `VideoProvider` interface. One account reaches
several models, it is pay-per-request rather than a subscription, and swapping
model is a string. The interface exists so Replicate or a direct vendor API can
be added without touching the pipeline.

**Bring your own key.** Stored in the app's config TOML. That is a plaintext
secret in the user's config directory — acceptable for a key the user created
and can revoke, and the alternative (us holding keys and billing) is a business
decision, not a technical one.

**Photos leave the machine on this path.** The owner accepted that for
generation specifically, on condition that the user is told plainly before it
happens. Matting stays local; only generation uploads.

### Decoding the clips

Providers return mp4. Decoding H.264 in Rust means either linking ffmpeg
(LGPL, tens of megabytes, a packaging problem on both platforms) or a patent-
encumbered decoder binding. Neither is worth it, because **the webview already
has a decoder**: the settings window loads the clip in a `<video>` element,
seeks frame by frame onto a canvas, and hands the frames back. WKWebView and
WebView2 both decode H.264, so this costs nothing and works on both platforms.

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
