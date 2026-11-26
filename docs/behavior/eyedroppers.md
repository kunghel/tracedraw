# Colour Eyedropper and Attributes Eyedropper

Both tools have two modes shown on the property bar: Select (sample)
and Apply. A sample switches to Apply; the Select button or Esc goes
back. Esc also drops the sample and returns to the Pick tool. The
pointer is the pipette in Select mode and the paint bucket in Apply mode
(the pipette again while Shift is held with the attributes eyedropper,
whose Shift+click samples; `ui::cursors` tests, #403).

In Apply mode Ctrl+click samples again with either tool and stays in
Apply mode, with no undo step (#1302,
`eyedropper::tests::ctrl_click_samples_again_in_apply_mode`); the
pointer is the pipette while Ctrl is held. Ctrl is the one modifier
both tools leave free: the colour eyedropper's Shift reads and writes
the outline, so its Ctrl+Shift+click samples the outline colour first;
the attributes eyedropper keeps Shift+click as a second way to sample.

Colours and attributes are read from the innermost object under the
pointer: a group's child, or a clip frame's content inside the frame (the
frame itself where no content is under the pointer), their transforms
composed (`App::leaf_to_read`, #403).

Both eyedroppers, and the document palette's, read from any visible
object under the pointer, a locked one or one on a locked layer too
(reading leaves it as it is; `App::hit_test_to_read`, #365). Applying a
sampled colour or attributes never changes a locked object.

A press let go is a click for both tools also when the pointer moved a
little while the button was down (a pen, a shaky hand) or the button
was held a moment, which the window takes for a drag or a long press:
such a click sampled or applied nothing, so the attributes eyedropper
seemed to apply only now and then (#2897,
`pick_drag_tests::the_attributes_eyedropper_applies_on_every_click`).

## Sample again after applying

Both tools' property bars have "Sample again after applying"
(`settings.eyedropper_sample_after_apply`, off at first, kept between
sessions). With it on, a click that applies puts the tool back in
Select mode, so the next click samples: pick, apply, pick, apply with
no key held and no press of Select Color, for a pen or a touch screen
(`eyedropper::tests::the_eyedropper_samples_again_after_applying_when_asked`,
#2380). Off, the tool stays in Apply mode as described above.

## Colour Eyedropper

- Property bar: Select Color, Apply Color, sample size (1x1, 2x2, 5x5
  pixels, for bitmaps), the sampled swatch with its description.
- Select mode, click on an object:
  - a bitmap gives the pixel under the pointer, averaged over the
    sample box (box anchored at the pointer pixel, clipped to the
    image), as RGB, whatever format the picture keeps (PNG, JPEG ...);
    a picture that keeps CMYK ink channels fitting its pixels (one read
    from a CMYK file, or converted to a CMYK bitmap) gives the inks it
    keeps, averaged the same way, as CMYK (#2381);
  - an object with bitmap effects (Hue/Saturation/Lightness and the
    others) gives the colour it shows with them, read from its picture
    over the sample box, its painted pixels only; it gave the fill the
    effects change (#2380,
    `eyedropper::tests::the_colour_of_an_object_with_a_bitmap_effect_is_read_as_shown`);
  - a closed object with a lens that lays a colour over what lies
    beneath (Transparency, Color Add, Color Limit, Tinted Grayscale,
    Wireframe) gives the lens colour, filled or not, anywhere inside it:
    an unfilled one lying over other objects gave the colour of the
    object beneath, a filled one its fill, which the lens does not draw
    (#2549, `eyedropper::tests::a_shape_with_a_transparency_lens_over_others_gives_its_colour`).
    A lens is picked anywhere inside it, as it shows there;
  - a solid fill gives its colour; a gradient fill gives its first
    stop; an unfilled object gives its outline colour on its outline
    only: inside it (picked there with Treat All Objects as Filled)
    nothing of it shows, so the colour shown there is read, where its
    outline colour was read as a fill (#3499,
    `eyedropper::tests::an_unfilled_object_gives_its_outline_colour_on_the_outline_only`);
  - Shift+click reads the outline colour first.
  - With objects selected, the colour sampled also goes straight onto
    them, as a palette swatch would: the fill, or with Shift+click the
    outline colour (each keeps its own width and dash), one undo step;
    locked objects keep theirs. Apply mode follows as usual (#2551,
    `eyedropper::tests::a_sampled_colour_is_applied_to_the_selection`).
- Apply mode, click on an object: a solid fill with the sampled colour
  (`SetFill`); Ctrl+click samples again (Ctrl+Shift+click: the outline
  first); Shift+click sets the outline colour instead, creating a
  hairline outline when the object has none (`SetStroke`).
- The object to paint is the usual hit test, else the topmost unfilled
  closed object whose interior contains the point, so an empty
  rectangle can be filled by clicking inside it.

### Live readout (#990)

While the Colour Eyedropper is in Select mode, and while a colour
picker's eyedropper button waits for a click on the drawing, a box
beside the pointer shows the colour a click would take
(`App::hover_reading`, `ui::cursors::paint_color_readout`):

- a 14-point swatch of the colour;
- its name, in bold, when it has one: a spot colour's ink, else the first
  colour of the default palette or the palettes shown that is exactly
  equal (`ui::palette::name_of`);
- its values in its own model, whatever the drawing's primary colour
  mode (CMYK: C, M, Y, K in %, RGB: R, G, B 0 to 255, HSB and HSL: H in
  degrees, S and B or L in %, Lab: L, a, b, YIQ: Y, I, Q 0 to 255,
  grayscale: its level 0 to 255), so a colour converted with
  Tools > Convert Colors reads in the model it went to, a picture in
  CMYK mode giving its pixels in CMYK (#1254), and a colour set in HSB,
  HSL, Lab, YIQ or grayscale reads in that model (#2612: they read in
  CMYK); one per line; spot colours show their tint and Registration
  its name;
- its hex value, upper case (`#E02020`).

The colour is the one the click would take: for the tool what
`sample_color_at` reads (Shift: the outline first); for a picker the
fill, else the outline, else white over empty paper. Off every object
the tool shows no box (a click there takes nothing). Apply mode (unless
Ctrl is held, which samples again), a held button and the other tools
show none.

A picker's request lives only while its eyedropper button is being
drawn (the button renews it every frame, `color_picker::picking` drops
one not renewed since the frame before): a picker closed while its
eyedropper waits (Edit Fill or Outline Pen closed with OK, a popup
picker closed by a click beside it) takes the request with it, so the
crosshair pointer, the readout and the click that would have gone to
the picker end with the picker (#1246: the readout followed the pointer
everywhere until the next click on the drawing).

The box sits 18 points right of and 20 below the pointer, on the
other side where the window ends, on the tooltip layer. Lag: the reading
is worked out again only when the pointer moves (to 1/1000 mm), the zoom,
page or drawing change, or Shift is pressed or released; the last
picture read stays decoded, so a bitmap is decoded once while the
pointer moves over it. The box is two rectangles and a few short texts,
with no render of the page. In `frame_times_of_the_eyedropper_readout`
(3000 objects and a 1200 x 900 picture, release build) a hover frame
costs the same with the eyedropper as with the Pick tool (about 0.85
ms on the test machine).

## Attributes Eyedropper

- Property bar: Select Object Attributes, Apply Object Attributes, the
  Properties menu (Outline, Fill, Text, Transparency; all on by
  default), the
  Transformations menu (Size, Rotation, Position; off by default) and
  Effects (off by default).
- Select mode, click: reads fill, outline and the first text span's
  character format (font family, size, bold, italic, underline,
  strikethrough, letter spacing, OpenType features, variable font axes)
  from the innermost object under the pointer (a group's child, a clip
  frame's frame or content), and the bounds width and height, rotation
  of the transform (angle of its x axis), bounds centre, opacity, drop
  shadow and live effects from the object as a whole. A text whose
  characters all carry the same colour (or outline) of their own gives
  that as its fill (outline). Reading the fill and outline of the group
  itself, which has none, made Apply take them away (#1550,
  `pick_drag_tests::copying_properties_from_a_group_takes_the_object_clicked`).
- Apply mode, click (Shift+click or Ctrl+click samples again instead): one batch
  "Copy Attributes" with the ticked groups.
  - Fill and Outline: `SetFill`, `SetStroke`. The characters of a text
    target let the colours (outlines) they carry of their own go, so the
    copied fill (outline) shows. A mesh fill, whose nodes lie in its
    object's own space, is fitted to the target's box
    (`copy_props::tests::a_mesh_fill_is_fitted_to_the_object_it_goes_to`).
  - Text: when both objects are text, every span of the target takes
    the sampled character format; the text itself is kept.
  - Transparency: the opacity and the transparency mask (fountain,
    pattern or texture, with its merge mode and what it applies to) in
    place of the target's own; its other live effects stay; a sample
    without transparency clears the target's (#1338,
    `eyedropper::tests::attributes_eyedropper_copies_transparency`).
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
  empty one, with default groups the second gets the blue fill and the
  50 % opacity and keeps its size; with Size and Effects ticked it
  becomes 10 x 20 mm about its own centre.
- `eyedropper::tests::eyedroppers_read_from_locked_objects_but_never_write_to_them`
- `eyedropper::tests::pictures_give_their_pixels_wherever_they_are` (alone,
  grouped, inside a clip frame, kept as JPEG)
- `eyedropper::tests::the_hover_readout_names_the_colour_and_keeps_its_work`:
  name, CMYK or RGB values and hex; nothing worked out again while
  nothing changes; a picture decoded once; white for a picker over paper.
- `pick_drag_tests::the_colour_eyedropper_reads_out_the_colour_under_the_pointer`:
  the box drawn in the window over an object, not off the objects nor in
  Apply mode; a picker's eyedropper shows it too.
- `pick_drag_tests::a_closed_pickers_eyedropper_stops_waiting`: Edit Fill
  closed while its eyedropper waits: no readout, no crosshair, the next
  click on the drawing is the tool's (#1246).

## Colours from the window

The Color Eyedropper also picks colours that no object gives (#3040):

- In Select mode, a click where no object under the pointer gives a
  colour (the desk around the page, the empty page) takes the colour
  shown there, read back from the window, averaged over the bar's sample
  size; it then goes where a sampled colour goes (Apply mode, onto the
  selection).
- Sample from Screen on its property bar: the next click anywhere in
  the window, on the panels, toolbars and palettes too, takes the colour
  shown there; the click presses nothing under it, and Esc or another
  tool ends it.
- Outside the window (#4083): pressed in the window (on the drawing, or
  with Sample from Screen anywhere) and let go outside it, the tool takes
  the colour the screen shows where it was let go: the desktop, the task
  bar, another program. Only where the system lets a program read the
  screen (Windows); elsewhere the status line says the colour could not
  be read (`screen_sample::desktop_color_at_cursor`).
- Over a photo being traced, a plain click takes the colour the drawing
  shows: inside an outline with no fill, the photo's; through a half
  transparent shape, the mix seen; on an opaque shape, the shape's own,
  and Alt+click reads the photo under it
  (`eyedropper::tests::a_plain_click_reads_what_shows_over_a_traced_photo`).
- The window is read back once a sample, so a renderer that cannot read
  back gives nothing and says so on the status line
  (`screen_sample::tests::a_click_off_the_objects_samples_the_window`,
  `screen_sample::tests::the_pixel_under_the_point_is_read_at_the_screens_scale`).
