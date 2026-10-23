# Symmetry

Live mirrored copies of an object across one or more mirror lines
(Object > Symmetry).

## Model

`Effect::Symmetry { center, angle, lines }` on the object. `center` is a
page point, `angle` the direction of the first mirror line in degrees
(0 = horizontal, counter-clockwise positive), `lines` the number of
mirror lines (1 to 12), spaced evenly around `center`.

## Result

With `n` lines the copies are the dihedral orbit of the object: a
reflection across each line plus the rotations those reflections
generate, `2n - 1` copies in all. The copies are drawn above the original
(`evaluate()` puts them in `above`), share its fill, outline and other
effects, and follow every edit of the original, because they are
recomputed from it.

## Defaults

- Create (Object > Symmetry > Create New Symmetry, Alt+S, or the object
  context menu): one vertical line (90 degrees) through the right edge of
  the selection's bounds, at its vertical centre, so the copy appears next
  to the original instead of over it; then straight into editing it.
  Several selected objects are grouped first and the symmetry goes on the
  group (one "Symmetry" undo step), so they mirror together; Ctrl+click
  picks one of them inside the group (round handles) to move or colour
  it alone, the copies following.
- While a symmetry is edited, an object drawn, pasted or duplicated
  joins it: the group takes it in (an object that was alone with the
  symmetry becomes a group of it and the newcomer, the symmetry moving
  to the group), folded into the step that made the object, so one Undo
  takes both back; the newcomer stays selected inside
  (`App::join_symmetry`,
  `ops2::symmetry_tests::a_symmetry_takes_several_objects_and_those_made_while_editing`).
- Edit (Object > Symmetry > Edit Symmetry, Alt+E): the symmetry editing
  mode (`symmetry_edit.rs`). A bar floats at the top left of the drawing
  window: Finish; the mirror lines icon and their number (1 to 12); the
  centre (X, Y in the drawing's units); the angle of rotation icon and
  the angle (0 to 360 degrees), every number field with spin arrows and
  the mouse wheel; Show full preview (off at first: the copies are drawn
  as red outlines and the renderer leaves them out; on: drawn in full)
  and Show symmetry lines (on). Both toggles are kept in the
  preferences. A run of changes is one undo step "Edit Symmetry".
- While editing, the bar and the lines stay whatever is selected (a
  member picked inside, a new object, a double click): they follow the
  object being edited, not the selection.
- Picking while editing: a click or a marquee takes the objects inside
  the symmetry group (its members), never the group and never a copy.
- The mirror lines with the Pick tool: a drag on a line or on the centre
  moves the lines (the centre follows the pointer, snapping as objects
  do); a click on a line shows round handles at both ends of every line,
  90 pixels from the centre, and dragging one turns the lines about the
  centre (Ctrl: by the constrain angle); another click on a line hides
  them (`pick_drag_tests::symmetry_editing_picks_members_and_drags_lines`).
- Finish Editing Symmetry (Alt+Shift+E) and the bar's Finish end the
  mode; it also ends when the object is deleted or loses its symmetry.
- Remove Symmetry (Alt+Shift+S) drops the effect; the copies disappear.
- Break Symmetry Link (Alt+X) bakes the copies into objects of their own.

## Display

Mirror lines are drawn as dashed selection-colour lines across the whole
canvas while an object with the effect is selected, and while its
symmetry is edited (unless Show symmetry lines is off).

## Checks

- Given a rectangle at x 10..20, when Symmetry is created with the
  defaults, then the evaluated `above` contains one copy spanning x 20..30.
- Given two lines at 0 degrees through the origin, then there are three
  copies, one per other quadrant (core test `symmetry_two_lines`).
- Given a symmetry object, when the original is moved, then the copies
  move with it (they are recomputed, nothing is stored).
