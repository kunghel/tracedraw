# Shape Builder

The Shape Builder tool (Crop flyout, after the Eraser; #1149) builds
shapes out of the parts that overlapping objects cut each other into.

## Parts

The tool works on the selected objects it can cut: rectangles, ellipses
(not pies or arcs), polygons and stars, and closed curves, not locked;
groups, text, pictures, tables and open curves are left out. Each object's
area is the one Weld and Trim use. A part is a place inside the same
objects and outside all the others, one connected piece of it: two bars
crossing make five parts, the middle and the four arms
(`shape_builder::tests::overlapping_squares_cut_each_other_into_three_parts`,
`a_part_is_the_one_piece_under_the_point`).

The part under the pointer is hatched in the selection colour and edged
with it. It is worked out again only when the pointer leaves it, or the
drawing or the selection changes.

## Drag

A drag collects every part the pointer crosses, each once, also the parts
between two pointer positions far apart; they stay hatched until the
release. On release, in one undo step ("Shape Builder"):

- the parts are welded into one new closed curve, on the active layer, on
  top, with the fill (carried to the new shape as Weld carries it), the
  outline and the opacity of the front object over the first part;
- every selected object the parts overlap loses them: it becomes a curve
  of what is left, in its own space, keeping its fill and outline; an
  object left with nothing is deleted;
- the objects left and the new one are selected, to go on building.

The parts are worked out from the objects' outlines flattened once, so
two parts that share an edge share its points exactly; the merge, and
what is cut out of each object, are worked out on those and only then
laid back onto the objects' own lines and curves. A square merged with a
circle over it is one object with a node at each crossing and the
circle's arcs whole, and what is left of the circle is clean too (#4123:
parts cut from the curves each flattened a shared arc a little
differently, and the merge kept dozens of short pieces along it;
`shape_builder::polygon`,
`a_square_merged_with_a_circle_keeps_its_arcs_whole`).

Open lines among the selection (#3814) cut the parts too; the stretches
of a line that run through the inside of the new object (the lines that
divided the parts merged) are removed from it, so the new object has no
line across it; the rest of each line stays and stays selected, a line
with nothing left is deleted (#4079,
`shape_builder::line_outside`, `a_grid_of_open_lines_makes_its_cells_into_parts`).

With Alt held during the drag ("Shape Builder Delete") the parts are cut
out of the objects and nothing new is made; with lines alone selected,
an Alt drag removes every segment of the lines it crosses, as far as the
nearest crossings with the other selected lines, as Segment Delete does
(only the selected objects; #4079, `alt_deletes_the_segments_of_lines`)
(`shape_builder::tests::a_drag_merges_the_parts_it_crossed_into_a_new_object`,
`alt_takes_the_parts_away`).

## Click

A click on a part does what a drag over that part alone does: the part
becomes an object of its own (with Alt, it goes). Alt and a click on an
outline of the selection (within 5 screen pixels) removes its segment
between the nearest crossings, as Segment Delete does (#4079). A click where there is
no part, or with nothing the tool can cut selected, selects as the Pick
tool does; with nothing usable selected after it, the status bar asks to
select the overlapping objects first.

## Property bar

The tool's property bar (#3220):

- **Add** and **Delete**: what a drag or a click does with the parts,
  merge them into a new object (Add, chosen at first) or take them away
  (Delete); Alt does the other one.
- **Keep originals** (off at first): a merge makes the new object and
  leaves the objects it came from as they were, nothing cut out of them
  and nothing deleted; Delete still cuts.
- **Remove leftovers** (off at first, #4123): Accept also deletes what
  is left of the objects and lines the merges since the tool was chosen
  (or since the last Accept) cut, in one undo step ("Shape Builder
  Leftovers"), so only the shapes built stay, selected; objects no merge
  touched stay, and so does everything with Keep originals on or after
  Delete (`remove_leftovers_deletes_what_the_merges_left_on_accept`).
- **Accept**: the shapes built stay and the Pick tool comes back.
- **Cancel** (once the tool made a step): every step the tool made since
  it was chosen or since the last Accept is undone, the drawing as it
  was; the tool stays. A step of another tool in between stops it there.

While the tool is in use the objects it works on are edged in grey with
a white halo, apart from the selection colour of the parts, so the
objects taking part show
(`shape_builder::tests::keep_originals_cancel_and_accept`).

## Use Guidelines (#3558)

Off by default. With **Use guidelines** ticked on the property bar, the
guidelines the page shows (its own and the master page's, those not
hidden) cut the objects too: a part is kept to the side of each
guideline that the pointer is on, and the pointer on a guideline finds no
part. A merge or a removal cuts the objects along the guidelines; the
guidelines themselves are not changed
(`shape_builder::tests::lines_cut_a_square_into_parts`,
`shape_builder::tests::use_guidelines_cuts_the_objects_at_the_guidelines`).

With nothing the tool can cut selected (no closed object, no line), the
guidelines alone make the parts (#4123): each room they close in on every
side is a part, and a click or a drag makes it a new closed curve with
the default fill and outline of a new curve. A room open on a side has
no part; there a click selects as usual
(`shape_builder::guide_room`, `guidelines_alone_make_parts`).
