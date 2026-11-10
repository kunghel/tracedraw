# TraceCut

[[menu.tools > menu.tools.tracecut]] opens TraceCut, the tools for cutting
plotters. It is also on the standard toolbar's Launch button. Select the
objects to work on before opening it.

The tools are listed on the left by group: Cut, Nesting, Measure and
Make. On the right are the chosen
tool's settings, a preview of the selection and what the tool finds, and
the buttons:

- **Apply** shows the result in the preview without changing the drawing;
- **OK** applies it and closes the window; anything it changes is undone in
  one step;
- **Cancel** closes the window without a change.

With nothing selected the preview asks you to select the objects to cut,
nest or measure, and Apply and OK are greyed out (Box Maker and Folded
Tray need no selection).

Folded Tray shows its pattern at once and updates it as you change a
setting; there is no need to press Apply.

The Measure tools (Perimeter and Area, Cutting Time, Cost) show their
results beside the preview at once, and update as you change the selection
or the settings; there is no need to press Apply.

To hide TraceCut, clear [[options.show_tracecut]] in Options, General.

## Zoom in the preview {#preview-zoom}

- Turn the **mouse wheel** over the preview to zoom at the pointer (with or
  without Ctrl), or **pinch** on a touch screen or touchpad.
- **Drag** with the left or middle button (or hold Space and drag) to move
  the view.
- The buttons in the preview's corner zoom in, zoom out, fit the whole
  result, zoom to the selected objects and show them at real size (1:1).
- With the pointer over the preview, **+** and **-** zoom and **0** fits.
- **Double-click** the preview to fit the whole result again.

Each tool keeps its zoom while the window is open; selecting other objects
fits the preview again.

## Presets {#presets}

Each tool keeps its settings under names you give. Pick a preset from the
list to load it. The gear button shows the row to **Save** the settings
under a name, **Delete** a preset, or go back to the **Defaults**.

## Plotter Output {#plotter-output}

Cuts the selection on a cutting plotter. Choose the **machine** (the commands
your cutter understands), the **knife offset** of your blade (often 0.25 mm),
the **overcut** that closes each outline, the **passes** for thick material,
and where the job goes:

