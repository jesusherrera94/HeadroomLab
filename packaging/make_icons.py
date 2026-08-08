#!/usr/bin/env python3
"""Renders `packaging/icon.png`, the master artwork every platform's icon is cut
from.

The mark is the same one `src/presentation/components/atoms/logo.rs` paints at
runtime — a play triangle with input/output leads and a little waveform, on the
app's rounded inset tile — so the icon and the in-app logo cannot drift apart.
The geometry below is that file's 64x64 grid, verbatim.

Written by hand rather than with Pillow because neither Pillow nor numpy is a
build requirement of this project, and an icon generator is not worth making one.
Anti-aliasing is analytic: coverage comes from a pixel's distance to the shape,
which for strokes and rounded rectangles is exact and cheap.

    python3 packaging/make_icons.py

Emits, from one renderer so nothing can drift:

    packaging/icon.png                 1024, the master artwork
    packaging/icon-256.png             embedded in the binary as the window icon
    packaging/macos/AppIcon.icns       the .app bundle icon
    packaging/windows/AppIcon.ico      embedded as a Win32 resource
    packaging/linux/hicolor/...        the freedesktop icon theme

To use your own artwork instead, replace `render()` with a PNG loader — or drop
your own files at those paths and stop running this script. Nothing reads the
generator at build time; these are committed artifacts.
"""

import math
import struct
import sys
import zlib
from pathlib import Path

# -- Palette (src/presentation/theme.rs) ----------------------------------
STROKE = (0x90, 0xCA, 0xF9)  # theme::LOGO_STROKE
TILE = (0x26, 0x26, 0x2B)  # theme::INSET_SURFACE
BORDER = (0x3A, 0x3A, 0x41)  # the prototype's tile border

# -- Geometry, on logo.rs's 64x64 grid ------------------------------------
TRIANGLE = [(18.0, 12.0), (52.0, 32.0), (18.0, 52.0)]
LEADS = [
    ((6.0, 23.0), (18.0, 23.0)),
    ((6.0, 41.0), (18.0, 41.0)),
    ((52.0, 32.0), (60.0, 32.0)),
]
WAVE = [
    (23.0, 40.0), (26.0, 26.0), (29.0, 34.0), (32.0, 28.0),
    (35.0, 35.0), (38.0, 31.0), (41.0, 34.0), (44.0, 32.0),
]
WIDE, THIN = 3.0, 2.0

# The mark occupies ~64% of the tile, as `logo_tile` lays it out.
MARK_SCALE = 0.64


def segments():
    """Every stroked segment as ((x0, y0), (x1, y1), width) on the 64-grid."""
    out = []
    for i in range(len(TRIANGLE)):
        out.append((TRIANGLE[i], TRIANGLE[(i + 1) % len(TRIANGLE)], WIDE))
    out.extend((a, b, THIN) for a, b in LEADS)
    out.extend((WAVE[i], WAVE[i + 1], THIN) for i in range(len(WAVE) - 1))
    return out


def distance_to_segment(px, py, ax, ay, bx, by):
    dx, dy = bx - ax, by - ay
    length_sq = dx * dx + dy * dy
    if length_sq == 0.0:
        return math.hypot(px - ax, py - ay)
    t = max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / length_sq))
    return math.hypot(px - (ax + t * dx), py - (ay + t * dy))


def rounded_rect_coverage(px, py, size, radius):
    """Signed-distance coverage of a rounded square filling the canvas."""
    half = size / 2.0
    qx = abs(px - half) - (half - radius)
    qy = abs(py - half) - (half - radius)
    outside = math.hypot(max(qx, 0.0), max(qy, 0.0))
    distance = outside + min(max(qx, qy), 0.0) - radius
    # One pixel of feather, centred on the edge.
    return max(0.0, min(1.0, 0.5 - distance))


def blend(dst, src, alpha):
    return tuple(round(d + (s - d) * alpha) for d, s in zip(dst, src))


def render(size):
    radius = size * 0.22  # macOS-ish squircle corner; matches the in-app tile
    margin = size * 0.18  # `logo_tile`'s inner margin
    mark = size * MARK_SCALE
    scale = mark / 64.0
    offset = margin + (size - 2 * margin - mark) / 2.0

    # Start fully transparent so the corners stay round in every consumer.
    pixels = [[(0, 0, 0, 0)] * size for _ in range(size)]

    # Tile, with a hairline border.
    border_px = max(1.0, size / 256.0)
    for y in range(size):
        for x in range(size):
            coverage = rounded_rect_coverage(x + 0.5, y + 0.5, size, radius)
            if coverage <= 0.0:
                continue
            inner = rounded_rect_coverage(
                x + 0.5, y + 0.5, size, radius
            ) - rounded_rect_coverage(
                x + 0.5 - border_px, y + 0.5 - border_px, size - 2 * border_px, radius
            )
            colour = blend(TILE, BORDER, max(0.0, min(1.0, inner)))
            pixels[y][x] = (*colour, round(255 * coverage))

    # Strokes, each limited to its own bounding box — the whole reason this runs
    # in a couple of seconds rather than a couple of minutes.
    for (ax, ay), (bx, by), width in segments():
        ax, ay = ax * scale + offset, ay * scale + offset
        bx, by = bx * scale + offset, by * scale + offset
        half = width * scale / 2.0
        pad = half + 1.5

        x0 = max(0, int(min(ax, bx) - pad))
        x1 = min(size - 1, int(max(ax, bx) + pad) + 1)
        y0 = max(0, int(min(ay, by) - pad))
        y1 = min(size - 1, int(max(ay, by) + pad) + 1)

        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                d = distance_to_segment(x + 0.5, y + 0.5, ax, ay, bx, by)
                alpha = max(0.0, min(1.0, half - d + 0.5))
                if alpha <= 0.0:
                    continue
                r, g, b, a = pixels[y][x]
                nr, ng, nb = blend((r, g, b), STROKE, alpha)
                pixels[y][x] = (nr, ng, nb, max(a, round(255 * alpha)))

    return pixels


