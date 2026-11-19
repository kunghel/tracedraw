# Pages

## Undo and Redo of the page shown

When Undo or Redo takes away the page shown (Undo of Insert Page or of
Duplicate Page, Redo of Delete Page), the page now at its place becomes
current, or the last page when it was the last one, and the selection is
cleared. The view no longer stays on a page that is gone, where nothing
could be drawn (
`app::tests::undo_of_the_page_shown_moves_to_a_page_that_is_there`).

## Duplicate Page

Page > Duplicate Page..., the page tab menu's Duplicate Page... and the
Duplicate Page button of the Pages panel open the Duplicate Page dialog
for the current page:

- Insert new page: Before selected page or After selected page (the
  default).
- Copy layer(s) only: the copy has the page's layers (names, visibility,
  printing and lock) with no objects in them.
- Copy layer(s) and their contents (the default): the objects are copied
  too, with fresh ids.
- OK duplicates, Cancel and Esc close the dialog, "?" opens the Help
  (`pages-layout/pages#duplicate-page`).

The copy keeps the page size, guides and background, is named after the
page with " (copy)", and becomes the current page. It is one command,
`Command::DuplicatePage { page, before, contents }`, so one undo step
removes it. A saved or scripted command that names only the page copies
it with its contents after it, as before (Data Merge works that way).

Copied with its contents, the page's linked text frames are a chain of
the copies and a blend ends at the copy of its end object, in the same
undo step: the copies named the original page's objects, so the copied
frames ran on into the original chain and the copied blend followed the
original end object
(`textflow::tests::a_duplicated_page_keeps_its_copied_chain_to_itself`,
`blend_link::tests::a_duplicated_page_blends_to_its_own_end_object`).

## Deleting a page or a layer with linked text frames

