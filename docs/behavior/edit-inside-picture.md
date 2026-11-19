# Edit a picture inside

Editing the pixels of a picture in place, on the drawing, without an
outside program (Bitmaps > Edit Inside, or a double click on a picture
with the Pick tool). Like editing inside a clip frame: the rest of the
page stays visible, the picture is edited where it lies, and Finish puts
the result back.

## Model

`PixelEdit` (`pixel_edit_app.rs`) holds the picture decoded to RGBA
(`PixelImage`), an optional selection mask (one byte per pixel, 0 to
255), the tool, its options and its own undo history. Nothing in the
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
| Rectangle selection | M | drag a box; Shift adds, Ctrl subtracts |
| Ellipse selection | | drag a box |
| Lasso | L | freehand outline, closed on release |
| Magic wand | W | pixels of like colour (Tolerance, Contiguous) |
| Brush | B | paints the colour |
| Eraser | E | soft eraser: fades pixels toward clear |
| Clone | S | Alt+click sets the source, the offset is fixed at the first stroke |
| Heal | J | clone that keeps the destination's colour: source detail plus the local blur difference |
| Dodge / Burn | O | lighten / darken |
| Sponge | | desaturate or saturate |
| Blur / Sharpen | R (Blur) | local blur or unsharp |
| Smudge | | drags colour along the stroke |
| Red eye brush | | takes red out where painted |
| Fill bucket | G | fills like-coloured pixels (Tolerance, Contiguous) |
| Gradient | | from the colour to clear, or to white |
| Eyedropper | I | picks the colour |

Brush options: Size 1 to 1000 px (default 40), Hardness 0 to 100 %
(50), Opacity 0 to 100 % (100), Flow 0 to 100 % (60). `[` and `]` change
the size.

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
- Clone, heal, dodge, burn, sponge, blur, sharpen, smudge, red eye,
  bucket, gradient, content fill, feather, grow and history each have a
  test in `pixel_edit::tests`; the light effects in `fx::light::tests`.
