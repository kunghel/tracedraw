# About box and start-up

## About TraceDraw

Help > About TraceDraw opens the About box from the Welcome Screen as well
as from a drawing. It is a chrome dialog (white title bar with the close
button) 480 points wide:

- A banner across the top: a diagonal gradient from `#244475` (top left)
  through `#1A3158` (top right) to `#0E192D` (bottom), faint light blue
  (`#5CAEFF`) curves on the right with one selected node (a square node,
  two handles along the tangent, round handle ends), the 76 px
  application icon, the name in the bold face at 29 px ("Trace" white,
  "Draw" `#7FC3FF`), the tagline and the version in a rounded chip,
  then a green ALPHA badge whose tooltip says the program is at an
  early stage: most tools work, some features are missing or
  incomplete, keep copies of important files, report problems on
  GitHub.
- The description at 13.5 px with 20 px lines, a light separator, then
  the facts as two columns with dim labels: Author, License, Source code
  (a link that opens the repository), Developed with.
- The copyright line in small dim text and a Close button (Enter or Esc).

## Splash screen

While TraceDraw starts, a borderless 640 x 380 point window centred on
the screen shows the same artwork as the About banner at a larger size
(two selected nodes on the main curve), the 96 px icon, the name at
46 px with the tagline, and a darker strip along the bottom with the
status line, a thin progress bar and the footer (copyright on the left,
the version on the right).

- The bundled interface font is installed at once; the system font list
  (and the system fonts for Chinese, Japanese, Arabic, Hindi and Bengali)
  is read on a worker thread. Texts in those languages appear once their
  fonts are in; the name and version always show.
- Status: "Loading fonts..." while the thread works, then "Opening
  <file>..." for a file named on the command line, else "Preparing the
  workspace...".
- The bar eases towards 80 % over 1.6 s while the fonts load and fills
  once they are in. The splash stays up at least 1.6 s.
- Then the editor is built and the window becomes the main window:
  decorations, resizable, the maximize button, a minimum of 800 x 500
  (never more than four fifths of the monitor, so it fits a 1366 x 768
  or 1280 x 720 screen with a taskbar at any interface size), the size
  kept from the last session (1600 x 950 the first time, never larger
  than 95 % of the monitor) centred on the monitor, maximized when it
  was left maximized or when that size does not fit the monitor (a
  small screen).
- The splash window is made resizable too, though it has no frame to
  drag: a window made not resizable stays at its size for good under
  some window managers (Xfwm4), and the main window could then neither
  be resized nor maximized
  (`splash::tests::small_screens_get_a_window_that_fits_and_starts_maximized`,
  `splash::tests::the_main_window_offers_the_maximize_button`).
- Options > General > Show the splash screen at start-up turns it off.
  `--no-splash` or `TRACEDRAW_NO_SPLASH=1` skip it for one run and open a
  plain 1600 x 950 window (automation and the visual checks use this).
- The browser build has no splash: the page shows its own loader.

## One window for every start (#646)

A file opened from the file manager, a shortcut or the command line while
TraceDraw runs opens in a tab of the running window (or shows its tab
when it is open already, see `documents.md`), which comes to the front;
the new start ends at once, before its log would replace the running
one's (`instance.rs`). Several files selected together all go there (the
Linux launcher passes them to one start, `Exec=tracedraw %F`). A start
with no file, one with `--new-instance`, and the `--mcp`, `--mcp-window`
and `--reset-settings` starts open windows of their own.

- The running window holds a lock on `instance.lock` in the settings
  folder for as long as it runs; the system lets go of it when the
  process ends, a crash included, so nothing stale is left in the way.
- It listens on a port of this computer (127.0.0.1) named, with a random
  token, in `instance.json` (readable by this user only on Linux and
  macOS). A start sends one line `{"token":…,"files":[…]}` with absolute
  paths and the window answers `{"ok":true}`; a wrong token or garbage
  reaches nothing. A start waits up to three seconds for a window that is
  just starting, then opens its own.
- The window takes the files between frames from a channel (one
  `try_recv` a frame when none came), waiting while a dialog or a question
  is up or the drawings are being closed. Wayland does not let a window
  raise itself: it asks for attention instead.
