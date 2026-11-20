# 3D Revolve and Inflate

3D Revolve and Inflate gives a flat drawing a shaded 3D look made of
ordinary vector shapes: spin a profile into a vase or a bottle with your
label wrapped round it, or puff a shape up like a cushion.

## Making a 3D object {#make}

Open the [[docker.shape3d]] panel with
[[menu.effects > menu.effects.shape3d]].

1. Select a curve or shape. Its fill colour (or outline colour) is the
   colour that gets shaded.
2. Choose the mode:
   - [[docker.shape3d_revolve]] spins the curve round a vertical axis at its
     left edge, like a potter's wheel: draw half the outline of a vase,
     bottle or glass (half a circle makes a ball). Pick a
     [[docker.shape3d_position]] (front, off-axis or isometric views) or set
     the [[docker.shape3d_tilt]] (how far you look down on it),
     [[docker.shape3d_turn]] and [[docker.shape3d_roll]]. Set the
     [[docker.shape3d_segments]], the [[docker.shape3d_sweep]] (less than 360
     cuts it open), the [[docker.shape3d_offset]] (the axis away from the
     curve makes a ring), [[docker.shape3d_axis_right]] and
     [[docker.shape3d_cap]] (closes the ends).
   - [[docker.shape3d_inflate]] puffs the shape up; [[docker.shape3d_depth]]
     sets how high.
3. Choose the [[docker.shape3d_surface]]: [[docker.shape3d_surface_plastic]]
   (lit, with a highlight), [[docker.shape3d_surface_diffuse]],
   [[docker.shape3d_surface_flat]] or [[docker.shape3d_surface_wireframe]].
   Set the light: [[docker.shape3d_light_angle]],
   [[docker.shape3d_light_height]], [[docker.shape3d_shine]],
   [[docker.shape3d_light_color]], [[docker.shape3d_intensity]],
   [[docker.shape3d_softness]] (a soft light reaches further round) and
   [[docker.shape3d_ambient]] (off, the dark side goes black).
4. To wrap art onto the surface, select the art (a logo, a label, text
   converted to curves) and click [[docker.shape3d_art]], then select the
   curve again, or pick one of the document's symbols in
   [[docker.shape3d_symbol]]. Place it with [[docker.shape3d_art_u]] (50 is the front),
   [[docker.shape3d_art_v]] and [[docker.shape3d_art_size]]. Art turned to
   the back is hidden. [[docker.shape3d_fit]] stretches it over the whole
   surface (stripes all the way round a ball); [[docker.shape3d_stretch]]
   stretches it along the curve; [[docker.shape3d_shade_art]] off keeps its
   own colours. With
   [[docker.shape3d_hide_surface]] only the art is drawn, not the surface.
5. Click [[docker.apply]]. [[docker.shape3d_keep]] keeps the original curve.

The result is one group of shapes, selected, and one undo removes it.

## Changing it later {#edit}

The 3D object keeps its curve and settings. Select it and the panel shows
them: change anything and click [[docker.shape3d_update]] to make it again
in place. [[docker.shape3d_release]] turns it back into the curve, to edit
the curve itself and apply again.

## Halftone sphere {#halftone-sphere}

[[docker.shape3d_halftone_sphere]] makes a ball of halftone dots in one
click, no selection needed: set how many [[docker.shape3d_halftone_dots]]
go across and click [[docker.shape3d_halftone_create]]. The dots, large in
the middle and small at the rim, are wrapped onto the front of a sphere
80 mm across and lit by the light settings above (and the tilt and turn).
Only the dots are drawn, as vector curves you can recolour or edit.
