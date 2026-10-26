# PDF and AI import

File > Open and File > Import accept `.pdf` and `.ai` (PDF-compatible
AI files, which carry a PDF after a PostScript preamble). Open
makes one page per PDF page; Import places the first page's objects on
the active layer.

## What is read

- Page geometry: MediaBox and CropBox (the crop box becomes the page),
  /Rotate 90, 180 and 270. Points become millimetres, the crop box's
  bottom-left corner is the origin.
- Paths: all construction operators, fill rules (even-odd is recorded as
  object data), strokes with width, caps, joins and dashes scaled by the
  CTM, constant alpha from ExtGStates (`ca`, `CA`).
- Colour: DeviceGray/RGB/CMYK, ICCBased (by component count), Indexed,
  Separation and DeviceN (as tints), CalRGB/Lab (as RGB).
- Clipping: `W`/`W*` paths become clip frames around the objects they
  clip; the intersection of nested clips is approximated by the inner
  one (or the shared rectangle). Objects fully inside a clip are not
  wrapped; text that is at least 60 % inside is left unclipped because
  substitute fonts change its width.
- Form XObjects (recursively, 12 levels), annotation appearance streams
  (stamps, form fields; links and popups skipped).
- Images: Flate/LZW/raw samples at 1, 2, 4, 8 and 16 bits with Gray, RGB,
  CMYK and Indexed spaces, Decode arrays, image masks (filled with
  black), SMask soft masks (resampled nearest), DCTDecode through the
  JPEG decoder. JPX images become a grey box. Inline images (`BI`) are
  read with their abbreviated keys expanded.
- Shadings: axial (type 2) and radial (type 3) as gradient fills, with
  type 2, 3 (stitching) and 0 (sampled) functions sampled into stops;
  `sh` paints the current clip. Shading patterns used as fills become
  gradient fills; tiling patterns become vector pattern fills built from
  the tile's content.
- Text: `BT`/`ET` blocks with Tf, Td, TD, Tm, T*, TL, Tc, Tw, Tz, Ts, Tr,
  Tj, TJ, ' and ". Consecutive strings on the same baseline merge into
  one artistic text object; a gap over 0.18 em (or a TJ adjustment under
  -180) inserts a space. Font family comes from BaseFont without the
  subset prefix (`Arial-BoldMT` becomes Arial, bold), bold and italic
  from the name, the descriptor flags and StemV. Characters decode
  through ToUnicode CMaps (bfchar, bfrange), then the font's Encoding
  (WinAnsi, MacRoman, Standard, Differences with glyph names), then
  Latin-1. Advances use Widths, W/DW for CID fonts, and Helvetica metrics
  for the standard 14. Render modes 3 and 7 (invisible) are skipped.
  Type 3 glyph procedures are not run.
- Text as curves (#386, `ImportOptions::text_as_curves`): each string
  (`Tj`, each part of `TJ`) becomes one filled curve of its glyphs'
  outlines, read from the embedded font program: `FontFile2` (TrueType)
  and `FontFile3` of subtype `OpenType` as sfnt fonts, `Type1C` and
  `CIDFontType0C` as bare CFF. A code reaches its glyph through the
  `CIDToGIDMap` (or the identity) for two-byte TrueType fonts, the CFF
  charset for CID-keyed CFF, the font's `cmap` ((3,0) at 0xF000 + code,
  then (1,0), then Unicode from the encoding) for simple TrueType fonts,
  and the CFF built-in encoding (else the glyph's standard name) for
  simple CFF fonts (`pdf_glyphs.rs`). Each glyph sits at
  `Trm * translate(advance) * scale(Tfs * Th, Tfs)` with the font's units
  per em (CFF: its FontMatrix); the fill colour and opacity are the
  text's, clipping as for text. Text whose font is not embedded (or a
  Type 3 font) stays text, and the app turns it into curves with the
  installed font that stands in (`remaining_text_to_curves`).
- The app asks how to read the text when a PDF or AI file is opened or
  imported (the Import PDF dialog: Text or Curves, Do not ask again);
  Options > File Formats keeps the choice (`pdf_text_as_curves`,
  `pdf_ask_text`).

## Not read

Soft masks on groups (`SMask` in ExtGState), blend modes, transfer
functions, optional content visibility, JPX and CCITT images (grey box
and skipped respectively), PostScript calculator functions (linear
approximation); as text, embedded font programs (text uses the installed
font of the same name, so widths can differ; as curves they are read).

## Protected files

Files protected by the standard security handler open in every revision
(2 to 6): RC4 with 40 to 128-bit keys, RC4 and AES-128 crypt filters,
and AES-256 (revisions 5 and 6), with strings and streams decrypted as
they are read, object streams and cross-reference streams included.

- A file that opens without a password (only its permissions are
  protected) opens at once.
