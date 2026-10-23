# Colour Eyedropper and Attributes Eyedropper

Both tools have two modes shown on the property bar: Select (sample)
and Apply. A sample switches to Apply; the Select button or Esc goes
back. Esc also drops the sample and returns to the Pick tool. The
pointer is the pipette in Select mode and the paint bucket in Apply mode
(the pipette again while Shift is held with the attributes eyedropper,
whose Shift+click samples; `ui::cursors` tests, #403).

Colours and attributes are read from the innermost object under the
pointer: a group's child, or a clip frame's content inside the frame (the
frame itself where no content is under the pointer), their transforms
composed (`App::leaf_to_read`, #403).

Both eyedroppers, and the document palette's, read from any visible
object under the pointer, a locked one or one on a locked layer too
(reading leaves it as it is; `App::hit_test_to_read`, #365). Applying a
sampled colour or attributes never changes a locked object.

## Colour Eyedropper

- Property bar: Select Color, Apply Color, sample size (1x1, 2x2, 5x5
  pixels, for bitmaps), the sampled swatch with its description.
- Select mode, click on an object:
  - a bitmap gives the pixel under the pointer, averaged over the
    sample box (box anchored at the pointer pixel, clipped to the
    image), as RGB, whatever format the picture keeps (PNG, JPEG ...);
  - a solid fill gives its colour; a gradient fill gives its first
    stop; otherwise the outline colour;
  - Shift+click reads the outline colour first.
- Apply mode, click on an object: a solid fill with the sampled colour
  (`SetFill`); Shift+click sets the outline colour instead, creating a
  hairline outline when the object has none (`SetStroke`).
- The object to paint is the usual hit test, else the topmost unfilled
  closed object whose interior contains the point, so an empty
  rectangle can be filled by clicking inside it.

## Attributes Eyedropper

- Property bar: Select Object Attributes, Apply Object Attributes, the
  Properties menu (Outline, Fill, Text; all on by default), the
  Transformations menu (Size, Rotation, Position; off by default) and
  Effects (off by default).
- Select mode, click: reads fill, outline, the first text span's font
  (family, size, bold, italic), bounds width and height, rotation of the
  transform (angle of its x axis), bounds centre, opacity, drop shadow
  and live effects.
- Apply mode, click (Shift+click samples again instead): one batch
  "Copy Attributes" with the ticked groups.
  - Fill and Outline: `SetFill`, `SetStroke`.
  - Text: when both objects are text, every span of the target takes
    the sampled font; the text itself is kept.
  - Effects: opacity, drop shadow and the live effect list.
  - Size, Rotation, Position, in that order about the target's bounds
    centre: rotate by the angle difference, scale the bounds to the
    sampled width and height, then move the centre to the sampled
    position. One `TransformShapes` carries the three.

## Checks

- Given a red rectangle and an empty one, when the colour eyedropper
  clicks the red one then inside the empty one, then the empty one has
  a solid red fill; a Shift+click afterwards gives it a red outline.
- Given a 10 x 20 mm blue rectangle at 50 % opacity and a 10 x 10 mm
  empty one, with default groups the second gets the blue fill and keeps
  its size and opacity; with Size and Effects ticked it becomes 10 x 20
  mm about its own centre at 50 %.
- `eyedropper::tests::eyedroppers_read_from_locked_objects_but_never_write_to_them`
- `eyedropper::tests::pictures_give_their_pixels_wherever_they_are` (alone,
  grouped, inside a clip frame, kept as JPEG)
