#!/usr/bin/env python3
"""Draws the disk image window's background: the landing page's dithered
orange waves (site/dither.js, one still frame) on the light paper colour, and
an arrow from the app to the Applications link. Light, not the landing's dark
paper: Finder draws the icon labels in black on a background picture, even in
dark mode. Finder shows still pictures only, so the waves do not move.

    make-background.py OUT_DIR

Writes background.png (1x) and background@2x.png, committed next to this
script. The release workflow joins them into one TIFF with tiffutil, so Finder
picks the sharp one on a Retina screen. Needs numpy and Pillow; run it again
only to change the picture, and keep WIDTH, HEIGHT and the icon centres in step
with dmg-settings.py.
"""

import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

WIDTH, HEIGHT = 640, 400            # window content, in points
APP, APPLICATIONS = (170, 180), (470, 180)  # icon centres, as in dmg-settings.py
ICON = 128

PAPER = (0xF6, 0xEF, 0xE6)
ORANGE = (0xF2, 0x6A, 0x21)
ARROW = (0x17, 0x13, 0x0F)
DENSITY, CREST_LOW, CREST_HIGH = 0.5, 0.25, 0.95
SPEED, FREQ, AMP = 0.05, 3.0, 0.3
TIME = 15.0                         # which frame of the animation
CELL = 2                            # dot size, in points
FADE_START, FADE_END = 190, 310     # waves fade out between these heights, in points

BAYER8 = np.array([
    0, 48, 12, 60, 3, 51, 15, 63, 32, 16, 44, 28, 35, 19, 47, 31,
    8, 56, 4, 52, 11, 59, 7, 55, 40, 24, 36, 20, 43, 27, 39, 23,
    2, 50, 14, 62, 1, 49, 13, 61, 34, 18, 46, 30, 33, 17, 45, 29,
    10, 58, 6, 54, 9, 57, 5, 53, 42, 26, 38, 22, 41, 25, 37, 21,
]).reshape(8, 8) / 64.0 + 0.5 / 64.0


def mod289(x):
    return x - np.floor(x / 289.0) * 289.0


def permute(x):
    return mod289((x * 34.0 + 1.0) * x)


def cnoise(px, py):
    """Classic Perlin noise, Stefan Gustavson's GLSL version, on arrays."""
    pi = np.stack([np.floor(px), np.floor(py), np.floor(px) + 1, np.floor(py) + 1])
    pf = np.stack([px - np.floor(px), py - np.floor(py), px - np.floor(px) - 1, py - np.floor(py) - 1])
    pi = mod289(pi)
    ix, iy = pi[[0, 2, 0, 2]], pi[[1, 1, 3, 3]]
    fx, fy = pf[[0, 2, 0, 2]], pf[[1, 1, 3, 3]]
    i = permute(permute(ix) + iy)
    gx = (i / 41.0 - np.floor(i / 41.0)) * 2.0 - 1.0
    gy = np.abs(gx) - 0.5
    gx = gx - np.floor(gx + 0.5)
    norm = 1.79284291400159 - 0.85373472095314 * (gx * gx + gy * gy)
    gx, gy = gx * norm, gy * norm
    n = gx * fx + gy * fy           # n00, n10, n01, n11
    fade = lambda t: t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
    ux, uy = fade(pf[0]), fade(pf[1])
    nx0 = n[0] + (n[1] - n[0]) * ux
    nx1 = n[2] + (n[3] - n[2]) * ux
    return 2.3 * (nx0 + (nx1 - nx0) * uy)


def fbm(x, y):
    value, amp = 0.0, 1.0
    for _ in range(4):
        value = value + amp * np.abs(cnoise(x, y))
        x, y, amp = x * FREQ, y * FREQ, amp * AMP
    return value


def draw(scale: int) -> Image.Image:
    w, h = WIDTH * scale, HEIGHT * scale
    cell = CELL * scale
    cols, rows = w // cell + 1, h // cell + 1
    # Same frame of reference as the landing: waves hang from the top centre,
    # sized by the window height.
    cx, cy = np.meshgrid(np.arange(cols) * CELL, np.arange(rows) * CELL)
    ref = 1.4 * HEIGHT
    u = (cx - 0.5 * WIDTH) / ref
    v = (0.5 * ref - cy) / ref
    shift = fbm(u - TIME * SPEED, v - TIME * SPEED)
    f = fbm(u + shift, v + shift)
    # Waves along the top edge only, faded out before the icons so the
    # arrow and the labels stay on plain paper.
    reach = np.clip((FADE_END - cy) / (FADE_END - FADE_START), 0.0, 1.0)
    f = np.clip(f, 0.0, 1.0) * reach * reach * (3.0 - 2.0 * reach)
    t = np.clip((f - CREST_LOW) / (CREST_HIGH - CREST_LOW), 0.0, 1.0)
    lit = t * t * (3.0 - 2.0 * t) * DENSITY > BAYER8[np.arange(rows)[:, None] % 8, np.arange(cols)[None, :] % 8]

    pixels = np.empty((rows, cols, 3), np.uint8)
    pixels[:] = PAPER
    pixels[lit] = ORANGE
    image = Image.fromarray(pixels).resize((cols * cell, rows * cell), Image.NEAREST).crop((0, 0, w, h))

    # Arrow between the two icons, at icon height.
    d = ImageDraw.Draw(image)
    y = APP[1] * scale
    x0 = (APP[0] + ICON // 2 + 22) * scale
    x1 = (APPLICATIONS[0] - ICON // 2 - 22) * scale
    stroke = 3 * scale
    head = 11 * scale
    d.line([(x0, y), (x1 - head // 2, y)], fill=ARROW, width=stroke)
    d.polygon([(x1, y), (x1 - head, y - head * 0.75), (x1 - head, y + head * 0.75)], fill=ARROW)
    return image


def main() -> None:
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    draw(1).save(out / "background.png", dpi=(72, 72))
    draw(2).save(out / "background@2x.png", dpi=(144, 144))


if __name__ == "__main__":
    main()
