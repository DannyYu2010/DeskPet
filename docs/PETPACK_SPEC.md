# `.pet` Pack Format — v0 (draft)

A pack is everything DeskPet needs to display and animate one character. It is
the project's public contract: someone should be able to publish a pack without
reading a line of Rust or TypeScript.

Treat this file as frozen once v0.1.0 ships. Additive changes bump the minor
version; anything that breaks an existing pack bumps `format` and needs a
migration path.

## Container

A zip archive with the extension `.pet`.

```
mypet.pet
├── manifest.json      required
├── sprites/           WebP frames or atlases
│   ├── idle_00.webp
│   └── ...
├── masks/             optional, precomputed hitmasks
├── sounds/            optional, OGG
├── LICENSE            required
└── preview.png        512x512, shown in the pack picker
```

If `masks/` is absent the app derives hitmasks from sprite alpha at load time
(a few ms per frame). Shipping them precomputed is an optimization, not a
requirement.

## manifest.json

```json
{
  "format": 0,
  "id": "com.example.shiba",
  "name": "Shiba",
  "author": "Someone",
  "license": "CC-BY-4.0",
  "canvas": { "width": 256, "height": 256 },

  "anchor": { "baselineY": 240, "centroidX": 128 },

  "states": {
    "idle": {
      "frames": ["idle_00.webp", "idle_01.webp", "idle_02.webp"],
      "fps": 8,
      "loop": true
    },
    "walk":   { "frames": ["walk_00.webp", "walk_01.webp"], "fps": 12, "loop": true },
    "sleep":  { "frames": ["sleep_00.webp"], "fps": 1, "loop": true },
    "notify": { "frames": ["notify_00.webp"], "fps": 6, "loop": false, "next": "idle" },
    "poke":   { "frames": ["poke_00.webp"], "fps": 10, "loop": false, "next": "idle" }
  },

  "transitions": {
    "idle":  [{ "to": "walk", "weight": 3 }, { "to": "sleep", "weight": 1, "minBoredom": 0.8 }],
    "walk":  [{ "to": "idle", "weight": 1 }],
    "sleep": [{ "to": "idle", "weight": 1, "maxBoredom": 0.2 }]
  },

  "gaze": {
    "enabled": true,
    "eyes": [
      { "x": 108, "y": 96, "radius": 4 },
      { "x": 148, "y": 96, "radius": 4 }
    ],
    "maxOffset": 3
  },

  "shadow": {
    "enabled": true,
    "opacity": 0.2,
    "widthRatio": 0.7,
    "blur": 8
  },

  "sourceMap": {
    "gmail":  "notify",
    "ci":     "notify",
    "default": "notify"
  }
}
```

### Required states

Only `idle` is required. Any state referenced by `transitions` or `sourceMap`
must exist. A pack with a single static PNG under `idle` is valid — the runtime
adds procedural breathing and blinking on top, which is enough to stop it
reading as a sticker.

### Anchor

`baselineY` is the character's foot line in canvas coordinates; `centroidX` is
the horizontal centre of mass of the opaque pixels.

Frames are aligned by these two values, **not** by canvas centre. This is the
single most common source of jitter in hand-assembled packs: bounding-box
centring makes the character bob whenever a limb extends.

The asset pipeline computes both automatically on import.

### Gaze

`eyes` positions let the runtime shift pupils toward the cursor by at most
`maxOffset` pixels. Cheap to render and disproportionately effective at making
the character read as alive. Packs without eye data set `enabled: false`.

### Shadow

The contact shadow is generated at runtime from frame alpha — a flattened
ellipse, blurred, drawn under the baseline. Packs tune it rather than ship it,
so it stays correct when the character jumps.

Omitting this is the main reason a desktop pet looks pasted onto the screen
rather than standing on it.

## Behaviour model

The runtime keeps a **boredom** value in `[0, 1]`. It rises while nothing
happens and resets on interaction. Transitions may gate on `minBoredom` /
`maxBoredom`, which is what makes the character look like it has intent rather
than like it is drawing from a hat.

The runtime — not the pack — owns:

- boredom accumulation
- gaze tracking
- procedural breathing and blinking
- edge-of-screen walking and physics
- notification queueing and do-not-disturb suppression

Packs declare appearance and preferences. They do not script behaviour. This
keeps packs small and lets behaviour improve for every existing pack at once.

## Licensing

`LICENSE` is required and must permit redistribution. Packs bundled in this
repository must be original work or CC0.

Do not publish packs of copyrighted characters. It is the most common reason
projects in this category get taken down.
