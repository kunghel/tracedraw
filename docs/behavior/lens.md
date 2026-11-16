# Lens

A lens changes how objects beneath it appear, without changing them.

| Lens | Parameter | Default |
|---|---|---|
| Brighten | rate -100..100 | 50 |
| Colour add | colour, rate | white, 50 |
| Colour limit | colour, rate | black, 50 |
| Custom colour map | from, to, direction | black..white, direct |
| Fish eye | rate -1000..1000 | 100 |
| Heat map | palette rotation 0..100 | 0 |
| Invert | none | |
| Magnify | amount 1..100 | 2 |
| Tinted grayscale | colour | black |
| Transparency | rate 0..100 (how see-through), colour | 50, white |
| Wireframe | outline colour, fill colour | black, white |
| Bitmap effects | any bitmap effects, in order | none |

Shortcut: Alt+F3. Options: Frozen (bakes), Viewpoint, Remove face.

## Formulas (per pixel beneath the lens shape, colour `c` 0..1)

- Brighten: `c + rate/100 * (1 - c)` for positive, `c * (1 + rate/100)`
  for negative.
- Colour add: `min(1, c + rate/100 * k)`.
- Colour limit: `c * (1 - rate/100) + rate/100 * c * k`.
- Custom colour map: luminance `y` to `lerp(from, to, y)` (direct) or
  through hue rotation.
- Fish eye: radial remap `r' = r ^ (1 + rate/1000)` about the lens centre,
  sampled between pixels (bilinear) so it is not blocky.
- Magnify: resample the scene at `1/amount` about the lens centre
  (bilinear).
- Tinted grayscale: the grey as a tint of the lens colour,
  `tint + (1 - tint) * y`: black becomes the colour, white stays white,
  a black lens gives plain grays (a white one, as older files may have,
  counts as black). Choosing the lens in the panel starts it black.
- Transparency: the panel's Rate is how see-through the lens is:
  `c * rate/100 + k * (1 - rate/100)` (the lens stores the colour's share,
  `100 - rate`).
- Wireframe: the objects beneath as a wireframe: where the colour beneath
  changes (an edge) the outline colour, inside objects the fill colour,
  the bare (white) page as it is.
- Invert: `1 - c`.

- Bitmap effects: the bitmap effects (Effects menu, each with its default
  settings) run, in order, over what lies beneath, and the result shows
  inside the lens. The effects see 24 pixels past the lens's box, so
  blurs and the like do not stop short at its edge. Their settings are in
  pixels of the picture being drawn, so on screen they follow the zoom.
  Add them in the Lens panel (Add effect, ✕ removes one) or with
  Effects > Effect Picker > Apply as a lens, which adds the clicked effect
  to the selection's lens (making one when there is none). Where the
  bitmap effects are not available (the command-line converter), the
  lens shows what is beneath unchanged.

Lenses render as a second pass: the scene beneath is rasterised, the
lens function is applied inside the shape's mask, then the result is
composited.

## Checks

- Given a white page and a 50% transparency lens coloured red, then
  pixels inside the lens are (255, 128, 128).
- Given an invert lens over black, then the pixels are white.
- Given a 80% transparency lens coloured red over white, then the pixels
  are (255, 204, 204).
- Given a wireframe lens (blue outline, yellow fill) over a red square on
  white, then the square's inside is yellow, its edge blue, the page white.
- Given a red tinted grayscale lens over black, then the pixels are red.
