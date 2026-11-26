# Mockup

Effects > Mockup opens the Mockup panel, which lays the selected design on
a product photo so a client can see how the print will look, and saves
the result as a JPG for approval.

## Products

Sixteen ready photos, drawn by the program itself (`mockup_photos.rs`:
shapes, light and texture, no picture files, no brands), each with a print
area:

| Product | Print area | Ink |
|---|---|---|
| White, black and grey T-shirt | The chest, a flat area brought 4 % forward in the middle | Multiply on white and grey, Normal on black |
| White and black mug | Round the body: a cylinder, 140 degrees of it facing the eye | Multiply on white, Normal on black |
| Steel bottle | Round the body, 130 degrees | Multiply |
| Tote bag, framed poster, shop sign, van side, business cards, flyer, phone case, product box | A flat area in perspective | Multiply |
| Cushion | A flat area brought 6 % forward in the middle | Multiply |
| Flag | The waving cloth | Multiply |

My own photo uses the picture selected together with the design: the
print area is that picture's middle (20 % in from each side) and the
design can then be fitted with the Envelope tool.

## Apply

With objects selected, Apply makes one group named "Mockup: <product>"
holding the photo (1600 by 1200 pixels, 90 % of the page's width or
height, centred on the page, kept as JPEG in a saved drawing) and the
design, a group of the selected objects (each keeps its own place on the
page). The design gets three live effects, editable afterwards:

1. Mesh Envelope, 8 by 8 cells: each grid point is the print area's point
   `(u, v)`; the design fills `Size` of the area along its tighter side,
   keeping its width over height `d` against the area's printed
   proportions `a` (`d > a`: width `Size`, height `Size * a / d`; else the
   other way), centred across, placed down at the product's anchor
   (T-shirts near the top), moved by Across and Up and down (fractions of
   the area). A flat area maps the unit square onto its four corners in
   perspective (a projective map, so the middle is where the diagonals
   cross); a cylinder takes `x = cx + r sin t`, `y = top + (bottom - top) v
   + ry cos t` with `t = (u - 0.5) span`; a bulge scales each point from
   the middle by `1 + bulge sin(pi u) sin(pi v)`.
2. Displace with the photo's lightness under the bent design (0.5 mm
   cells), pushing the outline by the product's amount times Texture, and
   its shadows and highlights onto the print (Both) by the product's
   strength times Shading.
3. On light products (or with Ink: Multiply), uniform transparency 0 %
   with the Multiply merge mode, so the texture shows through the ink.

The selection is replaced by the group in one undo step ("Mockup"), and
the group is selected. With a mockup selected, Apply builds it again with
the chosen product and settings from the same design, at the same place
on the page. With nothing to lay on the product, the status line says to
select a design (or, for My own photo, a picture with it).

## Settings

| Control | Default | Range |
|---|---|---|
| Size | 85 % | 10 to 100 % |
| Across, Up and down | 0 % | -50 to 50 % |
| Shading | 100 % | 0 to 200 % of the product's |
| Texture | 100 % | 0 to 300 % of the product's |
| Ink | As the product | As the product, Multiply, Normal |
| Caption | empty | any text |
| JPG width | 1600 px | 200 to 6000 px |

The panel shows every product's photo (click to choose) and the chosen
one larger with the outline the selected design will take on it.

## Export JPG

Export JPG renders the selected mockup's photo area (or, with no mockup
selected, the selection's bounds) at JPG width pixels, over white, JPEG
quality 92, and asks where to save it (`mockup.jpg`). A caption adds a
white band 8 % of the picture's height (at least 24 pixels) under it with
the caption written in the text font, dark grey.

Tests: `mockup_gen_tests.rs`.
