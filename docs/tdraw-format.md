# The `.tdraw` format

A `.tdraw` file is the drawing's `Document` as JSON (`Document::to_json`
in `crates/core/src/document.rs`), with `format_version` at the top
level. Fonts the drawing carries are embedded in it.

From version 2 (TraceDraw 0.47) the file is packed (#2849,
`Document::write_tdraw_version`):

- Each distinct picture and font is written once, as base64 text in the
  list `blobs` at the end of the JSON; every object that holds it has
  `"blob:N"` (entry N of the list) in place of the bytes. Copies,
  symbols, an effect's original and its result, a pattern on many
  shapes all share one entry.
- The whole JSON is gzip-compressed: the file starts with gzip's
  `1f 8b`. The extension stays `.tdraw`. TraceDraw 0.46 and older do
  not open it (their "Could not open" message is a generic one).
- A picture imported from a JPEG file, whose pixels are exactly that
  JPEG's, is written in the table as the JPEG (`"jpg:<base64>"`), not
  as PNG (two to three times larger for a photo). Reading decodes it
  back into the same pixels (`jpeg_source.rs`). A picture whose pixels
  changed is written as PNG.

Every reader (`document::tdraw_text`: the app, the thumbnailer, the
command line, templates, recovered backups) takes both kinds: gzip is
decompressed, anything else is read as the plain JSON of every earlier
version, and `blobs` is optional. GitHub (File > GitHub) still writes
plain JSON, which diffs as text.

A file of 8 MB or more opens on a worker thread (`opening.rs`); after
half a second a window shows how much of it is read, then that the
drawing is being built, with Cancel.

Groups inside groups: a file may nest 16,384 brackets deep
(`MAX_JSON_DEPTH`), some 5,000 groups inside groups, and a drawing nested
more than 30 deep is written and read on a thread with room for it
(`on_deep_stack`). The limit was 1024 brackets, some 330 groups, which
the Group command goes past: the drawing saved and then refused to open.
A drawing nested past the limit is not saved; the status line says it
nests too deep to open again, and the file on disk stays as it was
(`document::tests::deeply_nested_groups_open`).

## Versions

| Version | First written by | Change |
|---|---|---|
| 0 | TraceDraw 0.4 | The layout of the drawing. Files from 0.4 have no `format_version`; from 0.10 on it is written as 0. |
| 2 | TraceDraw 0.47 | The same layout, packed: pictures and fonts once in `blobs`, JPEGs kept, the file gzip-compressed (#2849). TraceDraw 0.46 and older do not open it. |

TraceDraw 0.5 to 0.9 wrote `format_version: 1` for the layout of 0; it
is read as 0 and never offered, so those files are never mistaken for
a later version.

Save As offers the packed versions only (today just v2, so there is
nothing to choose), and Save writes a plain file opened from an older
release packed. The plain versions stay readable; `to_json_version`
still writes them for the clipboard and GitHub (`DOWN_STEPS[i]` takes
version `i + 1` to `i`).

## The promise

- Every `.tdraw` file TraceDraw has written opens in every later
  TraceDraw. A file of an older version is brought up to date on
  reading, one step per version (`migrate`; `STEPS[i]` takes version `i`
  to `i + 1`).
- A new field always comes with a default, so files without it read as
  before; adding one does not raise the version.
- A field renamed, moved or given another meaning raises
  `TDRAW_FORMAT_VERSION` and adds the step that rewrites older files.
- A file from a newer TraceDraw opens as far as this one knows it: fields
  it does not know are left out (saving it again drops them).

## Checks

- `document::tests::tdraw_files_carry_their_format_version_and_older_ones_open`
- `document::tests::tdraw_files_can_be_written_in_an_older_format_version`
- `document::tests::tdraw_files_are_packed_with_each_picture_once`
- `opening::tests::a_large_file_is_read_on_a_worker_and_can_be_cancelled`
- `document::tests::deeply_nested_groups_open`