A page deleted (the page tab's Delete, Page > Delete Page) or a layer
deleted in the Objects docker with a frame of a linked text chain on it
leaves that frame's text to the frames left, as deleting the frame itself
does (see [Text](text.md)): the frames left are relinked and the text
flows anew over them, in the same undo step. The text the deleted frame
showed was lost with the page or the layer
(`textflow::tests::deleting_a_page_or_layer_holding_a_frame_of_a_chain_keeps_its_text`).

Tests: `command::tests::duplicate_page_goes_before_or_after_with_or_without_contents`,
`command::tests::a_saved_duplicate_page_without_options_copies_contents_after`,
`ui::window_bars::tests::the_duplicate_page_dialog_puts_the_copy_where_asked_and_undoes`,
`ui::window_bars::tests::layout_duplicate_page_opens_the_dialog`.

## Page size on the property bar

With nothing selected the property bar starts with the page size list:
the favourites (A4, Letter, A5, A3, Legal), the sizes the user saved,
then every preset under its group heading (ISO A, ISO B and JIS B,
North American, Print shop, Posters and banners, Envelopes, Cards and
postcards, Photo, Screen and social media, Web ads), a line, then
Custom... and Edit This List.... No two presets share a size in either
orientation. The list shows the
preset the page matches (either way round) or Custom when it matches
none, and Custom... is then the entry ticked. A preset keeps the page's
orientation. Custom... opens the Page Size dialog with the page's width
and height, to type any size (#650, #1092); the page width and height fields
next to the list also take any size at once. Both resize the current
page or every page as the All Pages / Current Page buttons say.
A page is resized about its centre: its objects move by half of what
the page grew or shrank, in the same undo step, so a drawing centred on
the page stays centred. The Page Size dialog, Document Setup and Switch
Page Orientation do the same; guidelines keep their place
(`ops2::tests::a_page_is_resized_about_its_centre`, #2356).
page or every page as the All Pages / Current Page buttons say.
Edit This List... opens Document Setup > Page Size, where the
save button next to Size keeps a custom size under a typed name and the
delete button forgets the saved size shown; saved sizes are kept in the
preferences and listed in every page size list (the property bar, the
Page Size dialog, Document Setup and Create a New Document).
Checked by `ui::propbar::tests::the_page_size_list_offers_a_custom_size`,
`ui::dialogs::tests::page_size_presets_are_distinct_and_keep_their_names`,
`ui::dialogs::tests::a_page_size_finds_its_name`.

## The zoom when going to another page or turning it

Going to another page (its tab, Page Up and Page Down, the navigator)
keeps the zoom: a page shown before comes back as it was left in that
window, zoom and position; one not shown yet is centred in the window at
the zoom in use. Portrait and Landscape on the property bar keep the
zoom and the page's centre where it was on screen. Each fitted the page
to the window (#3434,
`multipage::tests::pages_keep_the_zoom_when_gone_to_and_turned`);
Zoom to Page (Shift+F4) still does.

## Page Spacing

Page > Page Spacing, after Page Layout: a submenu of presets (0, 5, 10,
20, 30 and 50 mm, the one in use marked) and Custom..., a dialog with
one number field in the document's units (0 to 1000 mm) that OK applies.
It sets the space between the pages of the Multipage View, the same
value as Document Setup > Layout > Page spacing; one undo step, saved
with the drawing (see `multipage-view.md`,
`ui::menus::tests::the_page_menu_sets_the_page_spacing`,
`ui::dialogs::tests::the_page_spacing_dialog_sets_the_drawing_s_spacing`).

## Fit Page to Selection, Fit Page to Drawing

Page > Fit Page to Selection (pale with nothing selected) and Fit
Page to Drawing (pale on an empty page), also as buttons on the
Document Setup > Page Size page and, Fit Page to Drawing, on the
property bar with nothing selected (after the page width and height):
the current page takes the size of
the selection's bounds (effects included), or of everything on the page,
and the page's objects move so that box sits on the page from its
bottom-left corner. One undo step (`App::fit_page_to_selection`,
`fit_page_to_drawing`); a box under 0.1 mm does nothing and the status
bar says so. The property bar's button opens a popup instead of fitting
at once: a Margin distance field (in the ruler unit; kept in the
preferences, `settings.fit_page_margin`, 0 at first) and an Autofit
Page button, which fits the page to the drawing with that margin left
free on every side (`App::fit_page_to_drawing_with_margin`; a margin
that is not a number counts as none). Checked by
`ops2::fit_page_tests::the_page_fits_the_selection_or_the_drawing` and
`ui::propbar::tests::the_page_bar_draws_with_its_fit_page_button`.
Autofit Page follows the page bar's All Pages and Current Page buttons,
as the page size fields do: with All Pages on, every page is fitted to
its own drawing with the margin, in one undo step, and a page with
nothing on it keeps its size; the button is pale only when no page has
anything on it (#1393, `App::autofit_pages`,
`ops2::fit_page_tests::autofit_with_all_pages_fits_every_page`).
While the popup is open the page itself shows at the size Autofit Page
would give it, around the drawing, following the margin as it is typed;
nothing else is drawn, and closing the popup without OK puts it back
(#594, #604, `ops2::tests::the_autofit_preview_is_the_drawing_and_its_margin`).
A Live preview check box on the margin's row, right of the field (on at
first, kept in the preferences, `settings.fit_page_preview`), turns that
off: the page then keeps its size until Autofit Page is clicked (#659,
`ui::propbar::tests::the_autofit_popup_previews_only_with_live_preview_on`).
Clicks inside the popup (the box, the margin field and its arrows) keep
it open, so the preview can be switched and watched; a click outside or
Autofit Page closes it
(`pick_drag_tests::the_autofit_popup_stays_open_when_its_box_is_clicked`).

## Delete Page

Page > Delete Page... and the Delete Page... button of the Pages panel
open the Delete Page dialog (#500): Delete page (the current page to start
with) and a Through to page check box with the last page of a range, off
to start with (the range may be given in either order; numbers are kept to
the drawing's pages). OK deletes the pages in one undo step; the page after
them, else the one before, becomes the current page. A range of every page
deletes nothing and the dialog says a drawing keeps at least one page. The
page tab menu's Delete Page deletes that page at once
(`app::tests::delete_page_takes_a_range_of_pages`).

## Moving objects to another page by its tab

With the Pick tool, the selection dragged onto another page's tab goes
to that page (#1346, `page_drop`):

- While the selection is moved over a page tab, that tab lights up.
  Resting there for 0.6 seconds sends the selection to that page, which
  becomes the current page, and the move goes on there: while the
  pointer stays on the tabs the objects stay at their place on the
  page, and back on the drawing they follow the pointer from there, to
  be let go anywhere.
- Let go on a page tab (another page's at once, or the tab of the page
  the rest opened), the selection lands at its place on that page; the
  way the pointer went to the tab is not applied.
- With Ctrl (Cmd on macOS) held, or after a right click during the
  move, copies go to the page and the originals stay where they were.
- The objects go to the page's topmost visible, unlocked layer (its
  topmost unlocked one when none is visible); a page with only locked
  layers takes nothing and the status bar says so. They stay selected
  on the page they went to.
- The whole drag is one undo step, the move after the page changed
  included.

- A colour or a fill dragged the same way (a palette swatch, the fill
  swatch of the status bar or of the property bar, or an object dragged
  with the right mouse button) and rested 0.6 seconds on another page's
  tab opens that page, so it can be let go on an object there (#2704).
  A right-button drag carried over keeps the object it started from: let
  go on an object of the new page, that object takes its fill (with
  Shift its outline, with Ctrl both and the transparency), with no menu;
  the dragged objects stay on their page.

Checks: `pick_drag_tests::a_selection_dropped_on_a_page_tab_moves_to_that_page`,
`pick_drag_tests::ctrl_on_a_page_tab_sends_a_copy`,
`pick_drag_tests::resting_on_a_page_tab_opens_the_page_and_the_move_goes_on`,
`pick_drag_tests::a_palette_colour_rested_on_a_page_tab_opens_the_page_and_fills_there`,
`pick_drag_tests::a_right_drag_rested_on_a_page_tab_copies_the_fill_onto_that_page`.

## Gather Pages on Page 1

Layout > Gather Pages on Page 1 (when there is more than one page) puts
the objects of every page side by side on the first page, to print or
cut a run of pages on one sheet (#2098). Each page's objects keep their
place on their page and go to the right of the page before, a **gap**
apart (5 mm at first, up to 1 m), the pages' tops in line; each layer's
objects go into the first page's layer at the same place in the list,
else into its first layer. With **Fit page 1 to them** (on at
first) the first page grows to the pages' widths and gaps side by side
and the tallest page's height; with **Delete the emptied pages** (off at
first) the other pages go. It is one undo step ("Gather Pages"); a
drawing of one page has nothing to gather and says so on the status
line (`gather_pages::tests::every_pages_objects_come_side_by_side_on_the_first`).

## Page miniatures

The Pages docker shows each page's miniature beside its name, and the tip
of a page tab shows the page's miniature above its name, so a page is
found by what it shows, not by its number alone (#2932); a click on a
miniature goes to its page, as a click on its name does. The miniatures
are cached and drawn again once the drawing changes, one page a frame,
the others keeping their old picture meanwhile, so a drawing of many
pages does not lag; View > Page Sorter still shows every page large
(`ui::preview::tests::page_thumbs_are_drawn_one_a_frame_and_follow_changes`).
