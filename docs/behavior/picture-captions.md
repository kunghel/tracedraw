# Picture captions (#2180)

Image > Picture Captions writes each picture's name
beside it as artistic text, one undo step for all of them
(`captions.rs`).

- **Which pictures**: the selected objects that are pictures (a bitmap, or
  a clip frame holding one bitmap alone) or that have a name; with nothing
  selected, every picture on the page's shown, unlocked layers. A group
  of a picture and its caption counts as that picture.
- **The name**: the object's name (a picture imported from a file is
  named after the file, without its folder and extension), else the file
  name a linked picture points at, else the name of a frame's picture.
  A picture without one gets no caption; the status bar says how many.
- **The text**: prefix + name + suffix, in the font chosen (empty: the
  text tool's font) and size, black, no outline.
- **The place**: Below and Above centre the caption's box on the picture's
  box (its visual bounds, a drop shadow included) along that side; Left
  and Right align its lines towards the picture (right and left aligned)
  and centre it half way up. The gap is between the two boxes, in mm.
- **Again**: each caption keeps its picture's id (shape data
  `caption.of`), so running the command again on the same pictures
  replaces the captions they have, outside groups or inside a group of the
  picture and its caption, instead of adding more.
- **Group** (on by default): each picture and its caption become one
  group; off, the caption goes just in front of its picture.
- The dialog's settings are kept in the options (`picture_captions`).
- The captions, or the groups, end up selected.

Tests: `captions::tests`.
