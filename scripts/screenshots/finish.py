"""Turns capture.mjs's PNGs into the WebP images in docs/screenshots.

    python scripts/screenshots/finish.py <png-dir> docs/screenshots

Desktop shots are scaled to 1600 px wide; the three phone shots are put side
by side with rounded corners on a transparent background.
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

src, dst = Path(sys.argv[1]), Path(sys.argv[2])
dst.mkdir(parents=True, exist_ok=True)

PHONES = ["phone-home", "phone-photos-hindi", "phone-family-marathi"]


def rounded(img, radius):
    mask = Image.new("L", img.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, img.width - 1, img.height - 1], radius=radius, fill=255)
    out = img.convert("RGBA")
    out.putalpha(mask)
    return out


for png in sorted(src.glob("*.png")):
    if png.stem in PHONES:
        continue
    img = Image.open(png).convert("RGB")
    img = img.resize((1600, round(img.height * 1600 / img.width)), Image.LANCZOS)
    img.save(dst / f"{png.stem}.webp", "WEBP", quality=82, method=6)
    print(png.stem, (dst / f"{png.stem}.webp").stat().st_size // 1024, "KB")

# Three phones in a row, each with a soft shadow.
w, gap, pad, radius = 400, 56, 48, 44
phones = []
for name in PHONES:
    img = Image.open(src / f"{name}.png").convert("RGB")
    img = img.resize((w, round(img.height * w / img.width)), Image.LANCZOS)
    phones.append(rounded(img, radius))
h = phones[0].height
canvas = Image.new("RGBA", (pad * 2 + w * 3 + gap * 2, pad * 2 + h), (0, 0, 0, 0))
for i, phone in enumerate(phones):
    x, y = pad + i * (w + gap), pad
    shadow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle([x, y + 10, x + w, y + h + 10], radius=radius, fill=(0, 0, 0, 70))
    canvas = Image.alpha_composite(canvas, shadow.filter(ImageFilter.GaussianBlur(18)))
    canvas.alpha_composite(phone, (x, y))
canvas.save(dst / "phones.webp", "WEBP", quality=85, method=6)
print("phones", (dst / "phones.webp").stat().st_size // 1024, "KB")
