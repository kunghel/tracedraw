# Symbols panel

The Symbols panel (Ctrl+F3) lists the drawing's symbols, each with how
many instances of it the drawing holds: on every page and master layer,
inside groups and clip frames too. An instance inside a group was not
counted, so a symbol whose copies were all grouped said 0 (#3248,
`ui::dockers2::symbol_docker_tests::grouped_symbol_instances_are_counted`).

Each row shows a small picture of the symbol's objects (#3249). The
picture frames the visual bounds of what the symbol actually draws
(`Shape::visual_bounds_with`), including off-origin `.cdr` definitions,
tiny or oversized symbols, strokes inside groups, spot inks, embedded
fonts and nested symbols, instead of the 10 x 10 mm placeholder box at
the origin (#3899,
`object_thumbs::tests::symbol_thumbnails_frame_off_origin_sized_and_nested_definitions`).
Inserting a symbol places the centre of its drawn bounds at the page
centre (#3899).

Editing a symbol opens its objects on a clean sheet (visibility 0, paper
across the window and a light mark at the symbol's origin, zoomed to fit
and restored on Finish or Cancel); clicking Edit in the panel edits the
definition upright at 1:1 without placing a new instance on a page that
has none (#3900,
`ops2::tests::editing_a_symbol_from_the_panel_uses_a_clean_sheet_without_adding_an_instance`).
A selected instance, or a group holding one, was outlined by a square
instead of the objects it shows, and a click inside a group of instances
missed it; both the panel and the top-left bar carry Cancel beside
Finish while editing, and the panel carries Duplicate (#3437,
`ops2::tests::symbol_instances_outline_their_objects_cancel_and_duplicate`).

## Layout

The row at the top holds three buttons: New symbol from selection
(enabled when something is selected), By similarity (enabled when the
selection holds a group) and Revert to objects.

Each symbol takes one row, as before: the small picture of what the
symbol draws, its name (a double click renames it in place; Enter or a
click elsewhere keeps the new name, Esc leaves it), Insert, Edit,
Duplicate, Replace and its instance count, counted on every page and
master layer, inside groups and clip frames too. While the symbol is
being edited, its Edit button becomes Finish and Cancel. Replace is
enabled only when the drawing holds more than one symbol and no symbol
or clip frame edit is going on; Edit and Duplicate wait for the edit to
end too. The last By similarity or Replace result stays under the list
(`App::symbolize_report`). There are no keyboard shortcuts of their own
for By similarity and Replace; the panel opens with Ctrl+F3.

## By similarity (#2343)

By similarity turns every group that copies the selection into an
instance of one symbol, where each group stands. The sample is the
selected group, or, when the selection is a symbol instance, that
symbol's definition, or, for loose selected objects, those objects
themselves (text apart). The measure is a
fingerprint of the group's objects that are not text, in order
(`tracedraw_core::symbols::fingerprint`):

- the kind tag, the same the Objects docker shows (Rectangle, Ellipse,
  Curve, ...), so a curve never matches a rectangle;
- the object's box relative to the corner (least x, least y) of the
  common box of all those objects, each value rounded to a tenth of a
  millimetre (`(v * 10.0).round()`);
- the fill, serialised as JSON.

Two groups match when their fingerprints are equal. A copy only moved,
by its own transform or by its objects' places, matches, because the
boxes are read from the common corner; a recoloured or reshaped copy
does not. Text is left out of the measure: a group's labels stay
editable text beside the instance, in a group that keeps the original
group's transform and name.

The search walks every layer of every page and the master layers, into
groups and clip frames at any depth. The outermost matching group wins;
its own members are not matched again. An existing symbol whose
definition has the same fingerprint is reused, with its index and name;
otherwise a new definition is appended, named `Symbol {n}` (or the name
a script gives). The definition is the group's graphics moved so the
corner of their common box sits at the origin.

Each matched group is replaced in place by `Command::ReplaceShapes`:
the instance takes the group's slot, so its parent and its z-order stay,
and it stands at the corner of the graphics' common box. With labels,
the replacement is the group of the instance and those labels. The
whole plan runs as one batch, one undo step
(`pick_drag_tests::symbolize_similar_in_place`,
`symbols::tests::symbolize_matches_at_every_depth_and_keeps_labels`,
`symbols::tests::symbolize_reuses_a_symbol_that_draws_the_same`).

The status bar tells when no group is selected
(`status.symbolize_needs_group`) or no group copies the selection
(`status.symbolize_none`); the count and the symbol's name
(`status.symbolized`) are shown under the list.

## Replace: merge one symbol into another

Replace works without a window, in two presses. The first press on a
row's Replace picks that symbol as the source: its own button turns
into Cancel, and every other row's Replace turns into "Merge into this
symbol" (`docker.replace_target`). A press on another row's button
merges the source into it; a press on the source's own button, or Esc,
cancels the pick (`docker.replace_pick` hints the state).

The merge is `Command::MergeSymbols { from, to }`:

- it refuses an index out of range, merging a symbol into itself, and
  merging into a symbol whose definition draws the source: the result
  would draw itself without end (the cycle guard);
- every instance of the source becomes an instance of the target, on
  every page and master layer, inside groups and clip frames, and
  inside other symbols' definitions too;
- the source definition is removed, and every index after it shifts
  down a place, everywhere;
- one undo step brings the definitions, the indices and the instances
  back (`command::tests::merging_symbols_remaps_instances_and_shifts_indices`,
  `pick_drag_tests::merge_symbols_via_app`).

While a symbol or clip frame edit is going on the merge is refused with
`status.symbol_merge_finish_edit`; finish or cancel the edit first
(`pick_drag_tests::replace_pick_and_merge_guard`). The result is shown under
the list (`status.symbol_merged`).

## Scripting

`symbolize_groups(id, {name})` keeps its own top-level walk: it matches
the top-level groups of every page whose graphics copy the sample
group, replaces each by an instance (labels staying text beside it),
reuses an existing symbol with the same fingerprint, and returns the
counts (`replaced`, `with_text`, `skipped`), the symbol's index and name,
and the new ids
(`scripting::extra::tests::similar_groups_become_one_symbol_and_keep_their_text`).
The panel's By similarity uses the deeper recursive core plan
(`tracedraw_core::symbols::plan` / `plan_graphics`) instead, so it also
reaches groups nested in groups and clip frames and replaces in place.
