"""Process generated app icon into Tauri icon set.

Cleans the AI-generated master (black outer corners + bottom-left watermark)
by flood-filling the outside-squircle region from the borders and replacing
it with the warm-paper cream, then exports the Tauri icon set.
"""
from pathlib import Path

import numpy as np
from PIL import Image

SRC = Path(r"F:\su\alltokens\docs\icon-preview.png")
OUT = Path(r"F:\su\alltokens\src-tauri\icons")
CREAM = np.array([245, 239, 228], dtype=np.uint8)  # #F5EFE4

img = Image.open(SRC).convert("RGB")
arr = np.asarray(img)

# --- 1. Crop to the squircle bounding box (column/row cream counts, so the
#        sparse watermark pixels cannot stretch the bbox) ---
cream_mask = (arr[:, :, 0] > 200) & (arr[:, :, 1] > 185) & (arr[:, :, 2] > 160)
min_run = img.width // 4
col_ok = np.where(cream_mask.sum(axis=0) > min_run)[0]
row_ok = np.where(cream_mask.sum(axis=1) > min_run)[0]
x0, x1 = col_ok.min() + 2, col_ok.max() - 2
y0, y1 = row_ok.min() + 2, row_ok.max() - 2
side = max(x1 - x0, y1 - y0)
cx, cy = (x0 + x1) // 2, (y0 + y1) // 2
half = side // 2
crop = arr[cy - half:cy + half, cx - half:cx + half].copy()
print("cropped:", crop.shape)

# --- 2. Flood fill from borders through "background" pixels (dark or gray,
#        i.e. not cream and not copper) and paint the region cream.
#        The charcoal baseline is enclosed by cream so it stays untouched. ---
f = crop.astype(np.int16)
mean = f.mean(axis=2)
copper = (f[:, :, 0] > 120) & ((f[:, :, 0] - f[:, :, 1]) > 50) & ((f[:, :, 1] - f[:, :, 2]) > 30)
cream_px = (f[:, :, 0] > 200) & (f[:, :, 1] > 185) & (f[:, :, 2] > 160)
passable = (~cream_px) & (~copper) & (mean < 215)

filled = np.zeros(passable.shape, dtype=bool)
filled[0, :] = passable[0, :]
filled[-1, :] = passable[-1, :]
filled[:, 0] = passable[:, 0]
filled[:, -1] = passable[:, -1]
for _ in range(2000):
    grown = filled.copy()
    grown[1:, :] |= filled[:-1, :]
    grown[:-1, :] |= filled[1:, :]
    grown[:, 1:] |= filled[:, :-1]
    grown[:, :-1] |= filled[:, 1:]
    grown &= passable
    if (grown == filled).all():
        break
    filled = grown
print("background pixels cleaned:", int(filled.sum()))
crop[filled] = CREAM

clean = Image.fromarray(crop, "RGB")
# Save the cleaned master back over the preview copy as well? No: keep the
# raw generated preview untouched; only icons are derived from the clean one.

def save_png(size, name):
    out = clean.resize((size, size), Image.LANCZOS)
    out.save(OUT / name, "PNG")
    print(name, out.size)

save_png(32, "32x32.png")
save_png(128, "128x128.png")
save_png(512, "icon.png")

ico_sizes = [(16, 16), (32, 32), (48, 48), (256, 256)]
clean.save(OUT / "icon.ico", format="ICO", sizes=ico_sizes)
print("icon.ico", ico_sizes)
