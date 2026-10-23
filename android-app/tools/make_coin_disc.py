#!/usr/bin/env python3
"""Derive the circular coin emblem used by the app from the square-ish source photo.

The marketing render in `res/drawable-nodpi/ultranet_official_coin.png` is 861x686: a coin lit
against a dark studio backdrop. Drawn into a square frame it letterboxes, which showed up as pale
arcs inside the hero ring. This script crops to the measured coin circle, feathers the edge and
writes a square, transparent-cornered disc so any circular clip renders cleanly.

Usage: python3 android-app/tools/make_coin_disc.py
"""

import os
import struct
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
SOURCE = os.path.join(HERE, "..", "app", "src", "main", "res", "drawable-nodpi", "ultranet_official_coin.png")
TARGET = os.path.join(HERE, "..", "app", "src", "main", "res", "drawable-nodpi", "ultranet_coin_disc.png")

# Circle fitted to the coin rim in the source render, plus the output framing.
COIN_CX, COIN_CY = 421.5, 328.5
OUT_SIZE = 512
DISC_RADIUS = 255.0
FEATHER = 1.6


def load_png(path):
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
    pos, idat = 8, b""
    width = height = None
    channels = None
    while pos < len(data):
        length = struct.unpack(">I", data[pos:pos + 4])[0]
        kind = data[pos + 4:pos + 8]
        chunk = data[pos + 8:pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            width, height, _depth, colour_type = struct.unpack(">IIBB", chunk[:10])
            channels = {0: 1, 2: 3, 4: 2, 6: 4}[colour_type]
        elif kind == b"IDAT":
            idat += chunk
        elif kind == b"IEND":
            break
    raw = zlib.decompress(idat)
    stride = width * channels
    out = bytearray(height * stride)
    prev = bytearray(stride)
    offset = 0
    for y in range(height):
        filter_type = raw[offset]
        offset += 1
        line = bytearray(raw[offset:offset + stride])
        offset += stride
        if filter_type == 1:
            for x in range(channels, stride):
                line[x] = (line[x] + line[x - channels]) & 0xFF
        elif filter_type == 2:
            for x in range(stride):
                line[x] = (line[x] + prev[x]) & 0xFF
        elif filter_type == 3:
            for x in range(stride):
                left = line[x - channels] if x >= channels else 0
                line[x] = (line[x] + ((left + prev[x]) >> 1)) & 0xFF
        elif filter_type == 4:
            for x in range(stride):
                left = line[x - channels] if x >= channels else 0
                up = prev[x]
                up_left = prev[x - channels] if x >= channels else 0
                estimate = left + up - up_left
                pa, pb, pc = abs(estimate - left), abs(estimate - up), abs(estimate - up_left)
                predictor = left if (pa <= pb and pa <= pc) else (up if pb <= pc else up_left)
                line[x] = (line[x] + predictor) & 0xFF
        out[y * stride:(y + 1) * stride] = line
        prev = line
    return width, height, channels, out


def write_png(path, width, height, rgba):
    raw = bytearray()
    stride = width * 4
    for y in range(height):
        raw.append(0)  # filter: none
        raw += rgba[y * stride:(y + 1) * stride]

    def chunk(kind, payload):
        return (
            struct.pack(">I", len(payload))
            + kind
            + payload
            + struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF)
        )

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    open(path, "wb").write(png)


def main():
    width, height, channels, pixels = load_png(SOURCE)
    half = OUT_SIZE / 2.0
    out = bytearray(OUT_SIZE * OUT_SIZE * 4)
    for y in range(OUT_SIZE):
        for x in range(OUT_SIZE):
            dx, dy = (x + 0.5) - half, (y + 0.5) - half
            distance = (dx * dx + dy * dy) ** 0.5
            if distance > DISC_RADIUS + FEATHER:
                continue
            alpha = 255.0
            if distance > DISC_RADIUS:
                alpha = 255.0 * (DISC_RADIUS + FEATHER - distance) / FEATHER
            sx = min(width - 1, max(0, int(COIN_CX + dx)))
            sy = min(height - 1, max(0, int(COIN_CY + dy)))
            src = (sy * width + sx) * channels
            dst = (y * OUT_SIZE + x) * 4
            out[dst] = pixels[src]
            out[dst + 1] = pixels[src + 1]
            out[dst + 2] = pixels[src + 2]
            out[dst + 3] = int(max(0, min(255, alpha)))
    write_png(TARGET, OUT_SIZE, OUT_SIZE, out)
    print("wrote", os.path.normpath(TARGET), OUT_SIZE, "x", OUT_SIZE)


if __name__ == "__main__":
    main()
