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

With Alt held during the drag ("Shape Builder Delete") the parts are cut
out of the objects and nothing new is made
(`shape_builder::tests::a_drag_merges_the_parts_it_crossed_into_a_new_object`,
`alt_takes_the_parts_away`).

## Click

A click on a part does what a drag over that part alone does: the part
becomes an object of its own (with Alt, it goes). A click where there is
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
- **Accept**: the shapes built stay and the Pick tool comes back.
- **Cancel** (once the tool made a step): every step the tool made since
  it was chosen or since the last Accept is undone, the drawing as it
  was; the tool stays. A step of another tool in between stops it there.

While the tool is in use the objects it works on are edged in grey with
a white halo, apart from the selection colour of the parts, so the
objects taking part show
(`shape_builder::tests::keep_originals_cancel_and_accept`).
