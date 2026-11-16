# Table of Contents

A table of contents lists the headings of a drawing with the numbers of
the pages they are on, for books, catalogues and magazines (#1660). It
stays up to date by itself.

## Headings

- Text > Table of Contents > Heading 1, Heading 2 and Heading 3 mark the
  selected texts as headings at that level; Not a Heading takes the mark
  off. Each is one undo step (Table of Contents Heading,
  `Command::SetTocLevel`). The menu ticks the level the selected texts
  share.
- The mark is a property of the text (`Shape::toc_level`), saved in
  `.tdraw` files only while set. Artistic and paragraph text both take
  it; a text inside a group counts too. A table of contents is never a
  heading itself.
- A heading is listed by its first line that holds anything, with runs
  of spaces made one (`toc::heading_text`).

## Inserting one

- Text > Table of Contents > Insert Table of Contents puts a paragraph
  text on the current page, in the active layer, filling the page
  inside its margins (Document Options > Layout > Margins), or 20 mm in
  from the edges when the drawing has none. It is selected.
- Its first line is the title Contents, in the current text font at
  12 pt. A right tab at the frame's right edge with a dot leader puts
  each page number at the right, dots before it
  (`toc::tests::the_table_of_contents_lists_the_headings_marked`).
- The table of contents is a property of the text (`Shape::toc`, its
  title and the levels it lists, three), saved in `.tdraw` files.

## What it lists

- Every heading of the drawing in reading order: page by page, and on a
  page from the top down, then left to right (paragraph text by the top
  of its frame, artistic text by its first baseline). Hidden texts,
  hidden layers and master layers are left out.
- One line per heading: indented four spaces a level below level 1,
  then the heading, a tab and the page number.
- The page numbers are those the drawing's page number field shows: the
  first page number field on a master layer gives the first number, the
  first numbered page, the style (1, I, i, A, a) and the text around it;
  without one, pages count from 1
  (`toc::tests::the_table_of_contents_follows_the_headings_and_pages`).
- With no heading, the title alone shows.

## Kept up to date

- The list is worked out each time the page is drawn, printed or
  exported (`Document::layers_drawn_for_page`, like the page number
  fields), so it follows headings typed, moved to another page, marked,
  unmarked or deleted, and pages inserted, deleted or reordered, with
  nothing to update by hand. Every export format and printing show it
  as it stands.
- The text saved in the file is what was there when it was inserted;
  typing into the table of contents has no lasting effect, since the
  list replaces it when drawn. Its font, size, colour, frame and tabs
  can be changed like any paragraph text's: the list takes the style of
  its first run of text.
