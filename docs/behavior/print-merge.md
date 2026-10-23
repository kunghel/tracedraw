# Print Merge

File > Print Merge > Create/Load Print Merge opens the dialog; Edit Print
Merge reopens the records kept with the session, Perform Merge runs them
again. `print_merge.rs` holds the model and the merge.

## Records

The dialog: "Add columns or records manually or import data from a file"
with Import File; under "Add or edit records" Add Column, Configure
Columns and View Single Record; the records; + and - (add a record, delete
the current one) and the navigation (first, previous, next, last, Go to
Record); Save Data Source, Finish and Cancel.

- Import File reads a CSV whose first line names the fields (comma or
  semicolon, whichever the first line has more of), or an Excel workbook
  (`.xlsx`): its first sheet, shared, inline and formula strings, numbers
  as plain text (whole numbers without a decimal point), TRUE and FALSE,
  empty rows and trailing empty cells left out (`tracedraw_io::xlsx`,
  #377). The field types are guessed: a name holding photo, picture,
  image, foto, imagem, img or logo, or cells that all name picture files,
  gives Picture; a name holding QR gives QR code; the rest Text. Configure
  Columns opens after an import to check them
  (`field_types_are_guessed_from_the_data`). Save Data Source
  writes the records back as CSV, counters written out.
- Add Column asks for a name and a type and adds the field to every
  record (and a first record when there is none).
- Configure Columns shows each field's name (edited in place), type,
  counter settings and the x that removes it.
- View Single Record shows the current record as a form; View All
  Records goes back to the grid, where a click on a record's number makes
  it the current one.
- Types:
  - Text: put in place of `<Field>` in every text object of the page.
  - Number: a counter, not typed in: Start, then Step more for each
    record, padded with zeros to Digits digits (start 7, step 3, digits 3
    gives 007, 010, 013).
  - Picture: the cell is the path of a picture (relative paths start at
    the data file's folder). A text object holding only `<Field>` becomes
    the picture, as large as fits in the object's box (the frame of
    paragraph text) keeping its proportions, rotated with it.
  - QR code: a text object holding only `<Field>` becomes a black QR
    code of the cell's text, square, as large as the box allows.
  An empty cell, or a picture that does not load, removes the field's
  object from that record's page.
- Fields are found in groups and in clip frames too. A picture
  placeholder inside a clip frame becomes the picture covering the
  frame's box (as small as covers it, centred, cut by the frame); a
  rectangle, ellipse or curve named `<Field>` of a picture field (on the
  page, not in a group) takes the picture inside it, covering it, as a
  clip frame (#378, `fields_in_groups_and_frames_are_filled`). The page
  copies get new ids all through, clip frames included, so filling a
  copy never changes the template.
- The placeholder buttons insert `<Field>` into the text being edited.

## The Print Merge docker

Finish keeps the records and opens the Print Merge docker (also Window >
Panels > Print Merge), as in the reference program; it no longer merges
at once (#437). From top to bottom:

- Create (greyed while records are loaded) opens the dialog on new
  records, Edit on the ones kept.
- Text, Image or QR code: the kind of field Insert Field makes.
- Data source columns: the field to insert.
- For Image: Pictures folder (#460): the folder picture cells that are
  relative paths are looked for in first, before the data file's folder
  (the folder button picks it, the cross forgets it); a cell naming a
  picture without an extension ("1024") finds it with .jpg, .jpeg, .png,
  .webp, .bmp, .gif, .tif or .tiff
  (`each_record_gets_its_photo_from_the_pictures_folder`). Then
  Placeholder image (a picture that stands in for the merged
  ones on the page; the folder button picks it) and Align and scale: a
  grid of nine anchor points (the point of the placeholder the picture
  is placed by) and Actual size (its own size: its resolution, else
  96 dpi), Fit proportionally (the default), Fill proportionally (covers
  the placeholder, cut by it) or Stretch to fill.
- Insert Field: Text puts `<Field>` at the caret of the text being
  edited, else makes a text holding it in the middle of the view. Image
  makes a frame named `<Field>` in the middle of the view, 40 mm square,
  or the placeholder picture's shape at most 80 mm long with the picture
  inside it as a clip frame; it keeps the Align and scale in its object
  data (`PrintMerge.Scale`: actual, fit, fill or stretch;
  `PrintMerge.Anchor`: 0 top left to 8 bottom right, row by row). QR
  code makes a 40 mm square named `<Field>`. One undo step each.
- Update Field (when a placeholder is selected: a text holding only
  `<Field>`, or an object named `<Field>`): the selected placeholders
  take the chosen column, and with Image the Align and scale shown.
- Create Merged Document: the current page merged into a new drawing
  (one page per record, named as below); the drawing it came from stays
  as it was. Print Merged Document... does the same, then opens Print.

Clip frames keep the object data of the object they were made from, as
they keep its name.

## Merge

File > Print Merge > Perform Merge copies the current page once per
record, right after it, in the records' order, names the copies
"<page> 1", "<page> 2", ... and fills them. It is two undo steps (the
copies, then the fills).

An object named `<Field>` of a picture field on the page (not in a
group) takes the record's picture inside it as a clip frame, placed by
its Align and scale (placeholders without it fill it from the centre, as
before); a clip frame's old contents, such as the placeholder picture,
give way. An object named `<Field>` of a QR field becomes the code, as
large as its box allows.

## Checks

- `print_merge::tests::counters_and_short_records_are_filled_in`
- `print_merge::tests::fields_and_records_are_added_and_removed`
- `print_merge::tests::csv_round_trips_with_counters_written_out`
- `print_merge::tests::merge_fills_text_qr_and_counters`
- `print_merge::tests::pictures_are_placed_by_scale_and_anchor`
- `ui::print_merge_docker::tests::the_docker_inserts_fields_and_makes_a_merged_document`
- `ui::print_merge_docker::tests::update_field_changes_the_column_and_the_placement`
- `ui::print_merge_docker::tests::the_docker_draws_with_and_without_data`
