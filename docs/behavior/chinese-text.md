# Chinese text: Traditional, Simplified and pinyin

Text > Chinese (#2182) turns the selected text objects from Simplified
to Traditional Chinese characters or back in one click, and writes the
pinyin of each Chinese character above it.

## To Traditional Chinese, To Simplified Chinese

- Every selected text object is converted, as one undo step ("To
  Traditional Chinese" or "To Simplified Chinese"). Letters, digits,
  kana and every character the tables do not list stay as they are.
- At each place the longest known phrase is tried first, then the
  character alone. The phrases settle the characters with more than one
  answer: 发 is 發 in 发现 and 髮 in 头发; 后 stays 后 in 皇后.
- The result has as many characters as the text, so each character
  keeps its font, size, colour and other styles, and phrases may run
  across two styles.
- With nothing to change the status bar says so and no undo step is
  made.

## Add Pinyin

- Over each Chinese character of the selected texts its most common
  Mandarin reading is written, with its tone mark (中 zhōng, 文 wén), as
  a small artistic text of its own: centred on the character, 0.4 times
  the character's size, in its font and the text's fill, its baseline
  0.3 of its own size above the top of the line.
- A text lying on a layer is grouped with its pinyin, so they move and
  scale together; the group is selected. A text inside a group gets its
  pinyin beside it in that group.
- Text on a path and vertical text are left alone. One undo step ("Add
  Pinyin") takes it all back.
- Each reading is a text of its own, so a reading with another tone can
  be typed over it with the Text tool.

## Data

`apps/tracedraw/assets/cjk/` holds the tables, made by
`scripts/cjk/make_tables.py` from the OpenCC dictionaries (Apache-2.0)
and the pinyin-data `kMandarin` table (MIT, readings from the Unicode
Unihan database), downloaded at fixed commits and checked by SHA-256.
See ATTRIBUTION.md.

## Checks

- Given 汉字, when To Traditional Chinese runs, then it reads 漢字; 頭髮
  for 头发 and 發現 for 发现; A1 かな stays as it is
  (`chinese::tests::characters_and_phrases_convert_both_ways`).
- Given two spans 头 (20 pt) and bold 发很长, when converted, then they
  read 頭 and 髮很長 with their sizes and bold
  (`chinese::tests::spans_keep_their_characters`).
- Given 中国汉字 selected, when To Traditional and then To Simplified
  run, then it reads 中國漢字 and 中国汉字, and one undo gives 中國漢字
  back (`chinese::tests::to_traditional_is_one_undo_step`).
- Given 中文 ok on a layer, when Add Pinyin runs, then a selected group
  holds the text, zhōng and wén, each over the text and inside its
  width, left to right; one undo gives the text back on its layer
  (`chinese::tests::pinyin_sits_over_each_character_and_groups_with_the_text`).