- A file with a password to open it asks for it (Protected PDF: the
  file's name, a masked field, OK and Cancel); the user or the owner
  password opens it. A wrong one asks again and says so; Cancel leaves
  it closed and says why in the status bar. Import asks the same way.
- On the command line `TRACEDRAW_PASSWORD` gives the password.
- Other security handlers (certificates) are refused with a message.

How: the parser never decrypts. The trailer's `/Encrypt` key is renamed
(same length, so the offsets hold), a first read takes only the
dictionaries to find the encryption dictionary, the key comes from the
password (algorithms 2 and 6 for revisions 2 to 4: MD5 of the padded
password, `/O`, `/P` and the file ID, 50 more rounds from revision 3,
and the `/U` check; algorithm 2.A for 5 and 6: the owner's `/O` and
`/OE` first, then `/U` and `/UE`), and a second read decrypts each
object as it comes in (per-object keys from MD5 of the key, the object
number and generation, and `sAlT` for AES-128; the file key as it is for
AES-256). The members of an object stream are not decrypted again;
cross-reference streams, metadata that is not encrypted, streams with
their own crypt filter and signature contents are left alone.

## Checks

- `pdf_import::tests::protected_files_of_every_revision_open_without_a_password`:
  files protected by another implementation with revisions 2, 3, 4 (RC4
  and AES-128) and 6.
- `pdf_import::tests::our_protected_files_open_unless_they_need_a_password`
- `pdf_import::tests::object_streams_of_protected_files_are_decrypted`
- `pdf_import::tests::damaged_protection_is_an_error_not_a_panic`
- `pdf_crypt::tests::rc4_matches_the_published_vectors`,
  `what_is_encrypted_decrypts_again`,
  `revision_5_files_open_with_either_password`
- `ui::pdf_password::tests::protected_pdfs_ask_for_their_password`
- Checked by hand: files protected by an independent PDF tool in
  revisions 2, 3, 4, 5 and 6, with and without object streams, convert to
  the same SVG as the unprotected file.

- A page with `MediaBox [0 0 200 100]` gives a 70.56 x 35.28 mm page.
- `1 0 0 RG 0 0 1 rg 4 w ... B` gives a blue fill with a red 1.41 mm
  outline.
- `(Hello) Tj ( World) Tj` on one line gives one text "Hello World" at
  12 pt bold; `[(Se) -250 (cond)] TJ` gives "Se cond".
- A form XObject with a Matrix is placed by it and takes the page's
  `ca`; a rectangle painted under `W n` becomes a clip frame.
- A type 2 axial shading pattern becomes a two-stop linear gradient.
- A raw 2 x 1 RGB image gives a 2 x 1 bitmap at the CTM's rectangle,
  also when the file starts with a PostScript preamble (`.ai`).
- Garbage and truncated input return an error, never a panic.

- Devanagari text (Hindi, Marathi, Nepali, ...) read in Text mode comes
  back in the order it is typed: a PDF draws the short i sign (ि) before
  the consonant cluster it belongs to and the reph (र्) after its
  syllable, and its text kept that drawn order, so the signs went on the
  wrong letters. A run is changed only when it shows the drawn order (an
  i sign at the start of a word or after another sign); a reph moves back
  only at the end of a word (#585,
  `pdf_import::tests::devanagari_text_from_a_pdf_comes_back_in_typed_order`).
  A reph after a consonant or a sign at the end of a word also shows the
  drawn order when the run holds a reph glyph (one glyph for र् with no
  advance); a ra drawn with its virama shown at the end of a word (Nepali
  गर्, Sanskrit पुनर्) stays as it is. In drawn order a reph before the next consonant moves
  back to its syllable too (आर्थिक, प्रार्थना); a reph in a conjunct
  (after a virama) stays (#599).
- Hindi that a PDF draws apart comes in as one word (#599): a string
  that continues the syllable before it (a vowel sign, virama, nukta or
  anusvara, the rest of a conjunct, a reph glyph, the short i sign placed
  back over its letter) stays in the same text when the producer moved it
  up to an em off the baseline or up to three ems back, and no space goes
  in before it for a gap or a `TJ` adjustment; a step back for a sign and
  on again is no gap either. A consonant after a virama joins only when
  it touches (no gap of more than 0.18 em, no big adjustment before it):
  many words end in a virama (परिषद्, अर्थात्), and the next word or table
  cell stays apart (परिषद् द्वारा). Vowels drawn as two glyphs, each mapped to
  its own character, become one character again: the pairs the Unicode
  Standard lists not to use for vowel letters (अ + ा for आ, ए + े for ऐ,
  ऊ = उ + ु, the Kashmiri vowels ॳ to ॷ, ...) and a vowel sign split
  into the aa sign and a sign on top (ा + े for ो, ा + ै for ौ). Shaped as they came, these showed dotted circles.
- Marked content with an `ActualText` (inline or named in the page's
  `Properties`): its text stands for the glyphs inside it, in place of
  what their `ToUnicode` gives; a text that takes characters from one is
  in typed order and is not reordered. The outermost `ActualText` wins.
  It applies only while all its glyphs stay in one text: when they go on
  in another (the next line), each text keeps its glyphs' own characters.
  An empty `ActualText` or one of soft hyphens leaves the glyphs (a
  hyphen drawn at a line end) their own characters
  (`pdf_import::tests::hindi_drawn_apart_or_with_actual_text_comes_in_as_typed_words`).
- Hindi written by Publish to PDF with its font comes back as the typed
  words, shaped to the glyphs of the original: all 59 words of the
  Devanagari corpus (35 came back wrong before, with letters doubled,
  signs lost or a space inside the word)
  (`crates/io/tests/devanagari_pdf.rs`, a test binary of its own so the
  test font stays out of the other tests' fonts). Latin text gets no
  marked content.
