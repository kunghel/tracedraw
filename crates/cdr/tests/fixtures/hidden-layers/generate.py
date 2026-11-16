#!/usr/bin/env python3
"""Original CDR27 layer-visibility parser samples, MIT OR Apache-2.0.

Uses TraceDraw's own RIFF/ZIP and loda layouts, with layer flags observed
in a contributor-owned version 27 drawing. No source drawing is read.
These are parser fixtures, not files validated by another editor.
"""
from io import BytesIO
from pathlib import Path
import struct
import zipfile


def u32(*values):
    return struct.pack('<' + 'I' * len(values), *values)


def chunk(tag, data):
    return tag + u32(len(data)) + data + bytes(len(data) % 2)


def listing(tag, data):
    return chunk(b'LIST', tag + data)


def loda(kind, args):
    body = b''
    offsets = []
    for _, data in args:
        offsets.append(20 + len(body))
        body += data
    start = 20 + len(body)
    offsets.append(start)
    types = start + 4 * len(offsets)
    return (u32(types + 4 * len(args), len(args), start, types, kind)
            + body + u32(*offsets) + u32(*(t for t, _ in reversed(args))))


def layer(name, flags, empty=False, kind=0):
    props = chunk(b'loda', loda(kind, [(1000, (name+'\0').encode('utf-16le'))]))
    header = b'' if flags is None else chunk(b'flgs', flags)
    coords = struct.pack('<4d', 100000, 100000, 1, 1) + bytes(88)
    rect = listing(b'obj ', chunk(b'loda', loda(1, [(30, coords), (20, u32(9))])))
    return listing(b'layr', header + listing(b'lgob', props) + (b'' if empty else rect))


def put(archive, name, data):
    info = zipfile.ZipInfo(name, (2026, 11, 15, 0, 0, 0))
    info.external_attr = 0o644 << 16
    archive.writestr(info, data)


def cdr(layers, master=False):
    color = struct.pack('<HH', 5, 0) + bytes(8)
    props = b'\x01' + u32(12) + color + b'\0'
    solid = u32(1300, len(props)) + props
    fill = u32(9, 1300, len(solid)+2) + struct.pack('<H', 1) + solid
    body = chunk(b'vrsn', struct.pack('<H', 2700))
    body += chunk(b'mcfg', bytes(12) + u32(1000000, 700000)) + chunk(b'fild', fill)
    if master:
        body += listing(b'page', chunk(b'flgs', b'\0\0\x01\x90') + b''.join(layers))
        body += listing(b'page', layer('VISIBLE', b'\0\0\0\x98'))
    else:
        body += listing(b'page', b''.join(layers))
    buf = BytesIO()
    with zipfile.ZipFile(buf, 'w') as archive:
        put(archive, 'mimetype', b'application/x-vnd.corel.zcf.draw.document+zip')
        put(archive, 'content/root.dat', chunk(b'RIFF', b'CDRU'+body))
    return buf.getvalue()


def main():
    hidden = b'\x40\x01\0\x98'
    visible = b'\0\0\0\x98'
    samples = {
        'two-layers.cdr': cdr([layer('VISIBLE', visible), layer('HIDDEN', hidden)]),
        'hidden-only.cdr': cdr([layer('HIDDEN', hidden)]),
        'hidden-empty.cdr': cdr([layer('VISIBLE', visible), layer('HIDDEN', hidden, True)]),
        'hidden-master.cdr': cdr([layer('HIDDEN MASTER', hidden)], master=True),
        'visible.cdr': cdr([layer('VISIBLE', visible)]),
        'missing-flags.cdr': cdr([layer('VISIBLE', None)]),
        'short-flags.cdr': cdr([layer('VISIBLE', b'\x40\x01\0')]),
        'wrong-record-type.cdr': cdr([layer('VISIBLE', b'\x40\x01\0\x10')]),
        'unrelated-bit.cdr': cdr([layer('VISIBLE', b'\0\x02\0\x98')]),
        'guides.cdr': cdr([layer('Guides', b'\x0a\0\0\x98', kind=12)]),
        'locked.cdr': cdr([layer('LOCKED', b'\x10\0\0\x98')]),
        'excluded.cdr': cdr([layer('EXCLUDED', b'\x08\0\0\x98')]),
        'locked-excluded-hidden.cdr': cdr([layer('ALL', b'\x58\x01\0\x98')]),
        'locked-empty.cdr': cdr([layer('LOCKED', b'\x10\0\0\x98', True)]),
        'excluded-empty.cdr': cdr([layer('EXCLUDED', b'\x08\0\0\x98', True)]),
    }
    with zipfile.ZipFile(Path(__file__).with_name('samples.zip'), 'w') as archive:
        for name, data in samples.items():
            put(archive, name, data)
    print('Generated', len(samples), 'synthetic CDR27 layer samples.')


if __name__ == '__main__':
    main()