- A start with the next renderer after a failed one (see below) takes the
  place over. Portable copies and `TRACEDRAW_CONFIG_DIR` each have their
  own window. The browser build has none of this.
- Checks: `instance::tests::a_second_start_hands_its_files_to_the_first`,
  `a_crash_leaves_nothing_in_the_way`, `strangers_are_refused`,
  `starts_at_the_same_time_make_one_window`,
  `a_handed_over_file_opens_in_a_tab_once_no_question_is_up`,
  `the_linux_launcher_starts_once_for_several_files`; checked by hand on
  X11 with two starts (the second ended in 12 ms, its file in a new tab of
  the first window).

## The renderer (#445, #439)

- The desktop window is drawn with wgpu (Direct3D 12, Vulkan or Metal).
  Options > General > Use hardware acceleration (on) picks the graphics
  card named beside it, or with Fastest available a separate card before
  a built-in one, a virtual one, others, and a software adapter last.
  Off, the software adapter (WARP on Windows, a CPU Vulkan driver
  elsewhere). Options shows what draws the window in this run; the
  choice takes effect at the next start.
- A start that cannot open the window that way runs TraceDraw again
  with `--renderer=software`, and if that fails too with `--renderer=gl`
  (OpenGL, as earlier versions drew), the other arguments kept; the
  first process waits for it. The renderer that opened the window when
  the first choice did not is kept in the settings (`working_renderer`)
  and used straight away next time; changing the choice in Options
  clears it (`gpu::tests::a_renderer_that_fails_hands_over_to_the_next`).
  So a virtual machine without a usable graphics card opens.
- The log goes to `tracedraw.log` in the settings folder as well as to
  the console (written afresh each start; a start handed over to the
  next renderer adds to it). A panic is written to `crash.log` there and
  shown in a message box naming that file, once; the Windows build has
  no console, so a start that failed showed nothing (#570,
  `startup_log::tests::the_log_and_the_crash_report_go_in_the_settings_folder`).
- The Windows builds link the C runtime statically (`.cargo/config.toml`,
  `+crt-static`): they do not need `vcruntime140.dll` (#570).
- The browser build draws with WebGL.

## Back to the defaults

Pressing F8 while the splash screen shows asks, once the editor is up,
whether to reset every preference to how TraceDraw comes (Options >
Global > Reset all settings... asks the same). Reset keeps the recent
drawings, the last folder and the window's size; Cancel changes
nothing. `tracedraw --reset-settings` resets without asking (scripts,
support). Drawings are never touched.

- `splash::tests::f8_while_starting_asks_to_reset_the_settings`
- `settings::tests::a_reset_keeps_history_and_nothing_else`
- `app::tests::resetting_the_settings_brings_back_the_defaults`

## Window state

The main window's state is kept in the preferences on exit: maximized or
not, and its inner size while it was neither maximized nor minimized.

## Checks

- `ui::about::tests::the_about_box_draws_on_the_welcome_screen`
- `ui::brand::tests::the_embedded_icon_decodes`
- `ui::brand::tests::cubic_helpers_follow_the_curve`
- `ui::brand::tests::the_main_curve_spans_the_right_part`
- `splash::tests::progress_waits_for_the_fonts_and_the_minimum_time`
- `splash::tests::the_status_names_the_file_being_opened`
- `splash::tests::the_main_window_keeps_a_sensible_size`
- `splash::tests::the_main_window_is_centred`
- `splash::tests::a_splash_frame_draws_and_then_starts_the_editor`

## Recent drawings on the Welcome Screen

Get Started lists the drawings opened or saved last (ten at most), each
with a thumbnail of its first page, its name and its folder; a click
opens it, a drawing whose file is gone is greyed out, and Clear list
forgets them.

- The thumbnail is made when a drawing is opened or saved: the first
  page rendered with a white background, 160 pixels on its long side,
  kept as PNG in the preferences folder under `thumbnails/`, named by a
  hash of the drawing's path (FNV-1a, the same from one run to the next).
  Saving again makes it again.
- Clear list removes the thumbnails too. The browser version records no
  recent drawings, so it makes no thumbnails.
- `thumbnails::tests::opened_and_saved_drawings_get_a_thumbnail`

