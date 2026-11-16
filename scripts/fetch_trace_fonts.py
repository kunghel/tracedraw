#!/usr/bin/env python3
"""Fetch the free fonts the trace compares small print with.

Downloads, from the Google Fonts service, each face of FACES cut down to
the capitals and figures the trace reads, into
apps/tracedraw/assets/fonts/trace/, with FONTS.md (each face's copyright
notice and licence) and writes apps/tracedraw/src/trace_fonts.rs, the
list the application builds them in from. Run from the repository root:

    python3 scripts/fetch_trace_fonts.py

Standard library only. Fails (exit 1) when any face cannot be fetched.
"""
import os
import re
import struct
import sys
import urllib.parse
import urllib.request

FACES = [
    ("Inter", [400, 500, 600, 700, 800, 900]),
    ("Montserrat", [400, 500, 600, 700, 800, 900]),
    ("Poppins", [400, 500, 600, 700, 800, 900]),
    ("Roboto", [400, 500, 700, 900]),
    ("Open Sans", [400, 600, 700, 800]),
    ("Figtree", [600, 800]),
    ("Heebo", [700, 900]),
    ("Mulish", [700, 800]),
    ("Nunito", [700, 800]),
    ("Work Sans", [600, 700]),
    ("League Spartan", [700]),
    ("Lexend", [600]),
    ("Outfit", [600, 800]),
    ("Urbanist", [800]),
    ("Manrope", [800]),
    ("DM Sans", [700]),
    ("Rubik", [600]),
    ("Barlow", [700]),
    ("Kanit", [600]),
    ("Ubuntu", [700]),
    ("Fira Sans", [700]),
    ("Oswald", [600]),
    ("Bebas Neue", [400]),
    ("Anton", [400]),
    ("Archivo Black", [400]),
    ("Cinzel", [700]),
]
# Faces whose licence reserves their name are left out: a cut-down copy
# is a modified font, which may not carry a reserved name.
TEXT = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
OUT = "apps/tracedraw/assets/fonts/trace"
RS = "apps/tracedraw/src/trace_fonts.rs"
# An old browser name makes the service answer with TrueType files.
UA = "Mozilla/4.0"


def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read()


def name_record(data, want):
    """The English (or first) string of name ID `want`."""
    num = struct.unpack(">H", data[4:6])[0]
    for i in range(num):
        tag, _, off, ln = struct.unpack(">4sLLL", data[12 + 16 * i:28 + 16 * i])
        if tag != b"name":
            continue
        count, soff = struct.unpack(">HH", data[off + 2:off + 6])
        found = None
        for j in range(count):
            pid, eid, lid, nid, l, o = struct.unpack(">6H", data[off + 6 + 12 * j:off + 18 + 12 * j])
            if nid != want:
                continue
            raw = data[off + soff + o:off + soff + o + l]
            s = raw.decode("utf-16-be", "replace") if pid in (0, 3) else raw.decode("latin-1")
            if pid == 3 and lid == 0x409:
                return s
            found = found or s
        return found or ""
    return ""


def main():
    os.makedirs(OUT, exist_ok=True)
    rows = []
    notes = []
    for family, weights in FACES:
        stem = family.replace(" ", "")
        for w in weights:
            q = urllib.parse.quote(family)
            css = get(f"https://fonts.googleapis.com/css2?family={q}:wght@{w}&text={TEXT}").decode()
            m = re.search(r"src: url\((https://[^)]+)\) format\('truetype'\)", css)
            if not m:
                print(f"no TrueType file for {family} {w}", file=sys.stderr)
                sys.exit(1)
            data = get(m.group(1))
            if data[:4] not in (b"\x00\x01\x00\x00", b"true", b"OTTO"):
                print(f"{family} {w}: not a font file", file=sys.stderr)
                sys.exit(1)
            fn = f"{stem}-{w}.ttf"
            with open(os.path.join(OUT, fn), "wb") as f:
                f.write(data)
            rows.append((family, w, fn))
            if w == weights[0]:
                notes.append((family, name_record(data, 0).strip(), name_record(data, 13).strip()))
            print(f"{fn} {len(data)} bytes")
    with open(os.path.join(OUT, "FONTS.md"), "w") as f:
        f.write("# Fonts the trace compares small print with\n\n")
        f.write("Fetched by `scripts/fetch_trace_fonts.py` from the Google Fonts service, each cut down to\n")
        f.write("the capitals A to Z and the figures 0 to 9. Every font is licensed under the SIL Open Font\n")
        f.write("License, Version 1.1 (`OFL.txt` in this folder, also at https://openfontlicense.org).\n\n")
        f.write("| Font | Copyright |\n|---|---|\n")
        for family, cr, _ in notes:
            f.write(f"| {family} | {cr.replace('|', '/')} |\n")
    # The licence text, from the copy beside the interface font, without
    # that font's own notice.
    with open("apps/tracedraw/assets/fonts/Selawik-OFL.txt") as f:
        ofl = f.read()
    with open(os.path.join(OUT, "OFL.txt"), "w") as f:
        f.write("Copyright notices of the fonts in this folder: see FONTS.md.\n\n")
        f.write(ofl[ofl.index("This Font Software is licensed"):])
    with open(RS, "w") as f:
        f.write("//! The free fonts the trace compares small print with (see\n")
        f.write("//! `trace_text.rs`), written by `scripts/fetch_trace_fonts.py`.\n\n")
        f.write(f"pub(crate) const FACES: [(&str, u16, &[u8]); {len(rows)}] = [\n")
        for family, w, fn in rows:
            f.write(f'    (\n        "{family}",\n        {w},\n        include_bytes!("../assets/fonts/trace/{fn}"),\n    ),\n')
        f.write("];\n")


if __name__ == "__main__":
    main()
