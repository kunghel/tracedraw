# Attribution

Every asset bundled with TraceDraw, with its author, source and license.
Code dependencies (Rust crates) keep their own licenses, recorded in
their packages; `cargo tree` lists them from `Cargo.lock`.
Fonts embedded in PDF files are subset with the `subsetter` crate
(MIT OR Apache-2.0); the fonts themselves are the user's and are put in
only when their licence bits allow it. The desktop builds compile in
libwebp (Copyright Google Inc., BSD 3-Clause licence, through the
`libwebp-sys` crate) for lossy WebP export.

| Asset | Path | Author and source | License |
|---|---|---|---|
| Selawik Regular and Bold (interface font) | `apps/tracedraw/assets/fonts/` | Copyright 2015 Microsoft Corporation, the open-source Selawik project | SIL Open Font License 1.1, see `Selawik-OFL.txt` next to the fonts |
| Quicksand (variable, test font) | `crates/text/testdata/` | Copyright 2011 The Quicksand Project Authors, from the google/fonts collection | SIL Open Font License 1.1, see `Quicksand-OFL.txt` next to the font |
| Fonts of the browser build (bundled into the module and placed in the published page's font folder when it is built; not stored in this repository) | `web/fonts.txt`, `apps/tracedraw/build.rs` | Liberation, DejaVu, Noto, TeX Gyre and the google/fonts families listed in `web/FONTS.md` | Their own free licences (SIL Open Font License 1.1, Apache License 2.0, Ubuntu Font Licence 1.0, GUST Font License, Bitstream Vera), copied into the font folder's `licences/` |
| Application icon (PNG sizes and `.ico`) | `assets/icon/` | Made for TraceDraw | MIT OR Apache-2.0 |
| Microsoft Store tiles | `packaging/windows/msix/Assets/` | Made for TraceDraw from the application icon | MIT OR Apache-2.0 |
| Tool, toolbar and panel icons | painted in code, `apps/tracedraw/src/ui/icons.rs` | Made for TraceDraw | MIT OR Apache-2.0 |
| Default colour palette (colour names and values) | `apps/tracedraw/src/palette.rs` | TraceDraw contributors; common process colour tints with descriptive names | MIT OR Apache-2.0 |
| Spell-check core word list | `apps/tracedraw/lang/core-words.txt` | Made for TraceDraw | MIT OR Apache-2.0 |
| Hyphenation patterns | `crates/text/src/hyphen.rs` | Made for TraceDraw (a small Liang-style set) | MIT OR Apache-2.0 |
| Interface translations | `apps/tracedraw/lang/*.toml` | Made for TraceDraw | MIT OR Apache-2.0 |
| Sample drawing | `sample.tdraw` | Made for TraceDraw | MIT OR Apache-2.0 |
| Screenshots | `docs/images/` | TraceDraw itself, captured with `scripts/visual/readme_shots.sh` from drawings made for TraceDraw by `apps/tracedraw-cli/examples/showcase.rs`; their text is set in Inter and Lora (SIL Open Font License 1.1), fonts installed on the capture machine and not bundled | MIT OR Apache-2.0 |

Not bundled on purpose: licensed spot colour libraries, third-party
clipart, templates, fonts other than the ones above, and colour profiles
other than the ones a user loads. Test files that belong to someone else
are never committed.
