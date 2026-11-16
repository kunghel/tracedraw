# Object and group state flags (#2850)

Original synthetic CDR27 parser fixtures made for TraceDraw, licensed under
MIT OR Apache-2.0. Run `python3 generate.py` to reproduce `samples.zip`
using Python's standard library only. The archive has deterministic timestamps.

No private source drawing, geometry, text, fonts, metadata or pictures are
included. The generator uses TraceDraw's own RIFF/ZIP and argument layouts.
The group visibility bit was observed in contributor-owned version 27 files
and compared with their embedded previews and the reporter's screenshot.
These are parser fixtures, not files saved or validated by another editor.

| File | Expected import |
| --- | --- |
| `hidden-two.cdr` | Hidden group of two rectangles, both children retained |
| `hidden-one.cdr` | Hidden group of one rectangle, parent retained |
| `nested.cdr` | Visible group with a visible rectangle and a hidden subgroup |
| `visible.cdr` | Visible group of two rectangles |
| `short-flags.cdr` | Truncated flags ignored safely |
| `wrong-record-type.cdr` | Non-group record type ignored |
| `unrelated-flag.cdr` | Unrelated byte-1 bit does not hide the group |

All rectangles are black. The hidden-only files should render blank.
Native `.tdraw` save/reopen must retain the hidden parent and its children.
CDR export of hidden containers is outside this import regression.

Additional samples cover individual objects: `hidden-object.cdr` carries
`00 03 00 08`, while `visible-object.cdr` carries `00 02 00 08`, as seen
on two text objects in the reporter's updated version 27 sample. The
hidden bit is the same as a group's, on a different record type.
`locked-object.cdr` and `hidden-locked-object.cdr` check the already known
byte-0 lock bit 0x10 independently of visibility. `hidden-object-argument.cdr`
retains support for argument 16016. Short and mismatched object records
must not hide an otherwise visible object. All states survive native
save/reopen. Parent layer exclusion applies to the objects on that layer;
no independent per-object print-exclusion field was observed in this sample.
