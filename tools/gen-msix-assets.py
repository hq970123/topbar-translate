#!/usr/bin/env python3
"""Generate MSIX visual assets (packaging/msix/Assets) from src-tauri/icons/icon.png."""
from PIL import Image
import os
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
src = Image.open(os.path.join(root, "src-tauri/icons/icon.png")).convert("RGBA")
out = os.path.join(root, "packaging/msix/Assets")
os.makedirs(out, exist_ok=True)

def canvas(w, h, icon_frac):
    c = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    s = int(min(w, h) * icon_frac)
    ic = src.resize((s, s), Image.LANCZOS)
    c.paste(ic, ((w - s) // 2, (h - s) // 2), ic)
    return c

specs = {  # name: (w, h, icon fraction) at scale-100
    "Square44x44Logo": (44, 44, 1.0),
    "Square71x71Logo": (71, 71, 0.8),
    "Square150x150Logo": (150, 150, 0.66),
    "Wide310x150Logo": (310, 150, 0.66),
    "Square310x310Logo": (310, 310, 0.66),
    "StoreLogo": (50, 50, 1.0),
    "SplashScreen": (620, 300, 0.5),
}
for name, (w, h, f) in specs.items():
    for scale in (100, 125, 150, 200, 400):
        k = scale / 100
        canvas(round(w * k), round(h * k), f).save(os.path.join(out, f"{name}.scale-{scale}.png"))
# taskbar / Start icons without plate
for size in (16, 24, 32, 48, 256):
    ic = src.resize((size, size), Image.LANCZOS)
    for suffix in ("targetsize-%d" % size, "targetsize-%d_altform-unplated" % size, "targetsize-%d_altform-lightunplated" % size):
        ic.save(os.path.join(out, f"Square44x44Logo.{suffix}.png"))
print("ok", len(os.listdir(out)))
