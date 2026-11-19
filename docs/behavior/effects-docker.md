# Effects docker

Window > Dockers > Effects (or the "..." under the docker tabs) lists the
effect stack of the first selected object: its live effects (contour,
blend, distortion, envelope, perspective, extrude, bevel, block shadow,
inner and outer shadows, glows, color and gradient overlays, Gaussian
blur, fill opacity, symmetry, lens, transparency) in the order they
apply, from the top down. Code: `ui/effects_docker.rs`; the stack in
`tracedraw_core::document::Shape::effect_stack`. Above the stack, the
Layer effects section switches and sets the stackable looks
(`layer-effects.md`, #1141).

Each row:

- The check box switches the effect on or off. An effect switched off is
  kept with the object, at its place in the stack (`parked_effects` in
  `.tdraw`), and is not drawn, exported or printed; switching it on puts
  it back where it was.
- The name, dimmed while the effect is off.
- Up and down arrows move the effect one place earlier or later in the
  stack (disabled at the ends).
- The cross removes the effect.

Every change is one undo step ("Effects"). Each effect is still edited
in its own panel or tool (Extrude, Contour, Envelope and so on).

Without a selection the docker says to select an object; an object
without effects says it has none.

## Presets

Under the stack, Presets (#2160) keep looks to give to any object later,
in any drawing (kept in the settings, `style_presets`, at most 200):

- A name box and Save Preset (enabled with a selection) save the look of
  the first selected object: fill, outline, transparency (opacity), drop
  shadow, and its contour, bevel, block shadow, inner shadow, lens and
  transparency effects, and its layer effects (outer shadows, glows,
  overlays, Gaussian blur, fill opacity). Effects made of the object's own points
  (blend, envelope, perspective, extrusion, distortion, symmetry) are
  not saved. A blank name gets "Preset n", the first n free.
- Each preset has a row: a 40-point picture of the first selected object
  wearing the preset (a 30 x 20 mm rounded rectangle when nothing or a
  group is selected), drawn again only when the drawing, the selection or
  the preset changes, on a checkerboard; its name; Apply; and a cross
  that deletes it.
- Apply (or a click on the picture) gives the selected objects the
  preset's fill, outline, opacity and drop shadow, and replaces their
  saved-kind effects by the preset's, kept after the effects of their
  own kind that stay (an envelope stays, a block shadow is replaced, not
  doubled). It is one undo step ("Apply Preset").

## Built-in looks

Above the saved presets, Built-in looks shows five looks made by the
program (`builtin_looks.rs`), a 40-point picture each in a wrapping row,
drawn like the presets' pictures. A click on one (with a selection) gives
the selected objects the look exactly as Apply gives a preset, in one undo
step ("Apply Preset"); they cannot be deleted and are not in the settings.

| Look | Fill | Outline | Effects |
|---|---|---|---|
| Chrome | linear gradient at 90°: ground, a dark horizon, a pale sky | 0.2 mm dark grey | bevel (emboss, 0.8 mm, light at 135°, 4 levels, by direction) |
| Liquid metal | silver linear gradient at 70°, mirrored twice, smooth | none | soft-edge bevel 1.6 mm (10 levels, by direction), white inner glow, soft outer shadow |
| Gold | gold linear gradient at 80° | 0.2 mm dark gold | bevel (emboss, 1 mm, 6 levels, by direction) |
| Liquid glass | white | 0.25 mm white | bitmap effects lens (Gaussian blur 6 of what lies beneath), fill opacity 18 %, white inner glow, soft outer shadow |
| Clear glass | pale blue | 0.25 mm white | fill opacity 12 %, white inner glow, soft outer shadow |

## Checks

- `builtin_looks::tests::every_look_is_portable_and_has_its_own_number`
- `builtin_looks::tests::every_look_draws_a_picture`
- `builtin_looks::tests::applying_a_look_is_one_step`

- `style_presets::tests::a_preset_keeps_the_look_and_leaves_the_shape_bound_effects`
- `style_presets::tests::presets_apply_to_the_selection_as_one_step_and_undo`
- `style_presets::tests::the_picture_shows_the_preset`

- `command::tests::effects_switched_off_keep_their_place_in_the_stack` (core)
- `ui::effects_docker::tests::the_stack_switches_moves_and_removes_effects`
