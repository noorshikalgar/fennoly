"""Generates placeholder app icons (a proper icon comes after MVP 1).

Run: python web/scripts/make-icons.py
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

OUT = Path(__file__).resolve().parent.parent / "static" / "icons"
OUT.mkdir(parents=True, exist_ok=True)

TOP = (99, 102, 241)  # indigo-500
BOTTOM = (79, 70, 229)  # indigo-600


def gradient(size: int) -> Image.Image:
    img = Image.new("RGB", (size, size))
    px = img.load()
    for y in range(size):
        t = y / (size - 1)
        c = tuple(round(a + (b - a) * t) for a, b in zip(TOP, BOTTOM))
        for x in range(size):
            px[x, y] = c
    return img


def letter(img: Image.Image, scale: float) -> None:
    size = img.width
    draw = ImageDraw.Draw(img)
    font = None
    for name in ("segoeuib.ttf", "arialbd.ttf", "DejaVuSans-Bold.ttf"):
        try:
            font = ImageFont.truetype(name, int(size * scale))
            break
        except OSError:
            continue
    font = font or ImageFont.load_default()
    box = draw.textbbox((0, 0), "F", font=font)
    w, h = box[2] - box[0], box[3] - box[1]
    draw.text(((size - w) / 2 - box[0], (size - h) / 2 - box[1]), "F", font=font, fill="white")


def rounded(size: int) -> Image.Image:
    img = gradient(size)
    letter(img, 0.56)
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, size - 1, size - 1), radius=size // 4.5, fill=255)
    out = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    out.paste(img, (0, 0), mask)
    return out


for s in (192, 512):
    rounded(s).save(OUT / f"icon-{s}.png")

maskable = gradient(512)
letter(maskable, 0.42)  # keep the letter inside the safe zone
maskable.save(OUT / "maskable-512.png")

(OUT / "icon.svg").write_text(
    """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
  <defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#6366f1"/><stop offset="1" stop-color="#4f46e5"/>
  </linearGradient></defs>
  <rect width="64" height="64" rx="14" fill="url(#g)"/>
  <text x="32" y="45" font-family="Segoe UI, Arial, sans-serif" font-size="38" font-weight="700"
        text-anchor="middle" fill="#fff">F</text>
</svg>
""",
    encoding="utf-8",
)
print("icons written to", OUT)
