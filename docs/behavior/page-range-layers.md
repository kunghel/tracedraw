# Page range layers

The Objects panel has a **Range Pages** group beside the Master Page.
Each layer in this group shares its objects across a chosen set of page
positions. A chapter header or logo can be edited once and appear on every
page in that layer's range.

## Create and edit

- Use **New Range Layer...** in the Range Pages group's menu or the New
  Master Layer button's menu. The empty group also offers a creation button.
- Enter a layer name and page ranges such as `1-8, 12, 15-20`. The current
  page is selected initially. The dialog previews how many pages currently
  match, and keeps invalid input available for correction.
- Positive page numbers and inclusive ranges are accepted. Commas,
  semicolons and whitespace separate entries. Reversed ranges are put in
  order, and overlapping or adjacent ranges are merged.
- Empty input, zero, malformed entries and numbers above `u64::MAX` keep
  the dialog open with a validation message. Every part must be valid.
- The range may include future pages. A range with zero current matches
  stays in the drawing and has a status message explaining how to reach
  an applicable page.
- **Properties** edits a layer's name, range, visibility, printing, locking
  and position in front of or behind page layers. Applying a change is
  one undo step. Cancel keeps the saved values.
- A master layer can be given a custom range. Choosing All, Odd or Even
  pages changes it back to an ordinary master layer in the Master Page
  group. Its objects stay in the layer.

## Page positions and drawing

Ranges use the document's current page order, starting at 1 and including
cover pages. Printed page numbers and names are separate document settings.
Inserting, deleting or reordering pages keeps the range's numeric endpoints,
so the layer applies to the positions selected in the new order.

Each range layer has one set of objects. The canvas, object picking,
thumbnails, printing and export use the same page applicability check.
Layers outside the current page's range are dimmed in the Objects panel.
The layer's visibility, printable, lock and front/back settings retain
their usual meanings. Several range layers can overlap and follow their
normal stacking order.

Creating or selecting a range layer can move to its first matching page
so its objects can be edited. A range with no matching page reports that
condition. The Range Pages group can be collapsed for the session.

Selecting another range layer finishes the source layer's active editing
mode before changing the target. Page navigation finishes edits on range
layers excluded from the destination page, including pending text and
curves, and cancels pending crops. Shift selection in the panel skips
range objects excluded from the current page.

## Saving and output

The native `.tdraw` file stores the range on the shared layer. Older
drawings without a range field keep their existing master-layer behavior.
Older app versions that can read the file ignore the range field and
cannot retain its page restrictions.
Saving the full drawing keeps those shared layers editable. An exported
page subset is materialized as page content for its selected output pages.

Partial-page export and printing resolve range layers against the original
page positions before making an output document. Page-number and table-of-
contents fields are resolved in that same original document. Selecting
pages 5 and 7 therefore keeps the content assigned to pages 5 and 7.

CDR export places the applicable shared content on each output page. Those
copies are ordinary page objects when the file is read again. The native
format keeps the editable shared range layers.
A blend whose linked endpoint is outside an output page keeps its visible
steps there. Its CDR copy is static when that file is opened again.

## Implementation

`PageRanges` holds normalized inclusive intervals with 64-bit endpoints.
Parsing happens when the range is edited. `Layer::applies_to` tests a
0-based page position with a binary search and no allocation, and
`Document::layers_for_page` applies it for shared drawing consumers.

`Command::AddRangeLayer` creates a layer atomically.
`Command::SetLayerPageRange` changes its range, and `SetLayerScope`
selects an ordinary master scope. Changes use the engine's existing
document history for undo and redo.

## Checks

- `page_range::tests` covers parsing, normalized file data, native-file
  round trips, commands, undo/redo and page-order changes.
- `spreads::tests::range_output_*` covers original page positions,
  field freezing, stacking and linked-object identifiers in output copies.
- `ui::objects_docker::range_layer_tests` covers creation, editing,
  validation, cancellation, target-layer navigation and editing state.
- `ops2::tests` covers scoped page numbers and closing excluded editing
  modes, including text, tables, curves, symmetry and pending crops.
- The range tests in export, Publish PDF, renderer and IO exercise subset
  output, multi-document output, images, thumbnails, SVG, HTML and PDF.
- `write::tests::page_range_tests` checks geometry, stacking and page
  fields through CDR versions 7 through 27, including legacy layer slots.
