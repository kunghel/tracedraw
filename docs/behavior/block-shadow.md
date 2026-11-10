# Block Shadow

Live effect (`Effect::BlockShadow { offset, color, gap }`) drawn below
the object: a solid extrusion of the outline along `offset`.

## Geometry

- The object's area is read with its fill rule (text with the nonzero
  rule, so the overlapping strokes of a letter make no holes) and
  flattened (0.02 mm) into clean contours, its vertices kept exactly,
  so corners stay sharp.
- The shadow is the union of the area, its copy moved by `offset` and,
  for every edge of every contour that faces `offset`, the
  quadrilateral the edge sweeps along `offset` (the edges facing away
  add nothing). Each contour is swept within itself: the letters of a
  word each cast their own shadow, with nothing joining one to the next
  (#2156). The union is worked out in one pass and the curves of the
  outline and of its moved copy are put back; it is one filled path
  with no outline, in the shadow colour.
- Gap: when greater than 0, the outline grown by `gap` mm is trimmed
  from the shadow, which leaves a clear band between object and shadow.

## Property bar

- Depth: `|offset|` in mm. Direction: angle of `offset` in degrees
  (0 = right, counter-clockwise positive). Colour (default 60 % black,
  CMYK 0 0 0 60). Gap in mm (default 0). Clear Block Shadow removes the
  effect.
- Editing a field with a shadowed object selected rewrites its effect;
  with nothing shadowed the values become the defaults for the next
  drag.

## Interaction

- Drag from an object: the object is selected and the shadow offset
  follows the pointer from the press point; every move replaces the
  previous block shadow.

## Checks

- Given a 20 mm square at the origin and offset (5, -5), then the shadow
  bounds are (0, -5) to (25, 20) and the point 0.2 mm inside the far
  corner is covered.
- `live::tests::block_shadow_follows_each_part_of_the_outline`: two
  squares, one with a hole, cast two shadows with nothing between them,
  the hole left open as far as the shadow does not cover it.
- `shaping::tests::sweep_*`: a square, two squares, a ring, crossing
  bars read with the nonzero rule, a circle and bad input.
- `effect_outline_tests::block_shadow_of_letters_stays_on_the_slid_letters`
  (text crate): every point of the shadow of "VINSIDEAKXW" (depth
  4.23 mm, direction -2.4 degrees) lies on the letters slid along the
  offset, and the slid letters are covered.
