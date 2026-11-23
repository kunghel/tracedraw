# Find and Replace

Edit > Find and Replace (Ctrl+F) opens the Find and Replace panel. It has
three modes: Text, Objects and Properties.

- Text: finds text objects holding a string (Match case, Whole word) and
  replaces it (Replace, Replace All). Current page. A text's runs (its
  parts in their own looks) are searched as one text: a word half of it
  bold is found and replaced, the replacement taking the look of the run
  its match starts in. Texts inside groups and PowerClip frames are found
  (Find selects the object they are in) and replaced. Without Match case
  letters match one for one ignoring case; with Whole word a match has
  no letter or digit right before or after it. Before, a word in two
  looks was found but not replaced, grouped text was neither, and a
  Turkish "İ" before the match (longer in lower case) cut the text in the
  wrong place or closed TraceDraw
  (`export::tests::replace_finds_words_across_runs_in_groups_and_after_a_dotted_capital_i`).
  Special characters (#3679), in Find and in Replace: `^p` a paragraph
  break (Enter), `^l` a line break (Shift+Enter), `^t` a tab, `^s` a
  non-breaking space, `^^` a caret; in Find only, `^w` is two or more
  spaces in a row (the whole run), and in Replace `^w` puts one space. A
  caret before any other letter stays as typed. So Find `^p` Replace
  with a space joins paragraphs, and Find `^w` Replace `^w` leaves one
  space wherever there were several
  (`export::tests::find_and_replace_read_the_special_characters`).
  The Find and Replace with fields have a right-click menu (#3678):
  Cut, Copy, Paste (as one line) and Select All, and Special Character,
  which puts the code of each of these at the cursor, over the letters
  picked
  (`ui::dockers2::find_field_tests::the_find_fields_menu_puts_text_at_the_cursor`).
- Objects: finds objects whose name or kind contains the string. Current
  page.
- Properties: finds objects by their look and type and replaces fill
  colour, outline colour, outline width or font in them (#1071). The rest
  of this page is about this mode.

## Find objects by properties

Look in: This page (default) or Whole drawing (every page, then the
master layers).

Criteria, each used only while ticked; an object is found when it meets
every ticked one (`tracedraw_core::find::ObjectQuery`):

| Criterion | Default | Meets it |
|---|---|---|
| Type | off, Rectangle | the object is of this kind: Rectangle, Ellipse, Polygon or star, Curve, Artistic text, Paragraph text, Group, Bitmap, Clip frame, Table, Symbol |
| Fill | on, 100 % K | None: no fill. A colour: a uniform fill of it, or a gradient stop or mesh node of it |
| Outline | off | None: no outline. A colour: an outline of that colour |
| Outline width | off, 0.2 mm | an outline of that width, within 0.0005 mm; shown in the outline unit (Options) |
| Font | off, empty | a text with a span in that family, compared without regard to case |

Colours compare as the same when they are in the same model and each
value is within 0.5 % (0.005 of the 0..1 range), or, in different models,
when they show as the same 8-bit RGB. A spot colour matches the same ink
at the same tint; Registration matches only Registration
(`find::same_color`).

Groups and clip frames: the look criteria are tested on the objects
inside them (at any depth); the group is found when one of them meets
them. With only a Type ticked, the top-level object's own type counts.
Only top-level objects are listed, as they are what can be selected.

From Selection fills the criteria from the first selected object: its
fill (a uniform colour or None; other fills leave Fill unticked), its
outline colour (or None), its outline width (unticked), its first font
(ticked for text) and its type (unticked). Reset clears everything back
to the defaults, keeping Look in.

Buttons:

- Previous and Next: search again, then select the object before or after
  the one shown last (wrapping round), alone. With Whole drawing the page
  of that object comes into view first. After Find All, Next starts at
  the first object found and Previous at the last.
- Find All: selects every object found on the current page (and on the
  master layers); the status bar says how many were found in all and how
  many on this page.

The panel shows the number found.

## Replace

Replace with, each used only while ticked:

| Replacement | Effect on each object found (the members that meet the look criteria) |
|---|---|
| Fill | with a Fill colour ticked: that colour is replaced by the new one in uniform fills, gradient stops and mesh nodes; otherwise the fill becomes a uniform fill of the new colour |
| Outline | with an Outline colour ticked: outlines of that colour take the new colour; otherwise every outline does. Objects without an outline get none |
| Outline width | every existing outline takes the width |
| Font | with a Font ticked: spans in that font take the new one; otherwise every span does. Below it, All characters (the default), Numbers only or Letters only: with Numbers only just the numbers of those spans (the digits of every script) take the new font, with Letters only just the letters (of every script, vowel signs and other marks staying with their letter); spans are cut where the kind of character changes (#2585) |
| Digits to 0-9 | in the spans the font criterion picks (every span without it): the decimal digits of other scripts (Arabic-Indic, Persian, Devanagari, Bengali, Thai, full-width and the others) are written as 0 to 9, the rest of the text staying as it is. It works alone too (#2585) |

Replace All searches again and applies every change as one batch of
`SetFill`, `SetStroke` and `SetShapeKind` commands: one undo step
labelled Replace (`find::replace_commands`). Locked objects and objects
on locked layers are found but left unchanged. The status bar says in how
many objects something changed.

## Checks

- Given red, blue and empty rectangles, when searching by fill red, then
  only the red one is found; by outline width 0.5 mm, only that one; by
  no fill and no outline, the empty one; by type Rectangle, all three
  (`find::tests::finds_by_fill_outline_width_and_type`).
- Given a group of a red and a blue rectangle on page 1 and a red ellipse
  on page 2, then This page finds the group and Whole drawing finds both;
  replacing the fill with green changes the red rectangle and the ellipse
  only, and one undo restores both
  (`find::tests::finds_across_pages_and_inside_groups`).
- Gradient stops, outline colours and fonts are replaced, other fonts
  kept (`find::tests::replaces_gradient_stops_outlines_and_fonts`).
- Given "Room 12, floor 3" in Serif, when the font Mono goes to the
  numbers only, then 12 and 3 are in Mono and the rest stays in Serif;
  with letters only the words take Mono; Arabic-Indic, Persian,
  Devanagari and full-width digits become 0 to 9
  (`find::tests::a_font_goes_to_the_numbers_or_the_letters_only`,
  `find::tests::replace_all_changes_only_the_numbers`).
- A locked object is found but not changed
  (`find::tests::locked_objects_are_found_but_not_changed`).
- In the panel, From Selection, Find All, Next, Previous and Replace All
  work through clicks, across two pages
  (`ui::dockers2::find_properties_tests`).
