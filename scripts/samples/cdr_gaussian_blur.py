#!/usr/bin/env python3
"""Generate an original CDR27 parser fixture. MIT OR Apache-2.0.

Two rectangles only. Not an assertion that a third-party writer emits this
minimal container. The style layout is observed in a contributor-owned CDR27.
"""

import json
import struct
from pathlib import Path
from zipfile import ZIP_STORED, ZipFile, ZipInfo


def words(*values):
    return struct.pack('<' + 'I' * len(values), *values)


def chunk(tag, data):
    return tag + words(len(data)) + data + bytes(len(data) % 2)


def group(tag, data):
    return chunk(b'LIST', tag + data)


def loda(kind, args):
    offsets, body = [], b''
    start = 20 + 4 * (len(args) + 1) + 4 * len(args)
    for _, value in args:
        offsets.append(start + len(body))
        body += value
    offsets.append(start + len(body))
    return words(offsets[-1], len(args), 20, 20 + 4 * len(offsets), kind) + words(*offsets) + words(*(t for t, _ in reversed(args))) + body


def rectangle(number, width, height, white=False):
    style = {'fill': {'type': '1', 'primaryColor': 'RGB255,USER,' + ('255,255,255' if white else '20,20,20') + ',100,original'}}
    if white:
        style['StackedBitmapEffects'] = {
            'GaussianBlurEffect': {'Radius': '1500', 'Resampled': '0'},
            'GaussianBlurEffect_Deleted': '0',
            'GaussianBlurEffect_Visible': '1',
            'GaussianBlurEffect_Z_Index': '0',
        }
    encoded = json.dumps(style, separators=(',', ':')).encode()
    coords = struct.pack('<4d', width * 10000, height * 10000, 1, 1) + bytes(64)
    args = [(20, words(number)), (30, coords), (201, words(1, len(encoded)) + encoded)]
    dx, dy = (-80, -20) if white else (-90, -30)
    matrix = bytes(8) + struct.pack('<H', 8) + bytes(6) + struct.pack('<6d', 1, 0, dx * 10000, 0, 1, dy * 10000)
    transform = group(b'trfl', chunk(b'trfd', words(0, 1, 12, 16) + matrix))
    return group(b'obj ', chunk(b'usdn', words(number)) + group(b'lgob', chunk(b'loda', loda(1, args)) + transform))


def generate():
    # File order is top to bottom; the reader reverses it for painting.
    objects = rectangle(2, 70, 25, white=True) + rectangle(1, 90, 45)
    page = group(b'page', group(b'gobj', group(b'layr', objects)))
    def fill(number, level):
        color = struct.pack('<HH', 5, 0) + bytes(4) + bytes([level, level, level, 0])
        properties = bytes([1]) + words(12) + color + bytes([0])
        solid = words(1300, len(properties)) + properties
        return chunk(b'fild', words(number, 1300, len(solid) + 2) + struct.pack('<H', 1) + solid)
    body = chunk(b'vrsn', struct.pack('<H', 2700)) + chunk(b'mcfg', bytes(12) + struct.pack('<ii', 2400000, 1200000)) + fill(1, 20) + fill(2, 255) + page
    riff = chunk(b'RIFF', b'CDRU' + body)
    destination = Path(__file__).resolve().parents[2] / 'crates/cdr/tests/fixtures/gaussian-blur.cdr'
    destination.parent.mkdir(parents=True, exist_ok=True)
    with ZipFile(destination, 'w', compression=ZIP_STORED) as archive:
        info = ZipInfo('content/riffData.cdr', date_time=(2026, 11, 15, 0, 0, 0))
        archive.writestr(info, riff)
    print(destination)


if __name__ == '__main__':
    generate()
