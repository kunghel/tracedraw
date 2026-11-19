# Dropping files on the window

Files dragged from the desktop or a file manager (in a browser: from the
computer onto the page) and let go over the TraceDraw window open or
import, with no dialog.

## What a drop does

Where the files are let go decides for every file let go together:

| Let go over | The file | Result |
|---|---|---|
| the drawing window of the drawing shown | File > Import reads it (vector formats, `.cdr`, `.tdraw`, `.psd`/`.psb`, pictures, installed script filters) | imported into the active layer, selected, as one undo step, its middle at the middle of the drawing window |
| the title, menus, toolbars, document tabs or dockers, with a drawing shown | File > Open reads it (`.cdr`, `.svg`, `.tdraw`...) | opened in a tab of its own, as File > Open (#1142) |
| the same | a picture or a script filter's format | a new drawing with the file imported in the middle of its page |
| anywhere, with no drawing shown (Welcome Screen or no drawing) | File > Open reads it | opened in a tab of its own, as File > Open, or its tab shown when it is open already (#645) |
| the same | a picture (`.png`, `.jpg`, `.jpeg`, `.bmp`, `.gif`, `.webp`, `.tif`, `.tiff`) or a script filter's format | a new drawing with the last used new-document settings, the file imported in the middle of its page |
| anywhere | anything else | nothing; the status bar says the file cannot be opened or imported |

The place decides the same way on every platform (#1142). A system drag
sends the window no pointer moves, so: on Windows the cursor's place is
read from the system; on Linux the patched windowing layer turns each
XDND position message (X11), or each enter and motion of the Wayland
data device, into a cursor move over the window, and on
macOS each `draggingEntered:`, `draggingUpdated:` and
`performDragOperation:`; in the browser the patched eframe turns each
`dragover` and the `drop` into a pointer move (`vendor/README.md`).
Should the place still be missing, a picture is imported into the drawing
shown, and a drawing, which could go either way (`.cdr`, `.svg`,
`.tdraw`...), is asked about in a small window in the middle: "Import into
the drawing", "Open in its own tab" or Cancel (Esc).
On a native Wayland window (#1142) the patched windowing layer takes
the drag itself, which winit 0.30 did not (no file drop reached the
window, and the file manager showed the drag as refused; `--x11` was the
way round it): a drag offering a list of files (`text/uri-list`) is
accepted as a copy and its list read at once, so the hint knows the
files while they hover; the drop then gives the files, and the window
tells the file manager the copy is done. Over the window's frame (its
title bar drawn by TraceDraw's windowing layer) the place counts as
outside the drawing window, so a drawing opens. Only local `file:`
addresses are taken.

Several files dropped on the Welcome Screen open side by side, one tab
each. Imports go in the middle of the view, not under the pointer;
File > Import's placement option (Options > Global > File Formats) is
not used for drops.

## Pictures dragged out of other programs (#2349)

A picture dragged out of another program's canvas or page (a paint
program, a browser) comes as the picture itself, not as a file. The
windowing layer takes it as PNG, else as a bitmap (Windows: the
registered `PNG` or `image/png` formats, then `CF_DIBV5` or `CF_DIB`;
X11 and Wayland: `image/png`, `image/bmp`, `image/jpeg`), writes it to a
file of its own in the system's temporary folder (`tracedraw-dropped-pictures`)
as the drag enters, and hands that file on like a dropped picture file:
over the drawing it goes into it, elsewhere it gets a new drawing. The
picture comes in without a name and without a link to that file
(`vendor/winit/src/picture_drop.rs`; checks
`winit_picture_drop::tests`,
`clipboard::linked_watch_tests::a_picture_dragged_in_from_another_program_has_no_link`).

## While files are dragged over

A 3 px accent frame over a light accent tint goes round where the files
would land: the drawing window when they would be imported there, the
whole window otherwise. In its middle a label says what a drop would do:
"Drop to import into the drawing" or "Drop to open". Over the drawing
window a file no import reads says "Drop to open" with the frame round the
whole window; in the browser, which names no files while they are dragged,
the place alone decides
(`drop_files::tests::the_hint_says_what_a_drop_there_does`). A `.tdraw`
over the drawing window is imported like a `.cdr`
(`a_tdraw_dropped_on_a_drawing_is_imported_into_it`, #1142). On Windows
the frame shows only while a mouse button is held: a drag taken away
without a drop could leave the files listed as hovered, and the frame
then stayed over the window until TraceDraw was closed
(`drop_files::drag_held`, #2350). The drop itself, or the drag leaving,
can also fail to reach the program on Windows: the files stayed listed,
nothing was imported, and the frame came back at every later click or
drag, as if TraceDraw were still waiting for the drop (#2686). Once every
button has been up for half a second with the same files still listed,
the drag is over: the files count as let go where the cursor is (inside
the window, as any drop there; outside it nothing happens), and the frame
stays away until the next drag. Should the drop come in after all, it is
not taken twice (`drop_files::FileDrag`,
`a_file_drag_whose_drop_is_lost_ends_once_the_buttons_are_up`).
The files of such a lost drop stay listed, ahead of the next drag's own,
until a drop or a leave does reach the program; they belong to a drag
that is over and are not let go again, nor do they count for the hint.
Before, a file dropped and then deleted came back with every later drop
until TraceDraw was restarted (#2940, `FileDrag::fresh`,
`a_lost_drops_files_still_listed_do_not_come_back_with_the_next`).
Minimizing the window and bringing it back does not let them go again
either. The input kept while the window was minimized appended the list
of hovered files to itself at every frame, so on restore each file was
listed many times, the copies counted as a new drag, and a file dropped
once was imported again at every restore until TraceDraw was restarted.
The kept input now takes the latest list whole (#3058, patched eframe,
`eframe_pending_input::tests`).

## Browser build

The browser hands over the bytes, not a path: they are read in the
background, kept under `/upload/<file name>` like a file picked in a
dialog, and opened or imported on a later frame.

## Checks

- `drop_files::tests` drop real `.svg`, `.png` and unknown files through
  egui's input: over the drawing window an import lands centred in the
  view and one undo removes it; over the menus the `.svg` opens in a new
  tab and the `.png` gets a new drawing; a drag's place reported as
  pointer moves decides between import and open
  (`the_place_of_a_drag_decides_on_every_platform`); with the pointer
  unknown the `.png` is imported and the `.svg` is asked about, then
  imported, opened or left as answered; without a drawing the `.svg` opens in a new tab and the `.png`
  gets a new drawing with the picture centred on the page; two files on
  the Welcome Screen give two tabs; an unknown file only sets the status.
- `winit_file_drop::tests` (the Wayland part of the patched windowing
  layer, `vendor/winit/src/file_drop.rs`): file manager lists with
  escapes, comments, other hosts and schemes; hover then drop sends the
  place and the files; a drop before the list is read waits for it; a
  drag that leaves cancels the hovered files; a list read empty is read
  again at the drop. Checked by hand on a headless Wayland compositor
  (2026-11-03): a file dragged from another program's window opened on
  the Welcome Screen, was imported over the drawing window ("Drop to
  import into the drawing" while it hovered), opened over the dockers
  ("Drop to open"), and a drag taken back out left nothing behind.
