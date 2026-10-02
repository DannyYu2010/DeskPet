# DeskPet asset specification

**Audience:** whoever or whatever produces the material — a person with a
camera, an image model, a video model, or an agent driving one. Follow this and
the import succeeds. Deviate and the importer will say which rule was broken.

**Status:** v0.1, unvalidated. The numbers here are considered guesses until
real batches have been through the importer. Expect them to tighten. Every
change to this file should be caused by something that actually failed.

---

## 1. What one delivery contains

One delivery describes **one character**. Not a set of animals, not variations
on a theme — the same creature in different poses.

```
mypet/
├── idle/          required
├── sleep/         optional
├── lie_down/      optional
├── wake/          optional
├── poke/          optional
├── notify/        optional
└── REFERENCE.png  required
```

Each state folder holds **either** one still image **or** one clip's frames.
Never both.

| State | What it means | When the pet shows it |
|---|---|---|
| `idle` | Standing or sitting, alert, neutral | Most of the time. **Required.** |
| `sleep` | Lying down, eyes closed, settled | After a long stretch of no interaction |
| `lie_down` | A one-shot move from the idle pose into the sleep pose | Immediately before `sleep` |
| `wake` | A one-shot move from the sleep pose back to idle | When the pointer reaches a sleeping pet |
| `poke` | Reacting to being clicked — a look up, a small start | When the user clicks the pet |
| `notify` | Noticing something — head turned, ears up | When a notification arrives |
| custom | A named one-shot action such as running or jumping | On the configured single or double click |

Missing states fall back to `idle`. A delivery with only `idle` is valid and
produces a working pet. Add `sleep` first, then its two transition clips, because
the contrast between awake and asleep and the continuity between them make the
pet feel like it has a day.

The settings importer accepts one custom action for single click and one for
double click. If both are configured, single click responds after a 250 ms wait
so DeskPet can tell the two gestures apart.

`REFERENCE.png` is the canonical picture of the character. Everything else must
be recognisably the same animal as this file. It is not imported; it exists so
a human or a model can check identity, and so later batches can extend an
existing pet.

---

## 2. Hard constraints

These are checked. A file that fails is named and rejected.

### Every image

| Rule | Value | Why |
|---|---|---|
| Format | PNG, JPEG or WebP | HEIC cannot be decoded |
| Short edge | ≥ 512 px | Below this the cutout has nothing to work with |
| Long edge | ≤ 4096 px | Larger is wasted; it is downscaled to a 256 px canvas |
| File size | ≤ 25 MB | |
| Colour | sRGB, 8 bit | Wide-gamut files shift colour on import |
| **Alpha** | **Strongly preferred** | See §3 |

### Every clip

| Rule | Value | Why |
|---|---|---|
| Format | MP4 or MOV, H.264 | Decoded by the app's webview; no video library ships with DeskPet |
| Duration | 1.5–10 s | Under 1.5 s reads as a twitch; over 10 s is a film, not a loop |
| Frame rate | 12–30 fps | Sampled down to 12 |
| Resolution | ≤ 1920 px long edge | |
| File size | ≤ 60 MB | |
| Audio | None | Silently discarded — do not spend generation budget on it |

Clips may also be delivered as **numbered PNG frames** (`0001.png`, `0002.png`,
…), which is better than a clip if your tool can emit alpha per frame.

---

## 3. Alpha, and why it matters more than anything else here

**If your tool can output a transparent background, do that.** The importer
detects existing alpha and skips its own cutout step entirely.

This is not a small optimisation. Cutting a subject out of a photograph is a
model making an educated guess about edges; a generator that produced the image
already *knows* where the subject is. Delivered alpha is exact, and it is the
single biggest quality difference available in this whole pipeline.

If alpha is impossible, deliver on a **flat, evenly lit background in a colour
that does not appear on the animal**. Mid-grey works for most subjects; do not
use white for a white animal, and avoid gradients, patterns and shadows falling
on the backdrop.

Never deliver a subject cut out onto white and call it done — white pixels are
not transparency, and the pet will wear a white box.

---

## 4. Framing — the rules that actually decide whether it works

This section causes more rejected batches than the format table does.

### 4.1 The camera does not move between states

Same distance, same height, same lens, same angle. Every state is the same
animal photographed from the same place, doing something different.

The importer normalises scale, but it can only correct so much: it matches the
opaque *area* between states, which is invariant enough across poses to work
and not precise enough to rescue a close-up next to a wide shot.

### 4.2 The subject fills 60–80% of the frame height

Not more — cropped ears and feet break the pose. Not less — resolution is
wasted on background that gets discarded.

### 4.3 The whole animal is in frame, feet included