def write_png(path, pixels):
    size = len(pixels)
    raw = bytearray()
    for row in pixels:
        raw.append(0)  # filter: none
        for r, g, b, a in row:
            raw += bytes((r, g, b, a))

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    Path(path).write_bytes(png)


def png_bytes(pixels):
    """The same PNG encoding as `write_png`, returned rather than written."""
    import io

    size = len(pixels)
    raw = bytearray()
    for row in pixels:
        raw.append(0)
        for r, g, b, a in row:
            raw += bytes((r, g, b, a))

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    out = io.BytesIO()
    out.write(b"\x89PNG\r\n\x1a\n")
    out.write(chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)))
    out.write(chunk(b"IDAT", zlib.compress(bytes(raw), 9)))
    out.write(chunk(b"IEND", b""))
    return out.getvalue()


def bmp_entry(pixels):
    """An ICO image in BMP form: BITMAPINFOHEADER, BGRA bottom-up, AND mask.

    Sizes below 256 are written this way rather than as PNG. Windows has read
    PNG-compressed icon entries since Vista, but several shell surfaces still
    render small PNG entries blank, and a blank taskbar icon is exactly the sort
    of thing nobody notices until release day.
    """
    size = len(pixels)
    header = struct.pack(
        "<IiiHHIIiiII", 40, size, size * 2, 1, 32, 0, size * size * 4, 0, 0, 0, 0
    )

    xor = bytearray()
    for row in reversed(pixels):  # BMP rows run bottom-up
        for r, g, b, a in row:
            xor += bytes((b, g, r, a))

    # A fully opaque AND mask: the alpha channel above already carries the
    # transparency, and modern Windows honours it.
    stride = ((size + 31) // 32) * 4
    and_mask = bytes(stride * size)

    return header + bytes(xor) + and_mask


def write_ico(path, sizes):
    entries, blobs = [], []
    offset = 6 + 16 * len(sizes)

    for size in sizes:
        pixels = render(size)
        data = png_bytes(pixels) if size >= 256 else bmp_entry(pixels)
        # 0 means 256 in the directory's single-byte fields.
        dimension = 0 if size >= 256 else size
        entries.append(
            struct.pack("<BBBBHHII", dimension, dimension, 0, 0, 1, 32, len(data), offset)
        )
        blobs.append(data)
        offset += len(data)

    Path(path).write_bytes(
        struct.pack("<HHH", 0, 1, len(sizes)) + b"".join(entries) + b"".join(blobs)
    )


def write_icns(path, entries):
    """ICNS with PNG payloads, which macOS has accepted since 10.7."""
    body = b""
    for icon_type, size in entries:
        data = png_bytes(render(size))
        body += icon_type + struct.pack(">I", len(data) + 8) + data
    Path(path).write_bytes(b"icns" + struct.pack(">I", len(body) + 8) + body)


if __name__ == "__main__":
    root = Path(__file__).parent
    rendered = {}

    def png_at(size, *paths):
        if size not in rendered:
            rendered[size] = render(size)
        for path in paths:
            path.parent.mkdir(parents=True, exist_ok=True)
            write_png(path, rendered[size])
            print(f"  {path.relative_to(root.parent)}")

    print("master artwork")
    png_at(1024, root / "icon.png")
    # 256 is the window icon embedded in the binary: big enough for a HiDPI
    # taskbar, small enough that decoding it at startup costs nothing.
    png_at(256, root / "icon-256.png")

    print("macOS")
    icns = root / "macos" / "AppIcon.icns"
    icns.parent.mkdir(parents=True, exist_ok=True)
    write_icns(
        icns,
        [
            (b"icp4", 16), (b"icp5", 32), (b"ic11", 32), (b"ic12", 64),
            (b"ic07", 128), (b"ic13", 256), (b"ic08", 256),
            (b"ic14", 512), (b"ic09", 512), (b"ic10", 1024),
        ],
    )
    print(f"  {icns.relative_to(root.parent)}")

    print("Windows")
    ico = root / "windows" / "AppIcon.ico"
    ico.parent.mkdir(parents=True, exist_ok=True)
    write_ico(ico, [16, 32, 48, 64, 128, 256])
    print(f"  {ico.relative_to(root.parent)}")

    print("Linux (freedesktop icon theme)")
    for size in (16, 32, 48, 64, 128, 256, 512):
        png_at(
            size,
            root / "linux" / "hicolor" / f"{size}x{size}" / "apps" / "headroomlab.png",
        )
