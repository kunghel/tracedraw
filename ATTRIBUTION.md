# Attribution

The original two-rectangle Gaussian blur fixture
(`crates/cdr/tests/fixtures/gaussian-blur.cdr`), its generator
(`scripts/samples/cdr_gaussian_blur.py`) and associated synthetic sample
images are by TraceDraw contributors, written for this change and licensed
MIT OR Apache-2.0. They contain no private source drawing or application
assets. The style record layout was observed independently in a
contributor-owned CDR27 file; see `CLEANROOM.md`.

Every asset bundled with TraceDraw, with its author, source and license.
Code dependencies (Rust crates) keep their own licenses, recorded in
their packages; `cargo tree` lists them from `Cargo.lock`.
Fonts embedded in PDF files are subset with the `subsetter` crate
(MIT OR Apache-2.0); the fonts themselves are the user's and are put in
only when their licence bits allow it. The desktop builds compile in
libwebp (Copyright Google Inc., BSD 3-Clause licence, through the
`libwebp-sys` crate) for lossy WebP export; the browser build uses the
pure-Rust `vaam-image-webp` crate (MIT OR Apache-2.0) for it instead.

| Asset | Path | Author and source | License |
|---|---|---|---|
| Selawik Regular and Bold (interface font) | `apps/tracedraw/assets/fonts/` | Copyright 2015 Microsoft Corporation, the open-source Selawik project | SIL Open Font License 1.1, see `Selawik-OFL.txt` next to the fonts |
| Fonts the trace compares small print with: 53 faces of 26 families (Inter, Montserrat, Poppins, Roboto, Open Sans and others), cut down to the capitals and figures | `apps/tracedraw/assets/fonts/trace/` | Their project authors, each copyright notice listed in `FONTS.md` there; fetched from the Google Fonts service by `scripts/fetch_trace_fonts.py` | SIL Open Font License 1.1, see `OFL.txt` next to the fonts |
| TestSymbol (test font: Selawik Regular's "A", "B", "C" and space, renamed and re-mapped to a symbol character map at U+F020 to U+F043) | `crates/text/testdata/` | Copyright 2015 Microsoft Corporation, the open-source Selawik project | SIL Open Font License 1.1, see `apps/tracedraw/assets/fonts/Selawik-OFL.txt` |
| Noto Sans Devanagari (test font) | `crates/text/testdata/` | Copyright 2022 The Noto Project Authors, from the notofonts collection | SIL Open Font License 1.1, see `NotoSansDevanagari-OFL.txt` next to the font |
| Noto Sans Tamil (test font) | `crates/text/testdata/` | Copyright 2022 The Noto Project Authors (https://github.com/notofonts/tamil), from the Google Fonts collection | SIL Open Font License 1.1, see `NotoSansTamil-OFL.txt` next to the font |
| Devanagari corpus PDF (test file) | `crates/io/testdata/devanagari-corpus.pdf` | Written by TraceDraw's contributors from `crates/text/testdata/devanagari-corpus.txt` with an office suite; it embeds a subset of FreeMono from GNU FreeFont | The document is ours; the embedded font subset is GNU GPL 3 with the FreeFont font exception, which leaves documents that embed it outside the GPL |
| Quicksand (variable, test font) | `crates/text/testdata/` | Copyright 2011 The Quicksand Project Authors, from the google/fonts collection | SIL Open Font License 1.1, see `Quicksand-OFL.txt` next to the font |
| Roboto Flex (variable, test font: a subset of five glyphs with all 13 axes) | `crates/text/testdata/RobotoFlex-subset.ttf` | Copyright 2011 The Roboto Flex Project Authors (https://github.com/googlefonts/roboto-flex) | SIL Open Font License 1.1, see `RobotoFlex-OFL.txt` next to the font |
| Fonts of the browser build (bundled into the module and placed in the published page's font folder when it is built; not stored in this repository) | `web/fonts.txt`, `apps/tracedraw/build.rs` | Liberation, DejaVu, Noto, TeX Gyre and the google/fonts families listed in `web/FONTS.md` | Their own free licences (SIL Open Font License 1.1, Apache License 2.0, Ubuntu Font Licence 1.0, GUST Font License, Bitstream Vera), copied into the font folder's `licences/` |
| Application icon (PNG sizes and `.ico`) | `assets/icon/` | Made for TraceDraw | MIT OR Apache-2.0 |
| Microsoft Store tiles | `packaging/windows/msix/Assets/` | Made for TraceDraw from the application icon | MIT OR Apache-2.0 |
| Tool, toolbar and panel icons | painted in code, `apps/tracedraw/src/ui/icons.rs` | Made for TraceDraw | MIT OR Apache-2.0 |
| Default colour palette (colour names and values) | `apps/tracedraw/src/palette.rs` | TraceDraw contributors; common process colour tints with descriptive names | MIT OR Apache-2.0 |
| RGB default palette (`default_rgb`, the same names as RGB values: 10% greys and multiples of 51) | `apps/tracedraw/src/palette.rs` | TraceDraw contributors; plain RGB values read from a screenshot attached to issue #620 | MIT OR Apache-2.0 |
| Spell-check core word list | `apps/tracedraw/lang/core-words.txt` | Made for TraceDraw | MIT OR Apache-2.0 |
| Spelling dictionaries offered for download (not bundled: fetched when the user asks, Options > Text > Spelling) | `apps/tracedraw/src/dictionary_download.rs` | The wooorm/dictionaries collection (https://github.com/wooorm/dictionaries), Hunspell dictionaries by their own authors; each download's licence is saved beside it as `<code>.license.txt` | Per language (SPDX): en, en-GB, en-AU, en-CA: MIT AND BSD; pt: LGPL-3.0 OR MPL-2.0; pt-PT, bg, da, el, hu, ko, ro, sk: GPL-2.0 OR LGPL-2.1 OR MPL-1.1; es: GPL-3.0 OR LGPL-3.0 OR MPL-1.1; fr: MPL-2.0; de, de-AT, de-CH: GPL-2.0 OR GPL-3.0; it, gl, uk: GPL-3.0; nl: BSD-3-Clause OR CC-BY-3.0; pl: GPL-3.0 OR LGPL-3.0 OR MPL-2.0; cs, eu, ga, nb, nn, vi: GPL-2.0; ru, lt: BSD-3-Clause; tr: MIT; ca: GPL-2.0 OR LGPL-2.1; cy, sv: LGPL-3.0; et, lv: LGPL-2.1; fa: Apache-2.0; he: AGPL-3.0; hr: LGPL-2.1 OR SISSL; sl: GPL-3.0 OR LGPL-2.1; sr: GPL-2.0 OR LGPL-2.1 OR MPL-1.1 OR CC-BY-SA-3.0 |
| AI enlargement model for Trace Bitmap (not bundled: fetched when the user asks, in the Trace dialog's Enlarge small pictures first; run by TraceDraw's own code in `apps/tracedraw/src/upscale_net.rs`) | `apps/tracedraw/src/upscale.rs` | The realesr-general-x4v3 weights of the Real-ESRGAN project (Copyright (c) 2021, Xintao Wang, https://github.com/xinntao/Real-ESRGAN), downloaded from its v0.2.5.0 release and checked by SHA-256 | BSD-3-Clause |
| Chinese conversion tables (Simplified to Traditional and back, characters and the phrases that settle them) | `apps/tracedraw/assets/cjk/s2t.txt`, `t2s.txt` | Made by `scripts/cjk/make_tables.py` from the OpenCC dictionaries STCharacters, STPhrases, TSCharacters and TSPhrases (Copyright the OpenCC authors, https://github.com/BYVoid/OpenCC) | Apache License 2.0, see `OpenCC-LICENSE.txt` next to the tables |
| Pinyin readings (the most common Mandarin reading of each character) | `apps/tracedraw/assets/cjk/pinyin.txt` | Made by `scripts/cjk/make_tables.py` from the kMandarin table of pinyin-data (Copyright 2016 mozillazg, https://github.com/mozillazg/pinyin-data), itself made from the Unicode Unihan database | MIT, see `pinyin-data-LICENSE.txt` next to the table; the Unihan data is under the Unicode License v3 |
| Hyphenation patterns | `crates/text/src/hyphen.rs` | Made for TraceDraw (a small Liang-style set) | MIT OR Apache-2.0 |
| Interface translations | `apps/tracedraw/lang/*.toml` | Made for TraceDraw | MIT OR Apache-2.0 |
| eframe 0.36.2 (patched copy of the crate, its OpenGL context fallback backported from 0.37.0, #616) | `vendor/eframe/` | Copyright Emil Ernerfeldt and the egui contributors, from crates.io; the change is listed in `vendor/README.md` | MIT OR Apache-2.0 |
| winit 0.30.13 (patched copy of the crate, a pen's pressure on X11 and macOS, #850) | `vendor/winit/` | Copyright the winit contributors, from crates.io; the change is listed in `vendor/README.md` | Apache-2.0 |
| Sample drawing | `sample.tdraw` | Made for TraceDraw | MIT OR Apache-2.0 |
| Hidden group CDR27 parser fixtures and generator (#2850) | `crates/cdr/tests/fixtures/hidden-groups/` | Made for TraceDraw; original rectangles and generated chunk records, no private drawing data | MIT OR Apache-2.0 |
| Layer visibility CDR27 parser fixtures and generator (#2862) | `crates/cdr/tests/fixtures/hidden-layers/` | Made for TraceDraw; original rectangles and generated chunk records, no source drawing data | MIT OR Apache-2.0 |
| Picture import test files (a progressive RGB JPEG, a progressive CMYK JPEG, a palette PNG; a few flat colours each) | `apps/tracedraw/testdata/` | Made for TraceDraw's tests with a general-purpose imaging library | MIT OR Apache-2.0 |
| Screenshots | `docs/images/` | TraceDraw itself, captured with `scripts/visual/readme_shots.sh` from drawings made for TraceDraw by `apps/tracedraw-cli/examples/showcase.rs`; their text is set in Inter and Lora (SIL Open Font License 1.1), fonts installed on the capture machine and not bundled | MIT OR Apache-2.0 |

Not bundled on purpose: licensed spot colour libraries, third-party
clipart, templates, fonts other than the ones above, and colour profiles
other than the ones a user loads. Test files that belong to someone else
are never committed.
