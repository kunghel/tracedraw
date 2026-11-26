# Edit a picture inside

Editing the pixels of a picture without an outside program (Bitmaps >
Edit Inside, or a double click on a picture with the Pick tool). The
window turns into an image editor for it, as Print Preview turns it into
a view of the printed page, and Finish puts the result back where the
picture lies.

## Image editing mode

While a picture is edited (`app.pixel_edit` is set) the main window keeps
its bars, palettes and panels, docked, floating and moved as ever, with
the picture's content in them (`ui/pixel_mode.rs`):

- Menus: File (Finish, Cancel), Edit (Undo, Redo, Cut Ctrl+X, Copy
  Ctrl+C, Paste Ctrl+V, Copy or Cut the selection to a layer, Paste as layer, Delete, Fill, Content Fill),
  View (Zoom In, Zoom Out, Fit in Window, Actual Pixels), Image (Crop to
  Selection, Auto Crop; Convert to Greyscale, Convert to Black and White;
  Mirror (across, up and down); Rotate (90 degrees clockwise Ctrl+R,
  counter-clockwise Ctrl+Shift+R, 180 degrees); Canvas Size, Smart
  Resize; Red Eye; Histogram), Adjust (the adjustment effects in groups,
  each with a picture: Auto Adjust, Image Adjustments; Contrast
  Enhancement, Local Equalization; Target Balance, Tone Curve Ctrl+T;
  Brightness/Contrast/Intensity Ctrl+B, Gamma; Hue/Saturation/Lightness
  Ctrl+Shift+U, Desaturate, Convert to Greyscale, Selective Colour,
  Replace Colours; Channel Mixer, Colour Balance Ctrl+Shift+B; then the
  rest), Effects (every other effect group), Selection, Layers, Window
  (Toolbars: Standard, Property Bar, Toolbox, Palette, Status Bar;
  Panels; Hints; Reset Image Panels) and Help. An effect chosen from a
  menu or its keys opens the Effects panel with its settings, preview and
  Apply. Image commands keep the picture's middle where it was on the
  page (a turn) or its pixels where they were (a crop, a canvas grown),
  as one undo step, every layer, mask and the selection with them.
- Toolbox: the image tools as icon buttons in 13 groups, Pick first, then
  the selections, the view (Zoom, Pan), the retouching, the brushes that
  lighten, darken and soften, the distortions, the paint, the eraser, the
  shapes, the transparency tools, the light, the eyedropper and the
  fills; a group of more than one tool has a flyout (the corner arrow, a
  press held 0.4 s or a right click) listing them with icon, name and
  key, and its button shows the tool last picked from it. A tool's tip
  names its key.
- Standard bar: Finish, Cancel, Undo, Redo, Copy and Cut to layer, Paste
  as layer, Delete, the zoom (out, a list of the zoom steps, in, Fit,
  1:1) and the panels' show or hide. Its buttons are kept apart from the
  drawing's (`bar_items` under `image_standard`): Alt and a drag (or the
  Customization pages open) moves one or drags it off, and Window >
  Toolbars > Image Bar Buttons ticks any image command onto the end of
  the bar or off it, with Reset Image Bar.
- Property bar: the tool's icon and name and its options. The selection
  tools add four mode buttons (Normal, Add, Subtract, Exclusive Or; Shift
  adds, Ctrl subtracts and Shift+Ctrl keeps where only one is, whatever
  the button), the selection's box (X, Y, width, height), and for the
  rectangle and the ellipse a Fixed size with W and H (a click makes a
  selection that size). Pick shows X and Y, W and H in pixels and in
  percent (proportions kept with the lock), Rotate in degrees and the two
  flips, put in the layer with Apply (Reset clears them): the selected
  pixels (the whole layer with nothing selected) are lifted, moved,
  scaled, turned and flipped about the middle of their box (bilinear),
  laid over what stays, the selection going with them, one undo step.
- Panels: Hints, Layers, Effects, Selection, Retouch, Distort, Picture
  and Histogram (`DockerTab::PIXEL`, kept apart from the drawing's
  panels). On entry the drawing's panels are kept and the image panels
  shown as they were last left (`settings.pixel_dockers`, all of them the
  first time); on the way out they are remembered and the drawing's put
  back. The preferences keep the drawing's panels meanwhile. Hints joins
  panels left from before it came once (`settings.pixel_hints_added`);
  closed after, it stays closed. It shows the tool in use (icon, name,
  key), what it does, how to use it, the keys that change it and the
  window's keys. Picture adds Canvas Size: a width and height in pixels
  and one of nine anchors, for Apply; the new room is clear.
