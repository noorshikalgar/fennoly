"""Fills a FRESH Fennoly server with a made-up family for screenshots.

Everything here is invented: the Kulkarni family, their documents (simple
generated PDFs) and their "photos" (drawn landscapes). Nothing is real.

    python scripts/screenshots/seed_demo.py http://127.0.0.1:8695

Needs Pillow (pip install pillow). Prints a small JSON summary (ids, link
token) that capture.mjs reads.
"""

import base64
import io
import json
import math
import random
import sys
import time
import urllib.error
import urllib.request
from http.cookiejar import CookieJar

from PIL import Image, ImageDraw, ImageFilter, ImageFont

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8695"
PASSWORD = "demo-password-123"


class Client:
    def __init__(self):
        self.jar = CookieJar()
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.jar))

    def call(self, method, path, body=None, headers=None, raw=None):
        data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
        h = {"content-type": "application/json"} if body is not None else {}
        h.update(headers or {})
        req = urllib.request.Request(BASE + "/api" + path, data=data, method=method, headers=h)
        try:
            with self.opener.open(req) as res:
                text = res.read().decode() or "null"
                return json.loads(text) if res.headers.get_content_type() == "application/json" else res.headers
        except urllib.error.HTTPError as e:
            raise SystemExit(f"{method} {path}: {e.code} {e.read().decode()[:300]}")

    def login(self, username):
        return self.call("POST", "/auth/login", {"username": username, "password": PASSWORD})

    def folder(self, parent, name):
        return self.call("POST", f"/nodes/{parent}/folders", {"name": name})["id"]

    def upload(self, parent, name, data):
        meta = f"filename {base64.b64encode(name.encode()).decode()},parentId {base64.b64encode(parent.encode()).decode()}"
        headers = self.call(
            "POST",
            "/uploads",
            headers={
                "Tus-Resumable": "1.0.0",
                "Upload-Length": str(len(data)),
                "Upload-Metadata": meta,
                "Content-Type": "application/offset+octet-stream",
            },
            raw=data,
        )
        return headers["Fennoly-Node-Id"]


# ---- fake content --------------------------------------------------------

def font(size):
    try:
        return ImageFont.load_default(size=size)
    except TypeError:  # very old Pillow
        return ImageFont.load_default()


def document_pdf(title, lines=14, accent=(15, 110, 86)):
    """A one-page 'document': heading, a few grey text lines, a table."""
    w, h = 1240, 1754
    img = Image.new("RGB", (w, h), "white")
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, w, 18], fill=accent)
    d.text((100, 120), title, fill=(30, 30, 30), font=font(56))
    d.text((100, 200), "Demo document generated for screenshots", fill=(130, 130, 130), font=font(28))
    rnd = random.Random(title)
    y = 300
    for _ in range(lines):
        d.rounded_rectangle([100, y, 100 + rnd.randint(500, 1040), y + 22], radius=8, fill=(225, 228, 226))
        y += 52
    y += 40
    for r in range(5):
        for c in range(3):
            x0 = 100 + c * 347
            d.rectangle([x0, y, x0 + 330, y + 60], outline=(210, 214, 212), width=2)
        y += 60
    out = io.BytesIO()
    img.save(out, "PDF", resolution=150)
    return out.getvalue()


PALETTES = {
    "sunset": [(255, 196, 130), (244, 128, 98), (120, 66, 120)],
    "morning": [(176, 220, 255), (255, 236, 200), (120, 180, 160)],
    "sea": [(120, 200, 230), (190, 235, 245), (30, 120, 160)],
    "hills": [(200, 230, 255), (240, 245, 250), (70, 130, 90)],
    "dusk": [(60, 70, 140), (240, 150, 140), (40, 40, 80)],
    "gold": [(255, 225, 150), (255, 190, 120), (150, 90, 60)],
}


