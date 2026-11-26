# Scripts panel

A script is JavaScript written against an object model of the drawing:
documents, pages, layers, shapes, their fills, outlines, curves and nodes.
Every change a script makes goes through the same commands the interface
builds, so a whole run is one undo step, named `Script` in the history.

Tools > Scripts opens the panel: a code editor with a line margin, the
output of the last run, **Run**, **Debug** (see `behavior/script-debugger.md`),
**Record**, **Load...**, **Save...** and **Install as Filter** (see
`behavior/script-filters.md`). Recorded macros are plain scripts. A
text's font, size, weight, slant and character background set in the
Text panel record as `set_text_style` when all its characters share
them (#3518).

A **JavaScript / VBA** pair of buttons above the editor picks the language.
JavaScript is the object model described here. VBA writes the snippet as a
macro written for a drawing suite, which is compiled and run over the macro
object model instead (see `behavior/vba.md`); the desktop only.

## Units and conventions

- Lengths in millimetres, page coordinates: the origin is the bottom left
  corner of the page, x grows to the right and y upwards.
- Angles in degrees, counter-clockwise.
- Text sizes in points.
- Items are counted from 1, as the macro object model counts them:
  `Pages.Item(1)`, `Pages.Item(n).Layers.Item(1)`, `Shapes.Item(1)`,
  `Nodes.Item(1)`, `SubPaths.Item(1)`, `Effects.Item(1)`.
- `Shape.PositionX` and `Shape.PositionY` are the bottom left corner of the
  object's bounding box; `CenterX` and `CenterY` are its centre.
- `Shape.SetOpacity(pct)` takes a transparency: 0 is opaque, 100 invisible.
  `Shape.Transparency` reads it back in the same numbers.

## Running a script

**Run** evaluates the panel's script. What it prints with `print(text)` (or
`console.log`) goes to the panel's output line by line. When the script
prints nothing, the value of its last expression is shown instead, so a
one-liner such as `ActivePage.Shapes.Count` answers with a number.

An error stops the script and is shown with its message. The changes made
before the error are kept, as one undo step.

`MsgBox(text)` prints its text to the output; there is no dialog.

### Asking the user for a value