- The Text, Zoom, Transform, Time Tracker and user toolbars wait for the
  drawing; the document tabs are hidden.
- A palette colour sets the brush colour (a right click, where it sets
  the outline, the shapes' outline colour).
- Status bar: the tool's name and hint, the pointer's pixel, the
  picture's size, the layer in use and the zoom.

Scrollbars run along the right and the bottom of the picture's window,
like the drawing's (arrows of 40 points, a click on the track pages 90 %
of the window, the thumb drags); they cover the picture with half a
window round it, or the window where that is more, and the corner
between them fits the picture in the window.

The picture shows alone and upright in the middle, on a checkerboard of
8 point squares, at a zoom of its own (`pixel_view.rs`): fitted with a
48 point margin and never past 100 % until the user zooms or pans, then
kept. The wheel (or Ctrl and the wheel, or a pinch) zooms about the
pointer, Shift and the wheel moves it across; the middle button, Space
and a drag, or the Pan tool (H) move it; the Zoom tool (Z) zooms in a
step on a click, out with Alt or a right click. Zoom steps run from 2 %
to 3200 %, the zoom from 1 % to 6400 %; Ctrl++ and Ctrl+- step, Ctrl+0
fits and Ctrl+1 shows the actual pixels.

## Model

`PixelEdit` (`pixel_edit_app.rs`) holds the picture decoded to RGBA
(`PixelImage`), an optional selection mask (one byte per pixel, 0 to
255), the tool, its options and its own undo history. The selection
shows as marching ants (`selection_edges`): the pixel edges where the
mask crosses half way, joined into straight runs turned clockwise round
the selection, drawn as a black line one screen point wide with white
dashes of 4 points (gaps of 4) moving 12 points a second; only the runs
on screen are drawn, and past 400,000 points of outline the dashes stand
still. Nothing in the
document changes while editing: the page is drawn without the picture
(`pixel_edit_doc`, the shape with `visible = false`) and the edited
pixels are drawn over it as a texture through the picture's own
transform, so a rotated or skewed picture is edited where it is.

Finish (Enter, the green button, or switching document) runs one batch,
"Edit Picture", which is one undo step:

- `SetShapeKind` with the new PNG and no bitmap effects (the effects
  already stacked on the picture are baked into the pixels at the start,
  so what is seen is what is edited),
- `SetPictureInks` to clear spot inks, if there were any,
- `SetObjectData` without `bitmap.link`, because the pixels no longer
  match the linked file.

Cancel leaves the picture as it was. A Finish with no change runs no
command. A crop moves the picture's rectangle so the kept pixels stay
where they were on the page.

## Mapping

Page point to pixel: the inverse of
`parent_transform * shape.transform * [rw/w, 0, 0, -rh/h, x0, y1]`;
pixel y runs down, page y runs up.

## Tools