def photo(seed, palette, big=False):
    """A soft drawn landscape: sky gradient, sun, layered hills, water, with
    a little film grain. `big` makes a 4000 px high-quality file, like a
    phone camera's, so the compress tool has something to save."""
    rnd = random.Random(seed)
    top, horizon, ground = PALETTES[palette]
    w, h = 1600, 1067
    column = Image.new("RGB", (1, h))
    for y in range(h):
        t = min(1, y / h * 1.6)
        column.putpixel((0, y), tuple(int(top[i] + (horizon[i] - top[i]) * t) for i in range(3)))
    img = column.resize((w, h))
    d = ImageDraw.Draw(img)
    sx, sy, sr = rnd.randint(300, 1300), rnd.randint(220, 420), rnd.randint(70, 120)
    d.ellipse([sx - sr, sy - sr, sx + sr, sy + sr], fill=(255, 245, 220))
    for layer in range(3):
        base = 600 + layer * 120
        shade = tuple(max(0, int(ground[i] * (0.65 + layer * 0.18))) for i in range(3))
        p1, p2 = rnd.random() * 6, rnd.random() * 6
        f1, f2 = 1 / (260 + layer * 90), 1 / (110 + layer * 40)
        pts = [(0, h)]
        for x in range(0, w + 10, 10):
            pts.append((x, base - 80 * math.sin(x * f1 + p1) - 28 * math.sin(x * f2 + p2)))
        pts.append((w, h))
        d.polygon(pts, fill=shade)
    if palette in ("sea", "sunset", "dusk"):
        d.rectangle([0, 900, w, h], fill=tuple(int(c * 0.8) for c in horizon))
    img = img.filter(ImageFilter.GaussianBlur(1.2))
    if big:
        img = img.resize((4000, 2667), Image.LANCZOS)
    grain = Image.effect_noise(img.size, 40).convert("RGB")
    img = Image.blend(img, grain, 0.05)
    out = io.BytesIO()
    img.save(out, "JPEG", quality=97 if big else 84)
    return out.getvalue()


def text_file(content):
    return content.encode()


# ---- the family -----------------------------------------------------------

