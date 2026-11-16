# Spreadsheets as tables

File > Open and File > Import read spreadsheets into live tables whose
cells stay editable text, to lay out and print the data (#1564): an Excel
workbook's first sheet (`.xlsx`, read as Print Merge reads it: shared
strings, inline strings and numbers as stored), or a CSV file (comma,
semicolon or tab, whichever the first line has most of; any encoding
the text import reads) or TSV file. Both dialogs list them under
Spreadsheets, and dropping one on the window opens or imports it as the
other formats do. Trailing empty rows are left out; an empty sheet is
an error on the status line.

Each sheet row is a table row and each cell a table cell, in Arial: the
first row bold, cells that read as numbers (digits with signs,
separators, a decimal point, currency or percent) aligned right, the
rest left. Columns start 8 mm wide and grow to their widest text; rows
are 1.5 lines of the text high; hairline black borders, 0.8 mm cell
margins.

## Open

A new drawing named after the file, not tied to it (Save asks where, so
the sheet is never overwritten). A4 pages, 10 mm margins: portrait when
the table fits across, else landscape, and when it is still wider the
text shrinks (whole points, not under 6 pt) so it fits. The rows run
over as many pages as they need, one table per page at the top left
margin, the first row repeated at the top of each.

## Import

The whole sheet as one table, 10 pt, at the top left margin of the
page (or centred in the view, as Options > File Formats sets for every
import), selected, one undo step.

## Checks

`spreadsheet_import::tests`: CSV and TSV rows with their first row,
empty and broken files refused; a bold first row and numbers to the
right; a 150-row sheet over several portrait pages with the first row on
each and every row once; a 30-column sheet on landscape pages with
smaller text; opening a CSV leaves the drawing untied to it, and
importing it adds one selected 3 x 2 table.