| Tool | Key | What it does |
| --- | --- | --- |
| Pick | V | a box with handles round the selected pixels (the layer's pixels that are not clear with nothing selected): a drag inside it, or away from its handles, moves them by whole pixels; the square handles at the corners and the middles of the sides scale with the opposite side staying (a corner keeps the width to the height while Keep ratio is on, Shift the other way); the round handle above, or a drag just outside a corner, turns about the middle (Shift in steps of 15 degrees). The pixels follow the drag and go into the layer on release, one undo step; the selection goes with them. Its numbers move, scale, turn and flip them, shown before Apply or Enter; Esc resets them (see Property bar) |
| Rectangle mask | R | drag a box; Shift adds, Ctrl subtracts, Shift+Ctrl exclusive or |
| Ellipse mask | J | drag a box |
| Freehand mask | K | a click sets a corner, a drag draws freehand; a double click, Enter or a click within 8 points of the first corner closes it; Esc drops it |
| Pen mask | N | a press sets a point and a drag from it pulls its two handles (opposite, or with Alt only the one going on), so the outline curves through it as a cubic curve; a click is a corner; the curve on to the pointer shows dashed; Backspace takes the last point back; a double click, Enter or a press within 8 points of the first point closes it (curves cut into steps of about 2 px); Esc drops it |
| Lasso | A | freehand outline, closed on release |
| Edge mask | 4 | freehand outline whose points (2 px apart) go to the strongest edge (Sobel slope of the light, less half a unit a pixel away) within the reach (1 to 40 px, 6), closed on release |
| Magic wand | W | pixels of like colour (Tolerance, Contiguous) |
| Brush mask | B | paints the selection with the brush (Size, Hardness); Alt, Ctrl or Subtract takes away; in Normal a new stroke starts a new selection |
| Band mask | 8 | drag across: the band between the lines through both ends square to the drag, with a soft edge (0 to 400 px, 20, smoothstep) |
| Zoom / Pan | Z / H | see above |
| Clone | C | Alt+click sets the source, the offset is fixed at the first stroke |
| Heal | Shift+C | clone that keeps the destination's colour: source detail plus the local blur difference |
| Remove | Shift+X | see Following the surface and Remove |
| Red eye brush | Y | takes red out where painted |
| Dodge / Burn | O / Shift+O | lighten / darken |
| Sponge | Shift+P | desaturate or saturate |
| Blur / Sharpen | Shift+B / Shift+H | local blur or unsharp |
| Smudge | Shift+U | drags colour along the stroke |
| Tint | Shift+T | the brush colour moved to each pixel's lightness (luma), alpha kept |
| Smear / Twirl / Attract / Repel | Shift+S / Shift+W / Shift+A / Shift+R | Liquify's Push, Twirl (Alt the other way), Pinch and Bloat as tools of their own (Size, Hardness, Strength) |
| Liquify | Shift+L | moves the layer's pixels with the brush (Size, Hardness, Strength): Push along the drag, Twirl clockwise or counter-clockwise, Pinch towards the centre, Bloat away from it, held still or dragged; each pixel samples the layer as it was at the press at its own offset (bilinear), changed by every dab a tenth of the brush apart, inside the selection; one undo step a stroke |
| Warp | D | see Stretch and warp |
| Brush | P | paints the colour |
| Scatter brush | I | a burst every half spray width along the drag: Number (1 to 40, 4) little pictures (dots, stars, bubbles, leaves, confetti, sparkles) spread evenly over the spray (the brush size), each Item size (1 to 200 px, 12) less up to Size variation (50 %), turned at random when Turn each is on, its hue moved by up to Colour variation (15 %; confetti three times, bright), anti-aliased, with the opacity, inside the selection; the same each stroke; one undo step |
| Restore | U | paints back the picture as it was when the editing began (clear where the picture had no pixels then) |
| Replace Colour | Q | pixels within Tolerance of the colour under the first dab take the brush colour plus their difference from it, fading out towards the tolerance |
| Eraser | X | soft eraser: fades pixels toward clear |
| Shape | F6 | rectangle, ellipse, regular polygon (3 to 24 sides, first corner at the top) or line in the dragged box: filled with the colour, outlined with the outline colour and width (centred on the edge), a line in the colour; anti-aliased, inside the selection, with the opacity; one undo step |
| Layer fade | 1 | a drag fades the layer: alpha times Start opacity (100 %) at the press to End opacity (0 %) at the release, straight along the drag or Round about the press, inside the selection; one undo step |
| Colour to clear | 2 | a click: the pixels within Tolerance of the colour clicked (Contiguous or anywhere) lose alpha, fully at the colour, half at the tolerance, times Strength, inside the selection |
| Transparency brush | 3 | brush (Size, Hardness, Opacity, Flow, Tip) that takes the alpha down to (1 - Transparency) of what it was (50 %) |
| Light | L | paints light on the layer in use with the active light (see Light tool) |
| Eyedropper | E | picks the colour seen (all layers) |
| Fill bucket | F | fills like-coloured pixels (Tolerance, Contiguous) |
| Gradient | G | from the colour to clear, or to white: linear, reflected, radial, conical or square |

Shift+Q turns the quick mask on and off.

Brush options: Size 1 to 1000 px (default 40), Hardness 0 to 100 %
(50), Opacity 0 to 100 % (100), Flow 0 to 100 % (60). `[` and `]` change
the size.

Tip (a menu beside the brush options): Spacing 1 to 300 % of the size
between dabs (25 %), Scatter 0 to 300 % (each dab strays up to half the
size times it, across and along, the same each stroke), Angle 0 to 180
degrees and Roundness 5 to 100 % (the tip flattened across its angle).

Dab weight at distance `d` from the centre, `r = size / 2`,
`inner = r * hardness`: 1 inside `inner`, 0 beyond `r`, smoothstep
`t^2 (3 - 2t)` between, with `t = 1 - (d - inner) / (r - inner)`. Each
dab adds `weight * flow` to the pixel's coverage for the stroke, capped
at Opacity, and the pixel is worked out from its value at the press, so
going over the same place within one stroke never passes the opacity.
Coverage is multiplied by the selection mask, so painting stays inside
the selection and fades with a feathered edge.

## Selection and retouch panel

All (Ctrl+A), None (Ctrl+D), Invert (Ctrl+Shift+I), Feather (three box
blur passes of the given radius), Grow or shrink (px), Delete (Delete or
Backspace: clears to transparent), Fill, Crop to the selection, Content
fill (fills the selection from patches of the surroundings, from the
edge inwards), Red eye (in the selection), Flip horizontal and vertical.

Effects: any bitmap effect, with its settings and a preview, runs inside
the selection only (the whole picture with no selection), blended
through the mask.

## Layers

The picture can hold layers of pixels, all of its size, bottom first
(`PixelImage::layers`); the first is the picture as opened (Background).
Every tool works on the layer in use (`PixelImage::img`); a locked layer
keeps its pixels (brushes, fill, gradient, move and the retouch and
effect commands do nothing on it; selections and the eyedropper work).
What is seen is the layers put together bottom first, each with its
opacity and merge mode (the 28 merge modes of transparencies, drawn by
the renderer's `composite_layer`); hidden layers are left out. While
painting only the changed box is put together again.

Layers panel (`ui/pixel_layers.rs`): on top the merge mode and opacity
of the layer in use (a drag of the slider is one undo step); then the
layers top first in a list, each a row with its show (eye) and lock
buttons, a bent arrow when it is clipped, a miniature of its pixels
(an adjustment layer's icon), a miniature of its mask when it has one,
its name and, under it, its merge mode and opacity (and "hidden"). A
click on a row uses the layer; a double click renames it in place
(Enter or a click away keeps the name). A click on the mask's miniature
uses the layer and paints its mask (framed in the accent), a click on
the pixels' miniature paints its pixels again. The miniatures are made
again at most every half second while the picture changes, never in the
middle of a stroke. Under the list, icon buttons named in their tips:
New (clear, above), Duplicate, Add Mask, New Adjustment Layer, Up, Down,
Merge Down (into the one below with its opacity and mode; the one below
keeps its settings), Flatten, Delete (never the last); then Copy to
Layer and Cut to Layer (the selected pixels, soft edges kept, on a new
layer above; greyed with no selection), Paste as Layer (the picture on
the clipboard, its top left at the selection's), Fit to Surface and
Clip to layer below (pushed in when on). A layer with a mask adds a
Mask row: Paint Mask (pushed in while painting it), Invert, Apply and
Delete. A thin line parts the rows. A right click on a row uses that
layer and lists its commands with their icons: Rename, Duplicate, Delete,
Show or Hide, Lock or Unlock, Clip to layer below, Up, Down, Merge Down,
Flatten, and Add Mask or Paint Mask, Invert Mask and Delete Mask (#4108).

Clip to layer below: the layer shows only where the nearest layer
beneath it that is not clipped has pixels (its coverage after its mask).

Layer mask: Add Mask gives the layer in use a mask shaped like the
selection (all shown with none); while Paint Mask is on, the brush hides
(paints 0) and the eraser shows again (255), with the brush's size,
hardness, opacity and flow, inside the selection, and the mask shows in
red where it hides. Invert Mask, Apply Mask (multiplies the pixels'
alpha by the mask and drops it) and Delete Mask. Merge Down applies the
top layer's mask and clip first.

Crop and flip reach every layer and mask. Finish puts the layers together into
the picture's pixels, as they were seen (#4112). The layers can stay,
chosen at the foot of the Layers menu:

- Keep Layers After Finish (on by default): the layers and masks stay in
  memory while the drawing is open, and Edit Inside on that picture opens
  them again, the same layer in use. Turned off, the kept layers go and
  their memory is freed; at most 8 pictures keep layers, the oldest goes
  first, and closing the last drawing frees them all.
- Save Layers in the Drawing File (off by default): the layers are also
  saved with the picture, as its object data `pixel.layers` (each layer's
  settings, its pixels and mask as PNG), hidden from the Object Data
  panel, so they open again after the file is opened again. The file
  grows by the layers' size.

Either way the layers carry a fingerprint of the picture they were put
together into: when the picture changed since (Undo, another edit, a
picture swapped), they no longer fit and the picture opens as it is; a
Finish with one plain layer drops the saved layers. When neither is on,
Finish on a changed picture with layers asks first: Flatten and Finish,
Keep Layers and Finish (turns the first on) or Go Back, with "Do not ask
again" (Options > Global > Warnings, "Before Finish in the image editing
mode puts the layers together for good").

## More selections

- Quick Mask (Q): the selection painted with the brush (selects, 255)
  and the eraser (takes out, 0), shown in red where it leaves out; the
  other painting tools say only the brush and the eraser paint a mask.
  Turned on with no selection, nothing is selected; turned off with
  nothing painted, there is no selection.
- By tone: Shadows (full below 25 % lightness, gone at 50 %),
  Midtones (full within 15 % of the middle, gone at 35 %), Highlights
  (from 50 %, full at 75 %), smoothstep between.
- By channel: red, green, blue or alpha of the layer in use as the
  selection.
- Border: `grow(r) - grow(-r)` with r the Feather value. Smooth: feather
  by r, then cut at half.
- Save Selection keeps it for the editing session; Load puts it back
  (only while the picture keeps its size).

## Adjustment layers and histogram

New Adjustment Layer (Layers panel, with an effect chosen under
Effects) puts above the layer in use a layer with no pixels of its own
that runs that effect on everything beneath it as it is seen, then
shows through its mask, opacity and merge mode. While it is the layer in
use, the Effects section shows its settings first, changed in place (a
drag is one undo step); painting tools, fill and effects do not change
it (its mask can be painted). Merge Down runs the effect on the layer
beneath; Flatten and Finish keep what is seen. The lock box still locks
its mask.

Histogram (side panel, closed at first): what is seen inside the
selection (pixels at least half selected), clear pixels left out:
lightness (luma) filled, red, green and blue as lines, scaled to the
tallest lightness bar; the mean lightness and the pixels counted under
it. Pictures over about 250 000 pixels are sampled on an even grid.

## Following the surface and Remove

Surface (a menu in the bar, for the brush and the clone stamp): Follow
the surface makes each dab read the photo as a relief. What is painted
on is what is beneath the layer in use (the layer itself when it is the
bottom one). The relief comes from the photo's lightness, a little
smoothed, against the lightness of its surroundings (a spread of about
a fortieth of the picture's longer side, 4 to 40 pixels):

- Depth (0 to 40 px, 6 by default) bends the dab along the slope of the
  relief, so a stroke crossing a fold is pushed with it.
- Light (0 to 100 %, 80 by default) lights the paint as the photo is lit:
  the colour (or the cloned pixel) is multiplied by the photo's lightness
  over its surroundings' (0 to 2), so the folds' shadows and highlights
  show through.

A depth map given by the editing mode (its size, 255 nearest) stands for
the one read from the light; the light still comes from the photo.

Fit to Surface (Layers panel, not for the bottom layer): the layer in
use laid onto what is beneath it with the same Depth and Light, inside
the selection; one undo step.

Remove (tool): paint over what should go; the painted place shows in
red. On release it is widened by Expand (0 to 40 px, 4 by default) to
take in the edge and the shadow, kept inside the selection when there is
one, and filled from what is around it as Content Fill does; one undo
step, and the selection is left as it was. A locked layer is not
changed.

## Smart Resize

In the Picture section: Width and Height, 10 to 300 % of the picture's,
then Apply (one undo step). The picture takes away (or doubles) the
seams of pixels, one pixel a row from top to bottom (or a column from
left to right), crossing the least change of colour, found on what is
seen; every layer and mask loses or gains the same seams. Selected
pixels count as very marked, so they are kept while others remain; the
selection is then dropped. Widening doubles the seams a shrinking copy
would take first (more than the width is spread evenly). The picture
keeps its scale on the page and its top left corner.

## Stretch and warp

- Stretch (arrows up, down, left, right): the selection box's edge row
  (or column) on that side, each pixel weighted by the selection, is
  repeated from the box out to the picture's edge, on a new layer
  "Stretched pixels" above the layer in use. With no selection the box is
  the whole picture and nothing changes.
- Warp tool: Start puts a grid (2 to 8 cells a side, 4 by default) over
  the selection box, else over the box of the layer's pixels that are not
  clear, else over the whole picture. Dragging a point (the nearest within
  20 screen pixels of where the button went down, so a quick drag still
  takes it; the point keeps its distance from the pointer instead of
  jumping under it; the point the pointer would take shows larger and
  filled, with a grab hand, #4116) bends the layer through a smooth (Catmull-Rom) surface
  through the points, previewed live; pixels outside the box stay. Enter
  or Apply keeps it as one undo step, Reset puts the points back, Esc or
  Cancel drops it, and changing tool applies it. A mask on the layer
  keeps its shape.

## Undo

Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y undo and redo inside the editing; the
history keeps at least 3 and at most 100 steps within 600 MB. After
Finish, the whole edit is one step of the drawing's undo.

## Light effects

Effects > Light (also quick buttons in the panel):

- Lighting: Omni, Spot or Directional light; centre in % of the picture,
  Radius %, Direction, Cone, Intensity, Ambient, light colour, Relief
  (bump from luminance).
- Lens flare: centre, Size, Intensity, colour, rings; only adds light
  (screen blend).
- Light rays: rays from a centre through the parts brighter than
  Threshold, Length, Intensity, colour.
- Bloom: glow around the parts brighter than Threshold, Radius,
  Intensity.
- Light leak: a warm glow from a corner, Size, Intensity, colour.

All keep the size and alpha of the picture.

## Light tool

Light painted on the photo (`pixel_light.rs`, options in
`ui/pixel_lights.rs`). The light shows where the brush paints, as it
paints. There are one or more lights; each has:

- a colour (default orange, then blue, magenta, green, warm white),
- Strength 0 to 300 % (100),
- a mix: Lighten (screen: `1 - (1 - c)(1 - min(1, l * a))`, never past
  white), Glow (adds `l * a`, burns out to white) or Tint (overlay of the
  light's colour, mixed in by `min(1, a)`, keeps darks dark),
- Direction, where the light comes from (0° right, 90° top; default
  135°) and Relief 0 to 100 % (50),
- On / off, and a mask, one byte per pixel, that the brush paints.

`a = mask / 255 * strength`, times the relief factor
`clamp(1 - (g . L) * 14 * relief, 0, 2.5)`, where `g` is the slope of the
photo's brightness (luma, softened by two box blur passes of radius
`clamp(min(w, h) / 250, 1, 8)`) and `L` the unit vector towards the light
(y down): the side of a shape turned to the light gets more light, the
side turned away less. The lights are applied in order over the picture
as it was when the first stroke started.

The light brush uses Size, Hardness and Flow (the same dab weight as the
brushes); each stroke raises the mask to `cover * 255` (never past full),
multiplied by the selection. Alt+drag, or Erase light, takes light away
(`mask * (1 - cover)`). Fill with light fills the active light inside the
selection (the whole picture with none); Clear light empties it. + Light
adds a light, Remove light takes the active one away (one is always
left).

The lights stay live while the Light tool is in use: changing a light's
colour, strength, mix, direction, relief or On lights the painted part
again, and Ctrl+Z undoes the strokes one at a time (the last 20); with no
stroke left, Ctrl+Z goes on with the picture's own undo, which takes the
lights back in one step. Choosing another tool, any command that changes
the picture, Redo, or Finish keeps the lit pixels; the lights keep their
settings with nothing painted.

## Layer styles

The Layers panel has a **Layer style** part for the layer in use (not an
adjustment layer): effects worked out from the layer's see-through edge
each time the layers are put together, so they stay changeable until
Merge Down, Flatten or Finish make them pixels (#4369).

- **Drop shadow** and **Outer glow** go behind the layer; **Colour
  overlay**, **Gradient overlay**, **Inner glow**, **Inner shadow**,
  **Bevel and emboss** (inner, outer, emboss, pillow; raised or sunk) and
  **Stroke** (outside, inside, centred) go on it. Each has its colour,
  merge mode, opacity and sizes; the shadows and the bevel the angle the
  light comes from.
- **Fill** sets how much of the layer's own pixels show, its effects
  staying (0 % leaves only the effects). **Show effects** hides them all,
  keeping their settings. Copy, Paste and Clear work on the whole style.
- A drag of a setting is one undo step. The style is copied with the
  layer and kept with the layers after Finish and in the file.
- The layer's row shows a small picture when it has a style; its
  right-click menu has Clear layer style.

## Text tool

The **Text** tool (T) puts text on a layer of its own (#4370). A click
starts a new text layer whose first baseline starts there and opens a
box under it to type into (Enter a new line; Ctrl+Enter or Esc ends). A
click on a text layer's letters types into it again, a drag moves it.
The property bar has the font, the size in pixels, bold, italic,
underline, strikethrough, the alignment, the colour, the line spacing
and the letter spacing: changes go to the text layer in use at once,
and are what the next text layer starts with.

- The layer stays text, drawn by the drawing's own text engine, until it
  is painted on (it becomes pixels, one undo step brings the text back),
  scaled or turned with the Pick tool, merged down, flattened, or made
  pixels with **Make pixels** (property bar, or the row's right-click
  menu). Moved as a whole with the Pick tool it stays text.
- A text layer left empty goes when the typing ends. One still named
  "Text" takes its first words as its name.
- Layer styles work on text layers (a shadow or an outline round text).

## Right-click menu

A right click on the picture (#4391 #4393 #4394):

- with a brush (paint, eraser, clone, heal, the retouch brushes, the
  liquify brushes, the mask brush, Remove, the sprayer): the brush's size,
  hardness, opacity, flow, angle and roundness, and for the clone and
  heal brushes the source options below; Deselect and Select Inverse
  when there is a selection.
- with the other tools: Deselect, Select All, Select Inverse; Feather…;
  Modify > Border…, Smooth…, Expand…, Contract…; Transform selection (the
  Pick tool); Copy to layer, Cut to layer, Cut, Copy, Paste; Fill…,
  Stroke…, Content-aware fill, Delete; Crop to selection, Save selection,
  Quick Mask.

The commands with "…" ask their pixels in a small box first (Enter or OK
runs, Esc cancels), the fill and the stroke also the colour and the
opacity, the stroke its position (outside, inside, centred). The box
remembers what each was last given. Expand and Contract round the
corners as a circle rolled round them. The image menus open the same
boxes.

The selection tools' property bar has **Feather** (new selections get a
soft edge that many pixels wide), Deselect, Select Inverse and a
**Modify** menu of the boxed commands.

## Clone source options

The clone and heal brushes' **Source** menu on the property bar (and
their right-click menu) has (#4394): the **angle** the copy is turned
by round the source, its **scale**, **Mirror across** and **Mirror
down**, **Copy from** (this layer, this layer and below, all layers) and
**Aligned** (the source follows the brush from stroke to stroke; off,
each stroke starts again at the source). **New layer for cloning** adds
a clear layer and copies from all layers onto it, keeping the picture
beneath as it is.

## Checks

- Given a picture, when a page point is mapped to pixels and back, then
  it lands on the same point
  (`pixel_edit_app::tests::page_points_map_to_the_pictures_pixels`).
- Given a stroke inside a selection, then nothing outside changes
  (`pixel_edit::tests::a_brush_stroke_paints_inside_the_selection_only`).
- Given a stroke over the same place, then coverage never passes the
  opacity (`a_stroke_never_goes_past_its_opacity`).
- Given an edit, when finished, then one undo puts the old pixels back
  (`finish_puts_the_pixels_back_in_one_undo_step`).
- Given a crop, then the kept pixels stay where they were on the page
  (`a_crop_keeps_the_pixels_where_they_were_on_the_page`).
- Given a linked picture, when edited, then it no longer has the link
  (`a_linked_picture_stands_alone_after_editing`).
- Given an effect preview, then the picture changes only on Apply
  (`an_effect_preview_does_not_change_the_picture_until_applied`).
- Given a picture with a half-opaque layer, when finished, then the
  picture holds the layers put together, and a locked layer was not
  changed (`finish_puts_the_layers_together_and_a_locked_layer_keeps_its_pixels`).
- Copy, cut and paste: Ctrl+C takes the selected pixels of the layer in
  use (the whole layer with nothing selected), cut to the selection's
  box, and puts them on the system clipboard as a picture too; Ctrl+X
  also clears them; Ctrl+V pastes what was taken as a new layer at the
  place it was taken from (a picture copied in another program at the
  selection's top left) with nothing selected after, so the Pick tool's
  box is round the pasted pixels
  (`copy_paste_puts_the_pixels_back_where_they_were_and_move_takes_them`,
  `a_cut_clears_the_selected_pixels`,
  `copy_takes_the_selected_pixels_cut_to_their_box`).
- Pick tool: `the_move_box_handles_move_scale_and_turn`,
  `a_whole_pixel_move_keeps_the_pixels_sharp`,
  `the_move_tool_moves_only_the_selected_pixels_and_the_selection`.
- Given an effect open in the Effects panel, when a stroke is painted,
  then the preview shows again on the painted pixels once the button is
  up (`the_effect_preview_comes_back_after_painting_with_it_open`). A
  painting tool on an adjustment layer says the layer has no pixels to
  paint.
- Layers: `a_new_layer_is_clear_and_painting_changes_only_it`,
  `layer_opacity_visibility_and_merge_mode_change_what_is_seen`,
  `merge_down_flatten_delete_and_order`,
  `copy_and_cut_to_layer_and_move_a_layer`,
  `crop_and_flip_reach_every_layer`.
- Given light painted with the Light tool, then it shows only where it
  was painted, in its colour, inside the selection; two lights add up,
  a light turned off goes out; erasing and undo take it back; relief
  lights the side turned to the light; Tint keeps darks dark; keeping
  the lights leaves the pixels (`pixel_light::tests`).
- Masks: `a_layer_mask_hides_without_erasing_and_applies`,
  `a_clipped_layer_shows_only_over_the_layer_beneath`,
  `a_mask_stroke_moves_towards_its_value_up_to_the_opacity`,
  `tone_channel_border_and_smooth_selections`,
  `quick_mask_turns_painting_into_a_selection`.
- Brushes and shapes: `restore_tint_and_replace_colour_brushes`,
  `the_tip_spacing_and_roundness_shape_the_stroke`,
  `each_gradient_kind_spreads_its_own_way`,
  `a_shape_is_filled_and_outlined_inside_the_selection`,
  `a_line_and_a_polygon_are_drawn`.
- Following the surface and Remove:
  `the_shading_follows_the_light_of_the_picture`,
  `the_relief_pushes_paint_near_the_fold_only`,
  `a_given_depth_map_takes_the_place_of_the_light`,
  `a_layer_is_fitted_to_the_surface_beneath` in `pixel_surface::tests`;
  `a_brush_following_the_surface_is_shaded_by_the_photo`,
  `the_remove_brush_fills_the_painted_place_and_its_edge`;
  `the_remove_brush_shows_what_is_painted_then_fills_it_as_one_step`.
- Adjustment layers and histogram:
  `an_adjustment_layer_changes_what_is_beneath_until_merged`,
  `the_histogram_counts_the_selected_pixels` in `pixel_adjust::tests`.
- Liquify and Smart Resize: `push_moves_pixels_along_with_the_brush`,
  `pinch_and_bloat_pull_and_push_from_the_centre`,
  `smart_resize_takes_away_the_plain_columns_first`,
  `smart_resize_reaches_every_layer_and_keeps_the_selection` in
  `pixel_liquify::tests`.
- Image editing mode: `the_image_mode_swaps_the_panels_and_puts_the_drawings_back`,
  `only_the_main_bars_show_in_the_image_mode`,
  `toolbox_flyout_picks_are_remembered_per_button`,
  `the_image_bar_buttons_are_commands_and_can_be_changed`,
  `commands_run_from_the_menus_change_the_picture`,
  `every_image_tool_is_on_a_toolbox_button` in `ui::pixel_mode::tests`;
  `a_fitted_picture_fills_the_view_and_maps_both_ways`,
  `zooming_keeps_the_pixel_under_the_pointer_and_panning_moves_it` in
  `pixel_view::tests`; `every_image_tool_draws_an_icon_in_its_box`.
- Stretch and warp: `stretch_repeats_the_selections_edge_to_the_picture_edge`,
  `an_untouched_warp_leaves_the_layer_as_it_was`,
  `dragging_a_point_moves_the_pixels_with_it` in `pixel_warp::tests`.
- Clone, heal, dodge, burn, sponge, blur, sharpen, smudge, red eye,
  bucket, gradient, content fill, feather, grow and history each have a
  test in `pixel_edit::tests`; the light effects in `fx::light::tests`.