def main():
    asha = Client()
    me = asha.call("POST", "/setup", {"username": "asha", "display_name": "Asha Kulkarni", "password": PASSWORD, "locale": "en"})
    home, family = me["home_id"], me["family_id"]
    for username, name in [("rohan", "Rohan Kulkarni"), ("meera", "Meera Kulkarni"), ("aaji", "Aaji")]:
        asha.call("POST", "/admin/users", {"username": username, "display_name": name, "password": PASSWORD, "locale": "en"})
    users = {u["username"]: u for u in asha.call("GET", "/users")}
    for username, gb in [("rohan", 50), ("meera", 10), ("aaji", 5)]:
        asha.call("PATCH", f"/admin/users/{users[username]['id']}", {"quota_bytes": gb * 1024**3})

    # Asha's own files.
    tax = asha.folder(home, "Tax 2025-26")
    for name in ["Form 16.pdf", "Rent receipts.pdf", "Investment proofs.pdf", "ITR acknowledgement.pdf"]:
        asha.upload(tax, name, document_pdf(name[:-4]))
    insurance = asha.folder(home, "Insurance")
    for name in ["Health policy 2026.pdf", "Car insurance.pdf", "Term life policy.pdf"]:
        asha.upload(insurance, name, document_pdf(name[:-4], accent=(40, 90, 170)))
    house = asha.folder(home, "House")
    for name in ["Electricity bill October.pdf", "Society maintenance.pdf", "Home loan statement.pdf"]:
        asha.upload(house, name, document_pdf(name[:-4], accent=(170, 90, 40)))
    school = asha.folder(home, "Meera school")
    for name in ["Report card Term 1.pdf", "Fee receipt Term 2.pdf"]:
        asha.upload(school, name, document_pdf(name[:-4], accent=(150, 60, 140)))
    asha.upload(home, "Vaccination record.pdf", document_pdf("Vaccination record", accent=(200, 70, 80)))
    asha.upload(home, "Shopping list.txt", text_file("Rice\nToor dal\nGhee\nTomatoes\nCoriander\n"))

    # Family space.
    docs = asha.folder(family, "Family documents")
    for name in ["Ration card.pdf", "Property tax 2026.pdf", "Emergency contacts.pdf", "Gas connection.pdf"]:
        asha.upload(docs, name, document_pdf(name[:-4]))
    recipes = asha.folder(family, "Recipes")
    asha.upload(recipes, "Puran poli.pdf", document_pdf("Puran poli", accent=(190, 120, 30)))
    asha.upload(recipes, "Modak.pdf", document_pdf("Modak", accent=(190, 120, 30)))
    asha.upload(family, "Holiday plan 2026.pdf", document_pdf("Holiday plan 2026", accent=(30, 140, 120)))
    asha.upload(family, "Diwali 2025.jpg", photo("diwali", "gold"))
    asha.upload(family, "Family tree.pdf", document_pdf("Family tree", accent=(120, 90, 40)))
    asha.upload(family, "Doctor appointments.txt", text_file("Aaji - eye check-up, 12 Dec\nMeera - dentist, 3 Jan\n"))
    wedding = asha.folder(family, "Wedding album")
    for i, pal in enumerate(["gold", "dusk", "gold", "sunset"]):
        asha.upload(wedding, f"Wedding {i + 1:02d}.jpg", photo(f"w{i}", pal))

    rohan = Client()
    rme = rohan.login("rohan")
    car = rohan.folder(rme["home_id"], "Car documents")
    for name in ["RC book.pdf", "PUC certificate.pdf", "Service history.pdf"]:
        rohan.upload(car, name, document_pdf(name[:-4], accent=(60, 60, 60)))
    rohan.call("POST", f"/nodes/{car}/shares", {"user_id": users["asha"]["id"], "permission": "viewer"})

    # Photos last, so they lead "Recent files".
    goa = asha.folder(home, "Goa trip 2025")
    shots = [("Beach sunset.jpg", "sunset"), ("Morning walk.jpg", "morning"), ("Boat ride.jpg", "sea"),
             ("Hill view.jpg", "hills"), ("Evening at the fort.jpg", "dusk"), ("Lunch spot.jpg", "gold")]
    photo_ids = {}
    for name, pal in shots:
        photo_ids[name] = asha.upload(goa, name, photo(name, pal, big=name == "Hill view.jpg"))

    for node in [tax, insurance, photo_ids["Beach sunset.jpg"]]:
        asha.call("PUT", f"/nodes/{node}/star")

    # Sharing.
    asha.call("POST", f"/nodes/{insurance}/shares", {"user_id": users["rohan"]["id"], "permission": "editor"})
    asha.call("POST", f"/nodes/{school}/shares", {"user_id": users["meera"]["id"], "permission": "viewer"})
    asha.call("POST", f"/nodes/{goa}/shares", {"user_id": users["aaji"]["id"], "permission": "viewer"})
    link = asha.call("POST", f"/nodes/{goa}/links", {"allow_download": True})

    # Backups: a USB drive with a finished backup, and a NAS for weekly runs.
    usb = asha.call("POST", "/admin/backups", {"name": "USB drive", "kind": "local", "settings": {"path": "/backups/fennoly"},
                                                "schedule": "daily 02:00"})["target"]["id"]
    asha.call("POST", f"/admin/backups/{usb}/test")
    asha.call("POST", f"/admin/backups/{usb}/run")
    for _ in range(120):
        if asha.call("GET", f"/admin/backups/{usb}")["running"] is None:
            break
        time.sleep(1)
    asha.call("POST", "/admin/backups", {"name": "Home NAS", "kind": "smb", "encrypted": True,
                                          "settings": {"host": "192.168.1.20", "share": "backups", "path": "fennoly", "user": "asha"},
                                          "secrets": {"password": "demo"}, "schedule": "weekly sun 03:00"})

    print(json.dumps({
        "user": "asha", "password": PASSWORD, "home": home, "family": family, "goa": goa, "insurance": insurance,
        "photo": photo_ids["Beach sunset.jpg"], "link": link["token"],
    }))


if __name__ == "__main__":
    main()
