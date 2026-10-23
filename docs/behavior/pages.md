# Pages

## Duplicate Page

Layout > Duplicate Page..., the page tab menu's Duplicate Page... and the
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
it with its contents after it, as before (Print Merge works that way).

Tests: `command::tests::duplicate_page_goes_before_or_after_with_or_without_contents`,
`command::tests::a_saved_duplicate_page_without_options_copies_contents_after`,
`ui::window_bars::tests::the_duplicate_page_dialog_puts_the_copy_where_asked_and_undoes`,
`ui::window_bars::tests::layout_duplicate_page_opens_the_dialog`.

## Fit Page to Selection, Fit Page to Drawing

Layout > Fit Page to Selection (pale with nothing selected) and Fit
Page to Drawing (pale on an empty page), also as buttons on the
Document Options > Page Size page and, Fit Page to Drawing, on the
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
