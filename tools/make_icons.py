#!/usr/bin/env python3
"""Generate DeskPet's app icon and menu-bar icon.

    python3 tools/make_icons.py
    cd apps/desktop && npx tauri icon src-tauri/icons/source.png

Two different jobs, two different rules:

* **App icon** — full colour, 1024x1024, drawn to read at 32px. macOS applies
  its own rounded-rectangle mask on recent releases, so the artwork fills the
  square and does not pre-round itself.

* **Menu bar icon** — a *template* image: pure black plus an alpha channel,
  nothing else. macOS recolours it for light and dark menu bars and for the
  highlighted state. Shipping a coloured icon here is the usual mistake; it
  looks fine until someone switches appearance and it turns into a smudge.
  Drawn as a silhouette, because a 18pt icon has no room for interior detail.
"""

from pathlib import Path
from PIL import Image, ImageChops, ImageDraw, ImageFilter

SS = 4  # supersample, then downscale for clean alpha edges

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "apps" / "desktop" / "src-tauri" / "icons"

BODY = (122, 162, 214, 255)
BODY_DARK = (96, 134, 186, 255)
BELLY = (206, 226, 248, 255)
EYE = (38, 46, 62, 255)
BACKDROP = (247, 250, 255, 255)


def ellipse(d, cx, cy, rx, ry, fill):
    d.ellipse([(cx - rx) * SS, (cy - ry) * SS, (cx + rx) * SS, (cy + ry) * SS], fill=fill)


def silhouette(size: int) -> Image.Image:
    """Head-and-ears outline, solid black with alpha. Template image."""
    img = Image.new("RGBA", (size * SS, size * SS), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    c = size / 2
    r = size * 0.30
    ear = size * 0.135
    ear_dx = size * 0.235
    ear_dy = size * 0.235

    black = (0, 0, 0, 255)
    for sx in (-1, 1):
        ellipse(d, c + sx * ear_dx, c - ear_dy, ear, ear * 1.15, black)
    ellipse(d, c, c, r * 1.12, r, black)

    # Punch the eyes back out so the silhouette still reads as a face at 18pt.
    hole = (0, 0, 0, 0)
    for sx in (-1, 1):
        ellipse(d, c + sx * size * 0.115, c - size * 0.02, size * 0.052, size * 0.062, hole)

    return img.resize((size, size), Image.LANCZOS)


def app_icon(size: int = 1024) -> Image.Image:
    """A macOS app icon.

    macOS does **not** round your icon for you — that is iOS. On macOS the
    rounded rectangle is part of the artwork, and an icon that fills the whole
    square lands in the Dock as a hard-edged tile between its correctly shaped
    neighbours.

    Apple's grid: on a 1024 canvas the tile is 824x824, centred, corner radius
    ~185, with the remaining margin left for the drop shadow. The character
    fills most of the tile — at Dock size there is no room for polite padding.
    """
    S = size * SS
    inset = round(size * 0.0977)      # (1024-824)/2 / 1024
    radius = round(size * 0.181)      # 185/1024

    box = [inset * SS, inset * SS, (size - inset) * SS, (size - inset) * SS]

    tile_mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(tile_mask).rounded_rectangle(box, radius=radius * SS, fill=255)

    # Vertical gradient, light at the top. Flat fills read as unfinished next
    # to system icons, which all carry some depth.
    grad = Image.new("RGBA", (S, S))
    gd = ImageDraw.Draw(grad)
    top, bottom = (250, 252, 255, 255), (214, 229, 249, 255)
    for y in range(S):
        t = y / max(S - 1, 1)
        gd.line(
            [(0, y), (S, y)],
            fill=tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)),
        )

    tile = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    tile.paste(grad, (0, 0), tile_mask)

    # The character: a head, not the full body. The pack art's belly shading
    # belongs to a standing blob and turns a head into a ring if reused here.
    d = ImageDraw.Draw(tile)
    c = size / 2
    cy = c + size * 0.020
    rx = size * 0.225
    ry = size * 0.235

    # No contact shadow here. A head is not standing on anything, and the
    # ellipse just reads as a grey smudge floating under the chin. The pet on
    # the desktop needs one; an icon does not.

    for sx in (-1, 1):
        ellipse(d, c + sx * size * 0.155, cy - ry * 0.76, size * 0.072, size * 0.092, BODY)
    ellipse(d, c, cy, rx, ry, BODY)

    # Muzzle: one light ellipse low on the face. It gives the mouth somewhere
    # to sit and stops the face reading as a flat disc.
    ellipse(d, c, cy + ry * 0.40, rx * 0.55, ry * 0.36, BELLY)

    eye_y = cy - size * 0.045
    for sx in (-1, 1):
        ex = c + sx * size * 0.085
        ellipse(d, ex, eye_y, size * 0.032, size * 0.041, EYE)
        ellipse(d, ex + size * 0.011, eye_y - size * 0.013,
                size * 0.012, size * 0.012, (255, 255, 255, 235))

    for sx in (-1, 1):
        ellipse(d, c + sx * size * 0.152, eye_y + size * 0.052,
                size * 0.038, size * 0.025, (238, 156, 150, 105))

    nose_y = cy + ry * 0.28
    ellipse(d, c, nose_y, size * 0.024, size * 0.018, EYE)
    d.arc(
        [(c - size * 0.040) * SS, (nose_y - size * 0.004) * SS,
         (c + size * 0.040) * SS, (nose_y + size * 0.052) * SS],
        start=25, end=155, fill=EYE, width=int(size * 0.011 * SS),
    )

    # Drop shadow under the tile, in the margin the grid reserves for it.
    shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    shadow.paste(Image.new("RGBA", (S, S), (28, 40, 62, 90)), (0, 0), tile_mask)
    shadow = shadow.filter(ImageFilter.GaussianBlur(size * 0.018 * SS))
    shadow = ImageChops.offset(shadow, 0, int(size * 0.012 * SS))

    out = Image.alpha_composite(shadow, tile)
    return out.resize((size, size), Image.LANCZOS)


def main():
    OUT.mkdir(parents=True, exist_ok=True)

    app_icon().save(OUT / "source.png")

    # 1x and 2x. Tauri picks the right one from the same file on retina, but
    # shipping the 2x asset means the menu bar never resamples upward.
    silhouette(44).save(OUT / "tray.png")
    silhouette(22).save(OUT / "tray-1x.png")

    print(f"wrote {OUT/'source.png'}, {OUT/'tray.png'}, {OUT/'tray-1x.png'}")
    print("now run: cd apps/desktop && npx tauri icon src-tauri/icons/source.png")


if __name__ == "__main__":
    main()
