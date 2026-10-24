# Dropping files on the window

Files dragged from the desktop or a file manager (in a browser: from the
computer onto the page) and let go over the TraceDraw window open or
import, with no dialog.

## What a drop does

The window's state at the moment of the drop decides for every file let
go together:

| A drawing is shown | The file | Result |
|---|---|---|
| yes | File > Import reads it (vector formats, `.psd`/`.psb`, pictures, installed script filters) | imported into the active layer, selected, as one undo step, its middle at the middle of the drawing window |
| yes | only File > Open reads it (`.tdraw`) | opened in a tab of its own |
| no (Welcome Screen or no drawing) | File > Open reads it | opened in a tab of its own, as File > Open, or its tab shown when it is open already (#645) |
| no | a picture (`.png`, `.jpg`, `.jpeg`, `.bmp`, `.gif`, `.webp`, `.tif`, `.tiff`) or a script filter's format | a new drawing with the last used new-document settings, the file imported in the middle of its page |
| either | anything else | nothing; the status bar says the file cannot be opened or imported |

Several files dropped on the Welcome Screen open side by side, one tab
each. Imports go in the middle of the view because the windowing layer
does not report where the pointer let go; File > Import's placement
option (Options > Global > File Formats) is not used for drops.

## While files are dragged over

The window gets a 3 px accent frame over a light accent tint and, in the
middle, a label with what a drop would do: "Drop to import into the
drawing" with a drawing shown, "Drop to open" otherwise.

## Browser build

The browser hands over the bytes, not a path: they are read in the
background, kept under `/upload/<file name>` like a file picked in a
dialog, and opened or imported on a later frame.

## Checks

- `drop_files::tests` drop real `.svg`, `.png` and unknown files through
  egui's input: an import lands centred in the view and one undo removes
  it; without a drawing the `.svg` opens in a new tab and the `.png`
  gets a new drawing with the picture centred on the page; two files on
  the Welcome Screen give two tabs; an unknown file only sets the status.
