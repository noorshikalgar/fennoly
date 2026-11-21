"""Generates Fennoly's app icons from the sprout mark.

The mark is drawn on a 64-unit grid (same paths as Logo.svelte and
static/icons/icon.svg) and rendered at 4x, then scaled down for smooth edges.

Run: python web/scripts/make-icons.py
"""

from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path(__file__).resolve().parent.parent / "static"
ICONS = OUT / "icons"
ICONS.mkdir(parents=True, exist_ok=True)

TILE = "#0F6E56"
LEAF_BACK = "#9FE1CB"
LEAF_FRONT = "#E1F5EE"

# Each leaf is a closed shape made of two cubic curves: (start, cp1, cp2, end).
LEFT_LEAF = [((32, 50), (23, 45), (18, 33), (23, 17)), ((23, 17), (34, 21), (39, 34), (32, 50))]
RIGHT_LEAF = [((33, 50), (42, 46), (48, 36), (45, 23)), ((45, 23), (36, 26), (31, 36), (33, 50))]
SVG = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
  <rect width="64" height="64" rx="15" fill="#0F6E56"/>
  <path d="M32.5 46v8" stroke="#E1F5EE" stroke-width="3" stroke-linecap="round"/>
  <path d="M32 50C23 45 18 33 23 17c11 4 16 17 9 33z" fill="#9FE1CB"/>
  <path d="M33 50c9-4 15-14 12-27-9 3-14 13-12 27z" fill="#E1F5EE"/>
</svg>
"""


def bezier(p0, p1, p2, p3, steps=48):
    for i in range(steps + 1):
        t = i / steps
        a, b, c, d = (1 - t) ** 3, 3 * (1 - t) ** 2 * t, 3 * (1 - t) * t**2, t**3
        yield (a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0], a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1])


def shape(curves, scale, offset):
    pts = []
    for curve in curves:
        pts.extend(bezier(*curve))
    return [(offset + x * scale, offset + y * scale) for x, y in pts]


def render(size: int, *, rounded: bool, mark_scale: float = 1.0) -> Image.Image:
    """Draws the icon. `mark_scale` < 1 shrinks the sprout (maskable safe zone)."""
    s = size * 4
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    if rounded:
        draw.rounded_rectangle((0, 0, s - 1, s - 1), radius=s * 15 / 64, fill=TILE)
    else:
        draw.rectangle((0, 0, s, s), fill=TILE)
    unit = s / 64 * mark_scale
    offset = (s - 64 * unit) / 2
    w = 3 * unit
    x = offset + 32.5 * unit
    draw.line((x, offset + 46 * unit, x, offset + 54 * unit), fill=LEAF_FRONT, width=round(w))
    draw.ellipse((x - w / 2, offset + 54 * unit - w / 2, x + w / 2, offset + 54 * unit + w / 2), fill=LEAF_FRONT)
    draw.polygon(shape(LEFT_LEAF, unit, offset), fill=LEAF_BACK)
    draw.polygon(shape(RIGHT_LEAF, unit, offset), fill=LEAF_FRONT)
    return img.resize((size, size), Image.LANCZOS)


for px in (192, 512):
    render(px, rounded=True).save(ICONS / f"icon-{px}.png")
# Maskable icons are cropped to a circle or squircle by Android: full-bleed
# background, sprout inside the 80% safe zone.
render(512, rounded=False, mark_scale=0.78).save(ICONS / "maskable-512.png")
# iOS adds its own rounded corners and dislikes transparency.
render(180, rounded=False).save(ICONS / "apple-touch-icon.png")
render(64, rounded=True).save(OUT / "favicon.ico", sizes=[(16, 16), (32, 32), (48, 48), (64, 64)])
(ICONS / "icon.svg").write_text(SVG, encoding="utf-8")
print("icons written to", ICONS)
