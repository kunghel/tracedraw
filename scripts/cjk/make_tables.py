#!/usr/bin/env python3
"""Make the Chinese conversion and pinyin tables of Text > Chinese (#2182).

Downloads the sources at fixed commits, checks their SHA-256, and writes
apps/tracedraw/assets/cjk/:
  s2t.txt, t2s.txt  one "from to" pair per line, characters and then the
                    phrases that settle the characters with several
                    conversions, from the OpenCC dictionaries (Apache-2.0)
  pinyin.txt        "reading<TAB>characters" lines, the most common
                    Mandarin reading of each character, from the
                    mozillazg/pinyin-data kMandarin table (MIT, made from
                    the Unicode Unihan database)
and the two licences next to them. Run from the repository root:
  python3 scripts/cjk/make_tables.py
"""

import hashlib
import os
import sys
import urllib.request

OPENCC = "https://raw.githubusercontent.com/BYVoid/OpenCC/fae2a0a911c30764e669a7d8b0591cd8e852934a/"
PINYIN = "https://raw.githubusercontent.com/mozillazg/pinyin-data/fe29ad32bdf352b31a02b9936e537f575432e9a6/"
SOURCES = {
    "st": (OPENCC + "data/dictionary/STCharacters.txt",
           "a0ca1601c70648cf48b33c3c6210ccbecc5c7eead4b4c3daf76587ba2c03582b"),
    "ts": (OPENCC + "data/dictionary/TSCharacters.txt",
           "9ff46a7d30e5765375eb13d33f2b03a34d298913caf2b120380679f33ae1642d"),
    "st_phrases": (OPENCC + "data/dictionary/STPhrases.txt",
                   "24d23700f4f3d1860b899aa90896dcc7ec56c535a0b3a246bbf5a7048c7bdeb8"),
    "ts_phrases": (OPENCC + "data/dictionary/TSPhrases.txt",
                   "1b1e60a3d50492bc088bcd4372152570b5f0ccbe2ded0b66c90b6b8eeb5f849f"),
    "opencc_license": (OPENCC + "LICENSE",
                       "b534e465949558eec2597b04f5092b5e161236a68dfbfd04d547592ac3964308"),
    "mandarin": (PINYIN + "kMandarin.txt",
                 "643c38f4a533ac6f25d16390c3b09df41bac8e8f476da88cd18371fdb299198a"),
    "pinyin_license": (PINYIN + "LICENSE",
                       "9c048697be2502a16e8bcb282d5d465a07295b2def0ffb05a269c5d39dbe1586"),
}
OUT = os.path.join("apps", "tracedraw", "assets", "cjk")


def fetch(key):
    url, sha = SOURCES[key]
    data = urllib.request.urlopen(url, timeout=60).read()
    got = hashlib.sha256(data).hexdigest()
    if got != sha:
        sys.exit(f"{url}: SHA-256 {got}, expected {sha}")
    return data.decode("utf-8")


def entries(text):
    for line in text.splitlines():
        if not line or line.startswith("#"):
            continue
        key, _, values = line.partition("\t")
        yield key, values.split(" ")


def pairs(chars, phrases):
    """The characters with their first conversion, then the phrases that
    settle a character with several (kept only where the characters alone
    would go wrong, and only those of the same length)."""
    first, unsure, out = {}, set(), []
    for key, values in entries(chars):
        if len(key) == 1 and len(values[0]) == 1:
            first[key] = values[0]
            if len(values) > 1:
                unsure.add(key)
            if key != values[0]:
                out.append(f"{key} {values[0]}")
    for key, values in entries(phrases):
        to = values[0]
        if len(to) != len(key) or not any(c in unsure for c in key):
            continue
        if "".join(first.get(c, c) for c in key) != to:
            out.append(f"{key} {to}")
    return out


def readings(text):
    by = {}
    for line in text.splitlines():
        if not line.startswith("U+"):
            continue
        code, _, rest = line.partition(":")
        reading = rest.split("#")[0].strip().split(",")[0].strip()
        if reading:
            by.setdefault(reading, []).append(chr(int(code[2:], 16)))
    return [f"{r}\t{''.join(cs)}" for r, cs in sorted(by.items())]


def write(name, header, lines):
    with open(os.path.join(OUT, name), "w", encoding="utf-8", newline="\n") as f:
        f.write(header + "\n".join(lines) + "\n")


def main():
    os.makedirs(OUT, exist_ok=True)
    note = "# Made by scripts/cjk/make_tables.py from {}. Do not edit.\n"
    write("s2t.txt", note.format("OpenCC STCharacters.txt and STPhrases.txt (Apache-2.0, see OpenCC-LICENSE.txt)"),
          pairs(fetch("st"), fetch("st_phrases")))
    write("t2s.txt", note.format("OpenCC TSCharacters.txt and TSPhrases.txt (Apache-2.0, see OpenCC-LICENSE.txt)"),
          pairs(fetch("ts"), fetch("ts_phrases")))
    write("pinyin.txt", note.format("pinyin-data kMandarin.txt (MIT, see pinyin-data-LICENSE.txt; "
                                    "readings from the Unicode Unihan database)"),
          readings(fetch("mandarin")))
    for key, name in (("opencc_license", "OpenCC-LICENSE.txt"),
                      ("pinyin_license", "pinyin-data-LICENSE.txt")):
        with open(os.path.join(OUT, name), "w", encoding="utf-8", newline="\n") as f:
            f.write(fetch(key))


if __name__ == "__main__":
    main()
