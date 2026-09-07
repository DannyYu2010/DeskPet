#!/usr/bin/env python3
"""Generate the default DeskPet character.

Original artwork, CC0. Drawn procedurally rather than hand-painted so that it
stays reproducible and stays out of any licensing grey area.

Everything is rendered at 4x and downsampled, which gives properly
anti-aliased alpha edges. Aliased edges are exactly what makes a desktop pet
look pasted onto the screen, so this matters more than it sounds.

    python3 tools/make_default_pack.py

Writes packs/default/sprites/*.webp and preview.png.
"""

from pathlib import Path
import math
from PIL import Image, ImageDraw, ImageFilter

SS = 4  # supersampling factor
W = H = 256
BASELINE = 236  # must match manifest anchor.baselineY

BODY = (122, 162, 214, 255)
BODY_DARK = (96, 134, 186, 255)
BELLY = (206, 226, 248, 255)
EYE = (38, 46, 62, 255)
CHEEK = (238, 156, 150, 90)

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "packs" / "default" / "sprites"


def canvas():
    img = Image.new("RGBA", (W * SS, H * SS), (0, 0, 0, 0))
    return img, ImageDraw.Draw(img)


def ellipse(d, cx, cy, rx, ry, fill):
    d.ellipse(
        [(cx - rx) * SS, (cy - ry) * SS, (cx + rx) * SS, (cy + ry) * SS],
        fill=fill,
    )


def draw_pet(squash=0.0, eye_open=1.0, lean=0.0, mouth="neutral"):
    """squash: 0-1 breathing compression. eye_open: 0 closed, 1 open."""
    img, d = canvas()

    # Body: a rounded blob that squashes vertically and widens slightly, so
    # volume reads as roughly conserved.
    ry = 62 - squash * 5
    rx = 58 + squash * 4
    cy = BASELINE - ry
    cx = 128 + lean

    ellipse(d, cx, cy, rx, ry, BODY)
    # Underside shading, clipped by drawing a lighter belly on top.
    ellipse(d, cx, cy + ry * 0.30, rx * 0.82, ry * 0.60, BODY_DARK)
    ellipse(d, cx, cy + ry * 0.34, rx * 0.62, ry * 0.46, BELLY)

    # Ears
    for sx in (-1, 1):
        ellipse(d, cx + sx * 36, cy - ry * 0.72, 16, 21, BODY)

    # Eyes. Closing scales the vertical radius toward a line rather than
    # shrinking the whole eye, which reads as a blink instead of a shrink.
    eye_y = cy - 10
    for sx in (-1, 1):
        ex = cx + sx * 20
        if eye_open > 0.12:
            ellipse(d, ex, eye_y, 7.5, 9.5 * eye_open, EYE)
            ellipse(d, ex + 2.5, eye_y - 3 * eye_open, 2.6, 2.6 * eye_open,
                    (255, 255, 255, 235))
        else:
            d.rounded_rectangle(
                [(ex - 7.5) * SS, (eye_y - 1.4) * SS, (ex + 7.5) * SS, (eye_y + 1.4) * SS],
                radius=1.4 * SS, fill=EYE,
            )

    for sx in (-1, 1):
        ellipse(d, cx + sx * 36, eye_y + 13, 9, 6, CHEEK)

    # Mouth
    my = cy + 8
    if mouth == "smile":
        d.arc([(cx - 11) * SS, (my - 8) * SS, (cx + 11) * SS, (my + 9) * SS],
              start=20, end=160, fill=EYE, width=int(2.6 * SS))
    elif mouth == "open":
        ellipse(d, cx, my + 2, 7, 8.5, EYE)
    else:
        d.arc([(cx - 8) * SS, (my - 6) * SS, (cx + 8) * SS, (my + 6) * SS],
              start=25, end=155, fill=EYE, width=int(2.4 * SS))

    return img.resize((W, H), Image.LANCZOS)


def frames():
    """Frame set. Kept small deliberately — the runtime adds procedural
    breathing and gaze on top, so packs do not need many hand-made poses."""
    return {
        "idle_00.webp": draw_pet(squash=0.0),
        "idle_01.webp": draw_pet(squash=0.5),
        "idle_02.webp": draw_pet(squash=1.0),
        "idle_03.webp": draw_pet(squash=0.5),
        "blink_00.webp": draw_pet(squash=0.4, eye_open=0.0),
        "walk_00.webp": draw_pet(squash=0.2, lean=-4),
        "walk_01.webp": draw_pet(squash=0.7, lean=0),
        "walk_02.webp": draw_pet(squash=0.2, lean=4),
        "walk_03.webp": draw_pet(squash=0.7, lean=0),
        "sleep_00.webp": draw_pet(squash=1.0, eye_open=0.0, mouth="neutral"),
        "poke_00.webp": draw_pet(squash=0.0, eye_open=1.0, mouth="open"),
        "notify_00.webp": draw_pet(squash=0.1, mouth="smile"),
    }


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    made = frames()
    for name, img in made.items():
        img.save(OUT / name, "WEBP", lossless=True, quality=100)

    preview = Image.new("RGBA", (W, H), (245, 246, 248, 255))
    preview.alpha_composite(made["idle_00.webp"])
    preview.save(OUT.parent / "preview.png")

    total = sum((OUT / n).stat().st_size for n in made)
    print(f"wrote {len(made)} frames, {total / 1024:.0f} KB total")


if __name__ == "__main__":
    main()
