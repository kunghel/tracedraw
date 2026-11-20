# Insert Object

Object > Insert > Insert Object (#1025) keeps a list of programs the
user adds once (an equation editor, a chart maker, any program that
copies or saves pictures) and opens each with one click. What the
program makes comes in as any picture or drawing does: through the
clipboard or a saved file. TraceDraw does not embed a live object of
another program; the drawing keeps what was pasted, and a change is
made in the program and pasted again.

## The submenu

- The programs, by name: a click starts that program. The status bar
  says it is open and how its work comes in. A program that cannot be
  started says why.
- Paste Object: pastes what the clipboard holds, like Edit > Paste
  (pictures, SVG, PDF and the other formats Paste takes).
- Paste Special...: the Paste Special dialog, to pick the format.
- From File...: File > Import, for a program that saves files.
- Add Program...: picks a program (an executable; on macOS an
  application bundle, started with `open -a`). It is named after its
  file without the extension and kept in the settings for every
  drawing. The same program is not added twice; at most 30 are kept.
- Remove Program: the programs again; a click removes that one.
- Insert What It Saves (a check mark, off by default, kept in the
  settings, #3391): a program then opens on a blank white picture
  (800 x 600 pixels, a PNG in the system's temporary folder, named
  after the program). Nothing comes in until the program saves it
  (Ctrl+S there); then the saved picture comes in at the page centre,
  selected and linked to its file, and every later save there updates
  it in the drawing, as an edited bitmap's file does (checked once a
  second). A program that cannot open a PNG says so; its work still
  comes in through Paste Object.

In a browser and on Android there are no programs to start: the
submenu holds Paste Object, Paste Special and From File.

## Checks

- Given `/opt/Formula/Formula.exe`, when it is added, then it is named
  Formula and starts as that program with no arguments
  (`insert_object::tests::programs_are_named_after_their_file`).
- Given Equations, Charts and Equations again added, then the list is
  Equations, Charts; opening Charts names it in the status bar; removing
  the first leaves Charts
  (`insert_object::tests::programs_are_added_once_opened_and_removed`).
- Given Insert What It Saves on, when a program is opened, then it is
  handed a blank 800 x 600 picture; nothing comes in until the picture's
  file changes, then it comes in once, selected and linked to the file
  (`insert_object::tests::what_the_program_saves_comes_into_the_drawing`,
  `insert_object::tests::the_blank_picture_is_handed_to_the_program`).
