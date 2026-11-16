#!/usr/bin/env python3
"""Generate original, non-sensitive CDR27 parser fixtures (MIT OR Apache-2.0).

Layouts: TraceDraw's cdr-format/riff.rs and loda.rs, cdr/parse.rs tests.
The hidden group flag was observed in contributor-owned version 27 files.
No private drawing, text, metadata, font or picture is read or copied.
These are synthetic parser samples, not files validated by another editor.
"""
from pathlib import Path
import struct
import zipfile
from io import BytesIO


def words(*values):
    return struct.pack('<' + 'I' * len(values), *values)


def chunk(tag, data):
    return tag + words(len(data)) + data + bytes(len(data) % 2)


def listing(tag, data):
    return chunk(b'LIST', tag + data)


def loda(kind, args):
    bodies = b''
    offsets = []
    for _, data in args:
        offsets.append(20 + len(bodies))
        bodies += data
    start = 20 + len(bodies)
    offsets.append(start)
    types = start + 4 * len(offsets)
    return (words(types + 4 * len(args), len(args), start, types, kind)
            + bodies + words(*offsets) + words(*(t for t, _ in reversed(args))))


def rectangle(flags=None, hidden_arg=False):
    # A black 10 x 10 mm rectangle, no text or font dependencies.
    coords = struct.pack('<4d', 100000, 100000, 1, 1) + bytes(8)
    coords += bytes(24 * 3 + 8)
    args = [(30, coords), (20, words(9))]
    if hidden_arg:
        args.append((16016, words(1)))
    header = b'' if flags is None else chunk(b'flgs', flags)
    return listing(b'obj ', header + chunk(b'loda', loda(1, args)))


def group(objects, flags=b'\x80\x01\x00\x10'):
    return listing(b'grp ', chunk(b'flgs', flags) + b''.join(objects))


def cdr(objects):
    color = struct.pack('<HH', 5, 0) + bytes(8)
    props = bytes([1]) + words(12) + color + bytes(1)
    solid = words(1300, len(props)) + props
    fill = words(9, 1300, len(solid) + 2) + struct.pack('<H', 1) + solid
    body = chunk(b'vrsn', struct.pack('<H', 2700))
    body += chunk(b'mcfg', bytes(12) + words(1000000, 700000))
    body += chunk(b'fild', fill)
    body += listing(b'page', listing(b'layr', b''.join(objects)))
    riff = chunk(b'RIFF', b'CDRU' + body)
    buf = BytesIO()
    with zipfile.ZipFile(buf, 'w', compression=zipfile.ZIP_STORED) as archive:
        put(archive, 'mimetype', b'application/x-vnd.corel.zcf.draw.document+zip')
        put(archive, 'content/root.dat', riff)
    return buf.getvalue()


def put(archive, name, data):
    entry = zipfile.ZipInfo(name, (2026, 11, 15, 0, 0, 0))
    entry.external_attr = 0o644 << 16
    archive.writestr(entry, data)


def main():
    r = rectangle()
    samples = {
        'hidden-two.cdr': cdr([group([r, r])]),
        'hidden-one.cdr': cdr([group([r])]),
        # A visible outer group must retain its hidden inner group.
        'nested.cdr': cdr([group([group([r, r]), r], b'\x80\x00\x00\x10')]),
        'visible.cdr': cdr([group([r, r], b'\x80\x00\x00\x10')]),
        'short-flags.cdr': cdr([group([r, r], b'\x80\x01')]),
        'wrong-record-type.cdr': cdr([group([r, r], b'\x80\x01\x00\x98')]),
        'unrelated-flag.cdr': cdr([group([r, r], b'\x80\x02\x00\x10')]),
        'hidden-object.cdr': cdr([rectangle(b'\0\x03\0\x08')]),
        'visible-object.cdr': cdr([rectangle(b'\0\x02\0\x08')]),
        'locked-object.cdr': cdr([rectangle(b'\x10\x02\0\x08')]),
        'hidden-locked-object.cdr': cdr([rectangle(b'\x10\x03\0\x08')]),
        'hidden-object-argument.cdr': cdr([rectangle(b'\0\x02\0\x08', True)]),
        'short-object-flags.cdr': cdr([rectangle(b'\0\x03\0')]),
        'wrong-object-record.cdr': cdr([rectangle(b'\0\x03\0\x98')]),
    }
    with zipfile.ZipFile(Path(__file__).with_name('samples.zip'), 'w') as archive:
        for name, data in samples.items():
            put(archive, name, data)
    print('Generated samples.zip:', ', '.join(samples))


if __name__ == '__main__':
    main()