`InputBox(prompt, title, default)` and `prompt(message, default)` show
the app's input box (the one VBA macros' `InputBox` uses) and wait for
the answer. `InputBox` gives the typed text, or `""` for Cancel; `prompt`
gives the typed text, or `null` for Cancel. A script or plugin that uses
either runs on a thread of its own while the box is up, so the drawing
keeps the changes it made so far on screen; all of them are still one
undo step. Where nobody can answer (scripts written by the AI assistant,
file filters), both give the default at once (`""` without one). (#4139)

## A first script

A fan of twelve rounded bars, each a little more orange than the one
before. This is the example in `README.md`, word for word, and
`scripting::tests::the_readme_example_draws_a_fan_of_bars` runs it.

```js
for (let i = 0; i < 12; i++) {
  const bar = ActiveLayer.CreateRectangle(80, 120, 60, 12, 3);
  bar.Fill.ApplyUniformFill(Color.RGB(255, 60 + i * 15, 60));
  bar.Rotate(i * 15);
}
```

Reading a drawing works the same way: `ActivePage.Shapes.All` is a
`ShapeRange` of everything on the page, and each object answers with its
own properties.

```js
let widest = 0;
ActivePage.Shapes.All.forEach(s => {
  if (s.SizeWidth > widest) widest = s.SizeWidth;
});
print(`${ActivePage.Shapes.Count} objects, the widest ${widest.toFixed(2)} mm`);
```

## The object model

The model is built in JavaScript on top of one native call,
`__td(method, arguments)`, which the page does not need to use. Each method
below names the call it makes.

### Application and the active document

| Name | Meaning |
|---|---|
| `Application.ActiveDocument` | the drawing in the window |
| `Application.ActivePage` | its current page |
| `Application.ActiveLayer` | the current layer of that page |
| `Application.ActiveSelection` | the selected objects |
| `Application.ActiveShape` | the first selected object, or `null` |
| `Application.Version` | the program's version, as text |
| `Application.Name` | `'TraceDraw'` |
| `Application.CreateColor` | the `Color` class |
| `Application.ZoomToFit()` | View > Zoom To Fit: every object in the window |
| `Application.ZoomToPage()` | View > Zoom To Page |
| `Application.ZoomToSelection()` | View > Zoom To Selection |
| `Application.CopyVisibleArea()` | Edit > Copy Visible Area: what the window shows, as a picture on the clipboard |

The view calls take effect when the script ends, after its objects are in
the drawing, in the order the script made them (#4020). The macro
recorder records Zoom To Fit, Zoom To Page, Zoom To Selection and Copy
Visible Area as these calls, in their place among the other steps.

`ActiveDocument`, `ActivePage`, `ActiveLayer` and `ActiveSelection` are also
global, so a script can start with either form.

### Document

| Name | Meaning |
|---|---|
| `Name` | the drawing's title |
| `ActivePage`, `ActiveLayer` | the current page and its current layer |
| `Pages` | the document's pages |
| `Selection`, `SelectionRange` | the selected objects as a `ShapeRange` |
| `SelectAll()` | selects every object of the current page |
| `ClearSelection()` | selects nothing |
| `AddPages(n)` | adds `n` pages (1 when omitted) and returns the last |
| `CreateRectangle(x, y, w, h, radius)` | on the current layer |
| `CreateEllipse(x, y, w, h)` | on the current layer |
| `CreateArtisticText(x, y, text, font, sizePt)` | on the current layer |
| `CreatePath(pathData)` | on the current layer |
| `Data`, `Properties` | the drawing's own name and value pairs (#4137), see below |
| `Export(path, format)` | writes the drawing to a file; the file name's extension picks the format (`svg`, `pdf`, `eps`, `ps`, `prn`, `dxf`, `emf`, `wmf`, `plt`, `psd`, `html`, `png`, `tdraw`), and `format` is accepted but not used |

`BeginCommandGroup(name)` and `EndCommandGroup()` are accepted and do
nothing: a script's changes are one undo step already.

`ActiveDocument.Data` keeps text values by name in the drawing itself, so
a script or plugin can store its settings, presets or calibration with the
file instead of in hidden objects. `Get(name)` gives the value (`""` when
there is none), `Set(name, value)` adds or changes it, `Has(name)`,
`Remove(name)` and `Clear()` do what they say, `Count` and `Names` list
them, `Item(indexOrName).Value` reads or writes one (1-based index), and
a `for ... of` loop walks them. Each change is an undo step of the script
("Document Data"); the pairs are saved in `.tdraw` files. Macros reach
the same pairs as `ActiveDocument.Data` or `ActiveDocument.Properties`.

### Pages, layers

`Pages` has `Count`, `Item(i)` and `forEach(f)`.

`Page` has `Name`, `SizeWidth`, `SizeHeight`, `Index`, `Layers`,
`ActiveLayer`, `Shapes`, `SetSize(w, h)`, `CreateLayer(name)` (returns the
new layer) and `Activate()`.

`Page.Shapes` is narrower than a layer's: it offers `All` (a `ShapeRange`
of every object on the page) and `Count`.

`Layers` has `Count`, `Item(i)`, `forEach(f)` and `Find(name)` (a `Layer`,
or `null`).

`Layer` has `Name` (read and set), `Visible` (read and set), `Shapes`,
`Activate()`, and the create methods:

| Method | Object |
|---|---|
| `CreateRectangle(x, y, w, h, radius)` | rectangle, `radius` 0 for square corners |
| `CreateEllipse(x, y, w, h)` | ellipse in that bounding box |
| `CreatePolygon(x, y, w, h, points)` | regular polygon |
| `CreateStar(x, y, w, h, points, sharpness)` | star, `sharpness` 0 to 1 |
| `CreateArtisticText(x, y, text, font, sizePt)` | text at the baseline start |
| `CreateParagraphText(x, y, w, h, text, font, sizePt)` | text in a frame |
| `CreateLineSegment(x1, y1, x2, y2)` | straight line |
| `CreateCurve(points, closed)` | straight segments through `[[x, y], ...]` |
| `CreatePath(pathData)` | SVG path data, `M L H V C S Q T A Z` |
| `ImportBitmap(path)` | picture from a file |

Every create method returns a `Shape`. New objects go on the layer in front
of the others, with no fill and a thin black outline; text is black with no
outline.

### Shapes and ranges

`Shapes` (a layer's) has `Count`, `Item(i)`, `All` (a `ShapeRange`),
`FindShape(name)` (a `Shape`, or `null`) and `forEach(f)`.

`ShapeRange` is a set of objects, made by `new ShapeRange([id, id, ...])`:

| Name | Meaning |
|---|---|
| `Count`, `Item(i)`, `Shapes`, `forEach(f)` | the objects in it |
| `Delete()`, `Move(dx, dy)` | on all of them |
| `Group()` | groups them, returns the group as a `Shape` |
| `CreateSelection()` | selects them |
| `AlignLeft()`, `AlignRight()`, `AlignTop()`, `AlignBottom()` | to the last object in the range |
| `AlignCentersHorizontally()`, `AlignCentersVertically()` | centres on the last object |

### Shape

| Name | Meaning |
|---|---|
| `Name`, `Text` | read and set |
| `Font`, `FontSize`, `Bold`, `Italic` | a text's style, read and set; `SetTextStyle({font, size, bold, italic, background})` sets several at once (`background` a fill, or `null` for none) |
| `TextBackground` | the colour behind a text's characters, apart from their fill, or `null`; set to a colour or `null` (#3518) |
| `Type` | `Rectangle`, `Ellipse`, `Polygon`, `Star`, `ComplexStar`, `Curve`, `Text`, `Group`, `Bitmap`, `ClipFrame`, `Table`, `Symbol` |
| `PositionX`, `PositionY` | bottom left of the bounding box |
| `SizeWidth`, `SizeHeight` | the bounding box's size |
| `CenterX`, `CenterY` | its centre |
| `Transparency` | from 0 (opaque) to 100 |
| `PathData` | the outline as SVG path data, in page millimetres |
| `Fill`, `Outline` | see below |
| `Curve`, `Effects` | see below |
| `SetPosition(x, y)` | places the bounding box's bottom left corner |
| `SetSize(w, h)` | resizes the bounding box |
| `Move(dx, dy)`, `Rotate(deg)`, `Stretch(sx, sy)`, `Skew(ax, ay)` | transforms; `Rotate` turns about the centre |
| `Duplicate(dx, dy)` | returns the copy |
| `Delete()` | removes the object |
| `ConvertToCurves()` | returns the same object, now a curve |
| `OrderToFront()`, `OrderToBack()` | in its layer |
| `SetOpacity(pct)` | transparency, 0 opaque to 100 invisible |
| `AddDropShadow(dx, dy, opacity)` | a drop shadow, `opacity` in percent (50 when omitted) |
| `CreateSelection()`, `AddToSelection()` | selects it, or adds it to the selection |

### Fill, outline, colour

`Color.RGB(r, g, b)` takes 0 to 255, `Color.CMYK(c, m, y, k)` and
`Color.Gray(v)` take 0 to 100 (0 black, 100 white). `Color.Hex` reads a
colour as `"#rrggbb"`.

`Shape.Fill` has `Type`, `UniformColor`, `ApplyUniformFill(color)`,
`ApplyFountainFill(from, to, angle)` and `ApplyNoFill()`.

`Shape.Outline` has `Width`, `Color` (read and set),
`SetProperties(widthMm, color)` and `SetNoOutline()`.

### Curves and nodes

`Shape.Curve` has `Closed`, `SubPaths` and `Nodes`.

`SubPaths` has `Count`, `Item(i)` and `forEach(f)`; a `SubPath` has `Closed`,
`Index` and `Nodes`.

`Nodes` has `Count`, `Item(i)`, `First`, `Last` and `forEach(f)`; a `Node`
has `PositionX`, `PositionY`, `SubPathIndex`, `IsEnding`, and its `list` can
be narrowed to one subpath.

`Node.Type` reads and sets `cusp`, `smooth` or `symmetrical`. A node can be
moved with `SetPosition(x, y)` or `Move(dx, dy)`, taken out with `Delete()`,
and one added before it with `AddNodeBefore(t)`, `t` being where along the
segment to put it (0.5 when omitted).

A curve's `Nodes` lists every node of the object in path order, whichever
subpath each belongs to; `Node.SubPathIndex` says which, counting from 1.

### Effects

`Shape.Effects` has `Count`, `types`, `Item(i)`, `Add(effect)`,
`Remove(i)`, `Clear()`, `AddExtrude(dx, dy)` and
`AddBlockShadow(dx, dy, color)`.

`Add` takes the effect as it is written in a `.tdraw` file, so any effect the
program has can be added by passing its object. The effects that are live
effects counted here are the ones the Effects docker lists, and an object's
drop shadow is not one of them: `Shape.AddDropShadow(dx, dy, opacity)` sets
the object's shadow, which `document_info` reports apart from `effects`.
So `AddDropShadow` leaves `Effects.Count` as it was.

### Dimensions

`Layer.CreateParallelDimension(x1, y1, x2, y2, tx, ty)`,
`CreateHorizontalDimension` and `CreateVerticalDimension` (same arguments)
make a live dimension from (x1, y1) to (x2, y2), its line through
(tx, ty), in the Dimension tool's style, font and units: the same object
the tool draws, which follows the objects it measures when they are
edited: a point within 0.01 mm of a node or an edge of an object on the
page holds to it, as the tool's points hold within 3 pixels.
`CreateLinearDimension(kind, ...)` takes the kind as `'parallel'`,
`'horizontal'`, `'vertical'` or `'auto'` (the larger span).
`CreateAngularDimension(cx, cy, x1, y1, x2, y2, tx, ty)` measures the
angle at (cx, cy) from the first point to the second, its arc through
(tx, ty). `Shape.Dimension` gives `kind`, `value` (mm or degrees),
`text` and `points`, or `null` for other objects (#2348).

VBA macros have `Layer.CreateLinearDimension(Type, Point1, Point2,
TextCentered, TextX, TextY)` with `cdrDimensionHorizontal` (0),
`cdrDimensionVertical` (1) and `cdrDimensionSlanted` (2), and
`Layer.CreateAngularDimension(Center, Point1, Point2, TextX, TextY)`,
and `CreateHorizontalDimension`, `CreateVerticalDimension` and
`CreateParallelDimension(x1, y1, x2, y2, TextX, TextY)` with the points
as numbers, as scripts name them (#3517). A
point is anything with `PositionX`/`PositionY`, `{x, y}` or `[x, y]`, in
the document unit; without TextX/TextY the line goes 10 mm off the
points. A recorded macro records a dimension the Dimension tools drew
(and those Add Dimensions draws) as one of these calls, so playing it back
makes a live dimension again; they went unrecorded until 0.48 (#2348).

A dimension's style is set with `Shape.SetDimensionStyle(style)` (also on
a `ShapeRange` and `ActiveSelection`), `style` naming the settings as a
style is saved: `text_size` (points), `font`, `precision`, `prefix`,
`suffix`, `line_width`, `text_color` and the others; objects that are not
dimensions are left alone, a name the style does not have is an error,
and it answers how many dimensions changed. `Shape.LabelSize` reads and
sets the label's size in points (`null` for other objects). A change made
on the Dimension bar while a macro is recorded is recorded as
`ActiveSelection.SetDimensionStyle({...})` with the settings it changed,
so it plays back on the dimension the macro just drew (#4026).

### Symbols

`Document.Symbols` lists the drawing's symbols (`Index`, `Name`).
`Shape.CreateSymbol(name)` and `ShapeRange.CreateSymbol(name)` make a
symbol from the objects, which an instance replaces where they were.
`Layer.PlaceSymbol(nameOrIndex, x, y)` places an instance with its corner
at (x, y). `Shape.Children` lists a group's objects (each a `Shape` with
`IsText`, `ChildType` and `Box`).

`Shape.SymbolizeSimilar(name)` (#2343), on a group, turns every top-level
group that draws the same (its objects that are not text: their kind, box
from the group's corner, to 0.1 mm, and fill) into an instance of one
symbol: an existing symbol that draws the same, else a new one from this
group. A group with text becomes a group of the instance and its text, so
each label stays editable. It returns `replaced`, `with_text`, `skipped`,
`symbol_index`, `symbol_name` and `ids`. A recoloured or reshaped copy is
left as it is.

## Limits

A script cannot remove or rename a page or a layer, or ungroup a group:
the commands exist in the engine, but the object model has no method for
them. A script also cannot undo or redo; the history belongs to the
interface.

Scripts an AI service writes (AI Drawing, and the check when a script filter
is installed) run with limits instead: no file access, a bounded number of
loop iterations, calls and changes, and nothing kept unless all of it runs.
Those limits are in `scripting.rs`, and `behavior/ai-drawing.md` describes
what the user sees.

## Checks

- `scripting::tests::script_creates_and_moves_a_rectangle`
- `scripting::tests::the_readme_example_draws_a_fan_of_bars`
- `scripting::tests::script_error_is_reported`
- `scripting::tests::curves_show_their_subpaths_and_nodes`
- `scripting::tests::effects_are_listed_and_removed`
- `scripting::tests::selection_round_trip`
- `ai::tests::the_reference_names_only_methods_the_object_model_has` holds
  the description in `ai.rs` to the model
- `scripting::extra::tests::a_script_makes_live_dimensions`
- `scripting::extra::tests::a_vba_macro_makes_a_linear_dimension`
- `scripting::extra::tests::a_recorded_dimension_plays_back_as_one`
- `scripting::extra::tests::dimensions_drawn_while_recording_are_recorded`
- `view_steps::tests::the_recorder_keeps_the_zoom_steps`
- `scripting::extra::tests::a_dimension_label_size_change_is_recorded_and_plays_back`
- `scripting::extra::tests::scripts_and_macros_set_a_dimensions_label_size`
- `view_steps::tests::a_recorded_zoom_to_fit_plays_back`
- `view_steps::tests::scripts_and_macros_zoom_to_fit_the_objects`
- `scripting::extra::tests::a_script_dimension_follows_its_object`
- `scripting::extra::tests::similar_groups_become_one_symbol_and_keep_their_text`
- `scripting::extra::tests::a_script_creates_and_places_symbols`
