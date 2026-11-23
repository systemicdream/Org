#!/usr/bin/env python3
"""Regenerate Org's app icons from assets/branding/org-icon.png (needs Pillow).

Writes:
  packaging/macos/AppIcon.icns                    (macOS only: uses `iconutil`)
  crates/org-app/assets/icon/org-macos-512.png  (Dock icon when run unbundled)
  crates/org-app/assets/icon/org-256.png        (welcome screen)
  packaging/linux/icons/hicolor/NxN/apps/<APP_ID>.png

Usage: python3 scripts/make-icons.py
"""

import os
import shutil
import subprocess
import sys
import tempfile

from PIL import Image

APP_ID = "io.github.systemicdream.Org"
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def path(*parts):
    return os.path.join(ROOT, *parts)


art = Image.open(path("assets", "branding", "org-icon.png")).convert("RGBA")
bbox = art.split()[3].point(lambda v: 255 if v > 8 else 0).getbbox()
art = art.crop(bbox)


def place(canvas, fill):
    """The artwork fitted into `fill` of a transparent square canvas, centered."""
    box = int(round(canvas * fill))
    w, h = art.size
    s = box / max(w, h)
    a = art.resize((max(1, round(w * s)), max(1, round(h * s))), Image.LANCZOS)
    c = Image.new("RGBA", (canvas, canvas), (0, 0, 0, 0))
    c.paste(a, ((canvas - a.width) // 2, (canvas - a.height) // 2), a)
    return c


# macOS: Apple's icon grid puts the rounded square in ~824 of 1024 px.
mac = place(1024, 824 / 1024)
os.makedirs(path("crates", "org-app", "assets", "icon"), exist_ok=True)
mac.resize((512, 512), Image.LANCZOS).save(
    path("crates", "org-app", "assets", "icon", "org-macos-512.png"), optimize=True
)
if shutil.which("iconutil"):
    with tempfile.TemporaryDirectory() as tmp:
        iconset = os.path.join(tmp, "AppIcon.iconset")
        os.makedirs(iconset)
        for size in (16, 32, 128, 256, 512):
            for scale in (1, 2):
                name = f"icon_{size}x{size}{'@2x' if scale == 2 else ''}.png"
                mac.resize((size * scale,) * 2, Image.LANCZOS).save(os.path.join(iconset, name))
        os.makedirs(path("packaging", "macos"), exist_ok=True)
        subprocess.run(
            ["iconutil", "-c", "icns", iconset, "-o", path("packaging", "macos", "AppIcon.icns")],
            check=True,
        )
else:
    print("iconutil not found (not macOS): skipping AppIcon.icns", file=sys.stderr)

# Linux (hicolor theme) and the in-app copy: the artwork fills most of the square.
for size in (16, 24, 32, 48, 64, 128, 256, 512):
    d = path("packaging", "linux", "icons", "hicolor", f"{size}x{size}", "apps")
    os.makedirs(d, exist_ok=True)
    place(size, 0.94 if size >= 48 else 1.0).save(os.path.join(d, f"{APP_ID}.png"), optimize=True)
place(256, 0.94).save(path("crates", "org-app", "assets", "icon", "org-256.png"), optimize=True)
print("icons written")
