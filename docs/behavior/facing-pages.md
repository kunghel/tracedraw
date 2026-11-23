# Facing Pages

Facing pages show the pages of a drawing side by side in spreads while
drawing, as a book or magazine opens (#1972, #1660). Before, spreads
existed only in print imposition.

## Turning it on

- Page > Facing Pages turns it on and off; Page > Facing Pages Start On
  chooses Right Side or Left Side (choosing a side also turns facing
  pages on).
- Document Options > Layout has the same choices: Facing pages and
  Start on: Right side / Left side.
- Each change is one undo step (Document Properties). The setting is a
  document property (`Metadata::facing_pages`, `Metadata::facing_start`)
  saved in `.tdraw` files only while on, so older drawings read and
  write as before (`spreads::tests::facing_pages_round_trip_in_the_file`).

## Spreads

- Start on Right side (the default): page 1 stands alone on the right,
  then pages 2-3, 4-5 and so on pair up; the last page stands alone on
  the left when the count is even.
- Start on Left side: pages 1-2, 3-4 and so on.
- The spreads follow the page order alone, so inserting, deleting,
  duplicating or moving pages reflows them
  (`spreads::tests::adding_and_removing_pages_reflows_the_spreads`).
- The two pages of a spread touch at the spine with their tops lined up.

## Working

- The desktop shows the current page's spread: the facing page beside
  the current one at the spine. The current page is edited as always;
  rulers, grids and the status bar refer to it.
- Objects belong to one page. An object drawn across the spine belongs
  to the page it was started on and reaches over onto the facing page,
  where it shows. The facing page's objects that reach over the spine
  show on the current page too, beneath the current page's own.
- A press on the facing page makes it current before the tool acts,
  nothing moving on screen, as in the Multipage View. A press on an
  object of the current page keeps the page even where the object lies
  over the facing page, so an object across the spine is picked
  anywhere on it; a press on the current page where only an object of
  the facing page lies makes that page current
  (`multipage::tests::the_facing_page_becomes_current_where_it_is`).
- Objects of both pages selected together (#3488): with something
  selected, a Shift+click on an object of the facing page brings that
  object onto the current page, where it shows (on the current page's
  layer of the same name, else the active layer), and adds it to the
  selection; a marquee brings the facing page's objects it takes the
  same way, so it selects across the spine. Then they align,
  distribute and move together. One undo puts each back on its page
  (`multipage::tests::objects_of_both_facing_pages_select_together`).
  Selecting does not change the page an object prints on (#3817): once
  an object brought over leaves the selection (another selection, a
  click on nothing, another page made current), it goes back to its
  page and layer where it shows, in one undo step; one moved onto the
  current page while selected (its middle on that page) stays there
  (`multipage::tests::objects_selected_across_the_spine_go_back_to_their_page`).
  In the Multipage View an object let go with its middle on another
  page's paper, the facing page included, becomes that page's (#3847,
  see `multipage-view.md`).
- The page tabs, the navigator and Go To Page make the facing page
  current in place; a page of another spread is shown with its spread,
  fitted to the window.
- Zoom to Fit and turning facing pages on fit the spread.
- Guidelines: the facing page's show within it, the current page's
  across the window, as in the Multipage View. Snapping and alignment
  guides take the facing page and its objects too
  (`multipage::tests::the_spread_keeps_guides_snapping_and_drawing`).
- View > Bleed draws the bleed around the whole spread, not at the
  spine (#1660). Mask Outside Page leaves both pages uncovered.
- Master layers for odd and even pages (Document Options > Layout) give
  right and left pages their own master objects: with Start on Right
  side odd pages are right pages.

## Margins

- Document Options > Layout > Margins (#1660): Top, Bottom, Inside and
  Outside, in the document's units, the box the text and pictures of
  every page keep inside. Ticked, the margins start at 12.7 mm all
  round; unticked, the drawing has none. Each change is one undo step
  (Document Properties); the margins are a document property
  (`Metadata::margins`), saved in `.tdraw` files only while set.
- Each page shown, the facing page and the pages of the Multipage View
  too, draws the margins as a dashed violet frame over the drawing,
  like the guidelines; a frame is drawn only while a margin is above 0.
- With facing pages the inside margin lies at the spine and the
  outside one at the page's free edge, so the two swap sides between
  left and right pages: on the right of a left page, on the left of a
  right page. A page shown alone takes the inside margin on its left,
  as a right-hand page (`spreads::tests::page_margins_mirror_at_the_spine`,
  `multipage::tests::page_margins_show_on_both_pages_of_the_spread`).
- Margins wider than the page leave a frame of no width there; values
  below 0 or above 1000 mm are kept in range.

## Multipage View

With facing pages the Multipage View lays the spreads out two a row
(four page columns), `page spacing` apart, the pages of a spread
touching. Every slot is as wide as the widest page, so a page added
never moves the pages before it
(`multipage::tests::facing_pages_sit_in_spreads_touching_at_the_spine`).

## Export and printing

- Export works on pages as before, one page per PDF page or per file.
  With facing pages each page also carries what its facing page holds
  across the spine and within the bleed beside it, so a picture across
  the spine prints whole over the two pages.
- Export > Spreads (shown with facing pages): each spread of the chosen
  pages becomes one page as wide as its pages side by side; a page
  alone stays a page of its own. The master layers, page backgrounds
  and page number fields are drawn into each half. A PDF holds a page
  per spread; formats of one page write a file per spread
  (`export::tests::facing_pages_export_spreads_or_pages_with_the_spine_crossing`).
- Printing prints pages as before, each with what reaches over the
  spine onto it, with Fit to page too (#2678); not for the selection
  alone.
  Booklet and other impositions work on these pages.

## `.cdr` files

A `.cdr` book on facing pages (version 21 and later, `mcfg` byte 26,
see `docs/cdr-format.md`) opens with Facing Pages on, in spreads. Its
objects are measured from the spine of their spread, so each page's
objects move half a page onto the page by the page's place in the
spreads: pages take turns on the right and left of the spine. The file
does not say which side the first page is on; of Start on Right side
and Left side, the one that puts more of all the pages' objects on
their pages is taken, and every page then goes by it, so an object a
left page reaches across the spine stays at the spine. Each page took
the side that put most of its own objects on it before, and the pages
showed one by one: a book opened with pages mixed and no spreads
(#1660, `parse::tests::a_book_on_facing_pages_opens_in_spreads`).

The turns may break where the pages after hold their objects better
the other way (#2833): each page scores the share of its objects that
lands on it from the right of the spine and from the left, a break (two
pages running on one side) costs 1.5 pages' worth, and the sides with
the most score are taken (Viterbi's way through the pages,
`parse::facing_sides`). One page whose objects reach across the spine
keeps its turn (two breaks cost 3, more than its 1); a page the file
holds more or less in a long book no longer throws every page after it
half a page across, onto the page beside it. On a tie the first page is
on the right and the turns go on
(`parse::tests::facing_sides_take_turns_and_follow_a_lost_page`,
`parse::tests::a_long_book_whose_turns_break_keeps_every_page_on_its_side`:
300 pages whose sides change at page 151).

A large file's record stream (`content/riffData.cdr` in version 14 and
later packages) is read as far as the package declares it, up to 3 GB,
as the data streams are: it was cut at 512 MB, so in a book of some 300
pages in a 1 GB file the pages after the cut were lost and the last one
read was cut short (#2833). A member that inflates past what it
declares still stops at 512 MB or that size, whichever is larger.

Nothing is written for facing pages: a `.cdr` saved from TraceDraw has
single pages, each object where it shows on its page.

## Not done

- An advanced booklet window asked for in #1660 is a separate
  feature. The table of contents asked for there is Text > Table of
  Contents (see `table-of-contents.md`).
- The Page Sorter shows pages one by one, not in spreads.

## Formula

With `lead = 1` for Start on Right side and `0` for Left side, page `i`
(0-based) is in spread `(i + lead) / 2`, on the left when `i + lead` is
even, else on the right. In the Multipage View spread `k` is in row
`k / 2`, column `k % 2`; with `w` the widest page, the spine of column
`c` is at `c * (2w + gap) + w`; a left page ends at the spine and a
right page starts there; rows are as tall as their tallest page, `gap`
apart, pages top aligned.