- **File** saves a .plt file;
- **Port** sends it to a serial or USB-serial port, such as /dev/ttyUSB0 or
  COM3 (set the port's speed to the cutter's first, with your system's tools);
- **Network** sends it to the cutter's address, port 9100 unless you give
  another.

**Plan the cut order** cuts the nearest outline next and the inner outlines
first. **Test cut** cuts a small square with a triangle inside, to check the
knife before the real job. Save your cutter's settings as a preset.

## CNC Toolpath {#cnc-toolpath}

Turns the selected curves and shapes into a G-code file for a CNC router
or mill. Give the **tool diameter**: the tool runs half of it outside each
outline, so the part comes out at its drawn size, and inside each hole.
**Cut side** set to **Automatic** cuts holes from the inside and outlines
from the outside, whether a hole is part of the same curve or a separate
object; you can also cut everything outside, inside or on the line. Open
curves are always cut on the line.

The cut goes down to the **total depth** in passes no deeper than the
**depth per pass**, with the tool lifted to the **safe height** between
cuts. Set the **feed rate** and **plunge rate** in millimetres per minute
and the **spindle speed**. X0 Y0 is the bottom-left corner of the
selection or the ruler origin, Z0 the top of the material. Holes are cut
before the outline around them.

**Holding tabs** leave short bridges of material at the bottom of the
outside cuts so the parts do not move when they come free; set how many,
how wide and how thick. Apply shows the toolpaths, OK saves the .nc file.

## Weeding {#weeding}

Adds weeding lines from each object to its neighbours, or a box around each
object, and a weeding frame, so the waste peels off in small pieces.

## Bridges {#bridges}

Leaves short uncut stretches in closed outlines, so the parts stay in the
sheet until you break them out.

## Cut Order {#cut-order}

Reorders the objects and their outlines for the shortest travel of the knife,
inner outlines first, and sets where each outline starts and which way it
runs.

## CutContour {#cutcontour}

Gives the selected objects an outline in the spot color CutContour, the name
print and cut workflows look for, and can remove their fill. The name is kept
when you export to PDF, EPS or SVG.

## Registration Marks {#registration-marks}

Adds the marks a cutter's sensor reads for print and cut, around the selection
or inside the page: squares, circles or L-shaped corners, with more along long
sides.

## Find Duplicates {#find-duplicates}

Objects lying exactly on top of another one would be cut twice. Set the
**tolerance**, how far apart two outlines may be and still be the same, and
choose whether OK deletes the repeats or selects them. The lowest object
of each set stays.

## Find Crossings {#find-crossings}

Outlines that cross themselves cut badly, and objects that cross each other
usually need a weld first. TraceCut finds the crossings and shows them as red
rings in the preview. Tick **Also between objects** to find crossings between
different objects too. Under **On OK**, choose what OK does:

- **Select them** selects the objects that have crossings.
- **Fix them** rebuilds each outline that crosses itself as the outline of
  the area it fills and, with Also between objects, welds objects that cross
  each other into one.
- **Move them apart** moves objects that cross each other apart, each the
  shortest way that leaves the **Spacing** to the others. The lowest of each
  set of crossing objects stays where it is, and so do locked objects.

Each is one step of Undo.

## Eyelets {#eyelets}

In the **Make** group. Places marks for eyelets (grommets) on a banner or
sign: small circles of the **diameter** you give, one in each corner and
more along the edges. Choose whether they go along **the selection's box**
or along **each closed curve** of the selection, how far from the edge
their centres are, and whether they are **at most this far apart** or **a
number per side** (corners included). OK adds the circles as a group named
Eyelets.

## Box Maker {#box-maker}

In the **Make** group. Draws the flat panels of a box with finger joints,
ready to cut from plywood or acrylic with a laser or a CNC router. It needs
no selection. Give the **width**, **depth** and **height**, whether they are
the **outside** or **inside** sizes, the **material thickness** and the
**finger width**; tick **open top** for a box without a lid. OK adds the
five or six panels side by side at the bottom left of the page, a **gap**
apart, as a group named Box. Every finger on one panel matches a slot on
the panel it joins.

## Folded Tray {#folded-tray}

In the **Make** group. Draws the flat pattern of a tray folded from
composite panel (two thin aluminium skins round a plastic core), such as a
sign face with returns or the body of a light box. The fold lines are
routed from the back with a V-groove cutter that leaves the front skin, the
outline is cut through, and the returns are folded up by hand. It needs no
selection, and the preview shows the pattern as you change the settings.

- **Face width** and **face height** are the finished sizes, outside;
  **Size from selection** takes them from the selected objects.
- **Return depth** is how far the sides stand back from the face; tick
  **depth per side** to give the left, right, top and bottom returns
  their own.
- **Panel thickness** and the **groove allowance**, the length a fold
  takes up (with **automatic allowance**, a quarter of the thickness).
  Check it on an offcut: fold a test piece and measure it.
- **Running through**: the pair of returns that run to the corners; the
  other two stop the **corner relief** short of them, so the folds never
  bind. A relief equal to the panel thickness makes a neat butt joint.
- **Overlap flaps** add a flap at each end of the through returns that
  folds round the corner onto the next return, to be riveted.
- **Mounting ears** stand out from the back edge of the returns and fold
  out to fix the tray to a wall, each with a hole. Choose the returns,
  their width and length, how many or how far apart, and the hole's size
  and distance from the ear's fold.
- **Holes in returns**: round holes along every return, for fixings or
  cables.
- **Window**: an opening in the face for a light box, inset from the
  face's edges, with rounded corners.
- **Place**: the page's bottom-left corner or beside the selection, a
  **gap** away.

The pattern is drawn as seen from the back, the side you rout, so the
left return of the finished face lies on the right of the pattern. OK adds
it as one group named Folded tray, holding two groups: **Fold grooves**
(blue open lines, the first pass with the V-groove cutter) and **Cut**
(red closed lines: holes and the window first, then the outline, the
second pass). Select either group and use CNC Toolpath or Plotter Output
for each pass. The last settings are kept for next time, and presets save
your own.

## Nesting {#nesting}

**Shape Nesting** places the selected objects on sheets or a roll by their
true outlines, turning them and fitting small ones into holes, to waste as
little material as possible. **Rectangle Nesting** does the same by their
boxes, faster. Set the sheet or roll size, the margin and the spacing between
objects; OK moves the objects onto the material and can draw the sheets.

## Measuring {#measuring}

**Perimeter and Area** gives the outline length and the area of the
selection, holes taken away. **Cutting Time** works out how long the cutter
takes, from its speeds. **Cost** prices the job: the material by area or by
the length of roll, cutting, weeding, pieces and machine time. These tools
change nothing in the drawing.