The pet's floor line is measured from the lowest opaque pixel. A cropped-off
leg becomes the foot line and the pet sinks into the desktop.

### 4.4 Side or three-quarter view, not straight down

The pet stands on the user's desktop. A top-down photograph of an animal lying
on a floor cannot be made to stand up, no matter how good the cutout is.

### 4.5 One animal, no props, no hands

No leash, no toy, no arm holding it, no other pet. Anything touching the
subject is cut out with it.

### 4.6 Lighting is consistent and neutral

Same key light direction across states. Avoid hard shadows on the animal, harsh
backlight, and colour casts. The importer nudges colour toward the `idle` frame
to damp flicker, but that is a correction, not a fix.

---

## 5. Rules for clips specifically

### 5.1 A clip must end where it began

Every frame is kept and played in a loop. If the last frame differs from the
first, the pet visibly snaps on every repeat.

When prompting a generator, say so in the prompt. Wording that works:

> "…hold the pose, then return to exactly the starting position by the end of
> the clip. The first and last frames should be identical."

### 5.2 The animal stays in frame and roughly in place

No walking out of shot, no camera pans, no cuts, no zoom. The pet is anchored
to its own position on the desktop; a clip that drifts makes it slide.

### 5.3 Motion is small

A pet animation is breathing, an ear flick, a stretch, a roll. Whole-body
leaps and fast action look wrong at 256 px in the corner of a screen and
compress badly.

---

## 6. Prompt templates

Paste and adapt. `<ANIMAL>` is your subject; keep the description identical
across all four so the character stays the same.

**Reference / idle**

> A full-body <ANIMAL>, sitting upright and facing slightly to the left,
> three-quarter view, whole body including all four feet visible, filling about
> 70% of the frame height. Even soft studio lighting from the front-left, no
> harsh shadows. Plain flat mid-grey background, no gradient, no props, no
> hands, nothing touching the animal. Transparent background if supported.
> Sharp focus, sRGB.

**sleep**

> The same <ANIMAL> as the reference image, now lying down curled on its side
> with eyes closed, calm. Identical camera distance, height and angle to the
> reference. Same lighting, same background. Whole body in frame.

**poke**

> The same <ANIMAL> as the reference image, head turned toward the camera with
> a small startled expression, ears up, as if just prodded. Identical camera
> distance, height and angle. Same lighting, same background.

**notify**

> The same <ANIMAL> as the reference image, head turned sharply to one side,
> ears forward, alert, as if it just heard something. Identical camera
> distance, height and angle. Same lighting, same background.

**Clip (any state)**

> Starting from this exact image, the <ANIMAL> [describe a small motion:
> breathes slowly / flicks one ear / stretches and settles]. The camera does
> not move. The animal stays in the same position in frame. Hold, then return
> to exactly the starting pose — the first and last frames must be identical.
> 4 seconds. No cuts, no zoom, no background change.

---

## 7. Acceptance checklist

Run through this before handing a batch over. Every "no" is a round trip.

- [ ] One character, recognisably the same in every file
- [ ] `idle` present
- [ ] `REFERENCE.png` present
- [ ] Every file within the format, size and duration limits in §2
- [ ] Alpha delivered, **or** a flat contrasting background
- [ ] Same camera distance, height and angle in every state
- [ ] Whole animal in frame, feet included, filling 60–80% of the height
- [ ] Side or three-quarter view, not top-down
- [ ] Nothing touching the animal, no other subjects
- [ ] Clips return to their starting pose
- [ ] Clips have no camera motion, cuts or drift

---

## 8. What the importer does with it

Understanding this makes it obvious why the rules above exist.

1. **Decode.** EXIF orientation applied; you can rotate by hand in the UI.
2. **Cut out.** Skipped when alpha is already present. Otherwise BiRefNet_lite
   runs locally, one session for the whole batch.
3. **Measure.** Foot baseline = lowest opaque row. Horizontal centre = alpha-
   weighted centre of mass, not the bounding box, so an outstretched leg does
   not shift the animal sideways.
4. **Normalise scale** across every asset using the square root of opaque
   area — pose-invariant in a way height is not. A lying animal and a standing
   one come out the same size.
5. **Place** every frame with its feet on the floor line, centred on the
   centroid.
6. **Match colour** toward the `idle` frame at 60% strength.
7. **Build states.** A single still gets procedural breathing added. A clip is
   used as delivered.
8. **Write the pack** and make it the active pet.

---

## 9. Change log

Each revision should name the batch that caused it.

| Version | Date | Change | Caused by |
|---|---|---|---|
| 0.1 | 2026-09-07 | First draft, unvalidated | — |
