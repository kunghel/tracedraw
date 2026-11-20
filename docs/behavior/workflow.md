# Workflow

Tools > Workflow... opens a window where a series of picture steps is
drawn as boxes joined by wires, so the same chain of work (vectorize,
recolour, add an effect, place or save) runs again on another picture
without doing each step by hand. The model and the run are in
`apps/tracedraw/src/flow.rs`, the window in `ui/workflow.rs`.

## Steps

| Step | Takes | Gives | Settings (defaults) |
|---|---|---|---|
| Input | nothing | a picture | Selected objects (default) or a picture file |
| Vectorize | a picture (curves are drawn first) | curves | preset (Logo), most colours (0: the preset's), Remove (background colour) |
| Recolor | a picture or curves | the same kind | tint (off), hue -180 to 180 (0), saturation and lightness -100 to 100 % (0), invert colours (off) |
| Effect | a picture (curves are drawn first) | a picture | any bitmap effect of the Effects menu (Gaussian Blur) with its own settings |
| Compare | up to four results | its first input | none: shows the results wired in side by side |
| Export | a picture or curves | nothing | Place on the page (default) or Save to a file |

An Input step set to Selected objects takes, when the workflow runs, a
lone selected bitmap's own pixels; any other selection is drawn into a
picture at up to 300 dpi with its longest side at most 2400 pixels
(transparent background). A file chosen for an Input step is kept in
the saved workflow.

Vectorize traces with the Trace dialog's presets and engine
(`trace.rs`, see `bitmap-tracing.md`). Recolor applies the same colour
change as Tools > Recolor (`tracedraw_core::recolor`, see
`recolor.md`) to every pixel of a picture (its opacity kept) or to each
curve's colour and blend colours, then Invert takes each channel to
255 minus its value. Effect runs the effect as the Effects menu does
(`fx::apply_effect`).

## The canvas

- Add Step adds a step near the top left of the view and selects it;
  Delete Step or the Delete key (while no field is being typed in)
  takes the selected step and its wires out.
- A box is dragged by any part of it. A wire is drawn from a step's
  output dot (right side) to another step's input dot (left side), or
  dropped on a box (its first free input). A new wire into an input
  replaces the one there. A wire that would close a loop, leave a
  Compare or Export step, or go into a step that takes nothing is
  refused and the status line says so. Dragging an input's wire end
  picks the wire up again; right-clicking an input dot takes it off.
- One output may feed any number of steps: branches with other
  settings run side by side, and a Compare step shows up to four
  results next to each other in the settings column.
- Dragging empty space moves the view.
- The selected step's settings are in the column at the right, with its
  result (picture size or number of curves, or why it has none).

## Running

Preview runs every step and shows each result in its box without
changing the drawing. Run does the same and then carries out the Export
steps in order: Place on the page puts every result on the active layer
over the input (the selection's box, or for a file a box of the
picture's size at 96 dpi centred on the page) as one undo step named
"Workflow" and selects what it placed; curves become a group of filled
curves and outlined strokes exactly as a trace places them, pictures a
bitmap. Save to a file writes PNG, or SVG for curves when the file name
ends in `.svg`. Run on Picture File... runs the workflow once with a
picture file in place of every Input step's picture.

A step whose input failed, or that has nothing wired in, shows why in
its box (red) and passes the message down its branch; the other
branches still run. On the desktop the run is off the interface thread:
a spinner and Cancel show while it runs, and closing the window
cancels it. In the browser it runs in the frame.

The workflow, the selected step and the last results are kept while the
window is closed (until the program quits).

## Files

New starts again from the default workflow (Input, Vectorize, Export to
the page). Save... writes a `.tdflow` file: JSON with
`tracedraw_flow` (the format version, 1), `nodes` (each with `id`,
`pos` and `step`, tagged by its kind and holding its settings) and
`wires` (`from`, `to`, `port`). Open... reads one back; a file of a
newer version is refused, and wires that are not valid are left out.

## Steps added by other parts of the program

`flow::register` adds a step kind (`flow::Extension`): its stable
`kind` saved in files, its name, how many inputs it takes (0 to 4), its
settings as text fields, whether it can run now and the function that
runs it on the values wired in. Such steps are listed at the end of Add
Step (greyed while they cannot run), saved with their kind and settings
and run like the built-in ones. A saved step whose kind is not
registered keeps one input and fails with "This step is not available
in this version.". The optional AI steps are meant to come this way.
