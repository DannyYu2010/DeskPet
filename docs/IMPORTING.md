# Bringing your own pet

DeskPet builds a pet out of material you supply. It does not generate anything
itself, and it never has to: whatever you can photograph or produce in another
tool becomes a state your pet can be in.

There are three ways to feed it, and they mix freely in one import.

| What you give it | What you get | Cost |
|---|---|---|
| One photo | A pet that breathes, tilts and tracks the cursor | free |
| Several photos, one per pose | The above, plus real poses — sitting, lying, mouth open | free |
| Short clips | Real motion: rolling, stretching, a tail actually wagging | whatever the tool you used charged you |

The third row is the interesting one. Clips can come from anywhere — a video
you shot, or one produced by an image-to-video tool. DeskPet does not care
which, and takes no position on what you paid. That is deliberate: generating
inside the app would mean either a bill for every user or a model too large to
ship, and neither survives being given to other people.

## What the files have to be

Checked at import. A file that fails is named and skipped, never silently
degraded.

### Photos

- **PNG, JPEG or WebP.** HEIC — the iPhone default — cannot be read. Export as
  JPEG first (Photos → File → Export → Export Photo).
- **At least 256 px** on the short edge. Below that the cutout has nothing to
  work with.
- **At most 8000 px** and 25 MB.
- Orientation is read from EXIF, and you can rotate it by hand before
  importing. The pet's head should end up pointing up.

### Clips

- **MP4 or MOV, H.264.** These decode in the app's own webview, which is why
  no video library ships with DeskPet.
- **10 seconds or shorter.** A pet animation is a loop, not a film; anything
  longer is being used wrong.
- **1080p or smaller**, 60 MB or less.
- **Return to the pose you started from.** Every frame is kept, so a clip that
  ends somewhere else visibly snaps when it loops. If you are prompting a
  generator, say so explicitly — "hold for five seconds, then the camera
  returns" is the kind of instruction that produces a loopable result.

### What makes a good cutout

The matte model is good but not magic.

- One animal, filling a decent part of the frame.
- Not blurred by motion, not blown out.
- Avoid backgrounds the same colour as the animal — a white dog on a white
  sofa has no edge to find.
- Whole body if you want it to stand on the floor properly; the pet's feet are
  detected from the cutout, and a cropped-off leg becomes the foot line.

## Poses and states

Each asset is assigned a state — `idle`, `sleep`, `poke`, `notify` and so on.
The runtime picks between them (see PETPACK_SPEC.md); you decide which picture
or clip means which.

`idle` is required. Everything else is optional and falls back to `idle` when
missing, so a one-photo pet works and gets better as you add material.

## Consistency across assets

Photos taken minutes apart still differ in scale, distance and light, and a pet
that changes size when it lies down reads as a slideshow rather than an animal.
Import normalises for this:

- **Scale** is normalised across every asset so the animal is the same size in
  all of them.
- **Position** aligns on the foot baseline and the horizontal centre of mass,
  not the bounding box.
- **Colour** is matched to the first `idle` asset, which damps the flicker
  between a photo shot indoors and one shot in sunlight.

None of this rescues assets that disagree wildly. Material shot in one session,
from roughly one distance, gives a noticeably better pet than a decade of
camera roll.
