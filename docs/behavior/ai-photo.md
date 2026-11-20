# AI photo commands

Four commands that work on the selected bitmap through the one AI
service the user sets up with their own key (Tools > Options > AI >
Service), when that service can make pictures (OpenAI, Google Gemini, or
a custom address with an image model; see `ai-options.md`). TraceDraw has
no AI of its own, and nothing is sent until the user runs a command. The
work is in `apps/tracedraw/src/ai_photo.rs`; the requests, retries and
error messages are those of `ai_image.rs` (see `ai-generate.md`).

## Where, and when they show

Image menu (after Vectorize with AI) and a bitmap's context menu:

- Turn Object in 3D with AI...
- Remove Object with AI...
- Depth Map with AI
- Sketch to Curves with AI...

They show only while AI features are on (Tools > Options > AI >
General). With AI off nothing of them shows and nothing else changes.
While AI is on but the service cannot make pictures yet, they are greyed
and their hover tip says what is missing, as for the other AI picture
commands. They need one bitmap selected and no AI picture request
waiting.

Each dialog ends with a note: the picture and the words go to the AI
service set up in Tools > Options > AI, which bills for them.

## Turn Object in 3D

The dialog has Turn (-180 to 180 degrees around the object's upright
axis; positive turns it to its right), Tilt (-80 to 80 degrees; positive
shows more of the top), Also (optional words) and Keep the background
(off by default). OK sends the bitmap as an edit with words that ask for
the same object from the new viewpoint, keeping its identity, colours,
materials and lighting and inventing the hidden sides. With both Turn
and Tilt under one degree nothing is sent and the status bar asks for
one.

- Keep the background off: the object alone on a transparent background.
  The OpenAI Images API is asked for a transparent PNG; Gemini, which
  gives no transparency, is asked for flat green, keyed out like Remove
  Background's.
- Keep the background on: the whole picture comes back with the object
  turned in it.

The new view goes over the bitmap (same place, height and transform, its
own proportions), named "Turned object", selected, in one undo step. The
bitmap itself is kept underneath. The last settings are kept for the
session.

## Remove Object

The dialog takes what to remove in words ("the red car"). OK sends the
bitmap as an edit asking to remove that object with every trace it
leaves (shadow, reflection, smoke, dust, glow) and fill the area with a
background that matches its surroundings, changing nothing else. With
no words nothing is sent. The answer replaces the bitmap's picture in
place, resized back to the bitmap's own pixel size, as one undo step.

`App::ai_remove_object(shape, words, mask)` also takes painted marks
(white where painted, at the bitmap's size) for the photo editor: the
OpenAI Images API gets them as its `mask` (transparent where to change);
Gemini gets the picture with the marked area tinted red and words that
say to remove what is painted over in red.

## Depth Map

Asks for a grey depth map of the bitmap (same framing, white nearest,
black farthest, smooth shading that follows each surface). The answer's
brightness, at the bitmap's pixel size, is:

- placed over the bitmap as a grey picture named "Depth map" (one undo
  step), and
- kept with the bitmap's id (`ImageSession::depth`) until the photo
  editor takes it with `App::ai_take_depth(shape)`, given once. The
  photo editor's brush and stamp that follow an object's 3D shape use it
  in place of the depth they estimate from the photo's light.

## Sketch to Curves

The dialog takes a request in words (empty: "Ink the final lines and
fill the shapes in solid black."). OK sends the bitmap as an edit asking
to redraw the sketch as clean, flat artwork that follows its shapes,
letters and composition exactly, on white. The answer goes over the
sketch and is traced into curves with the Clipart preset, as Create
vectors does, named "AI-generated vector"; the sketch stays underneath.

## Checks

`ai_photo::tests`:

- the commands are hidden while AI is off, greyed while no service makes
  pictures, and enabled with a bitmap selected
  (`the_commands_show_only_while_ai_is_on`);
- the turn's words follow the turn, the tilt and the background, and a
  turn of nothing sends nothing (`a_turn_asks_for_the_new_view`);
- a turned view comes over the bitmap, keyed out on Gemini
  (`the_turned_view_goes_over_the_picture`);
- a depth map comes at the bitmap's size, over it, and is taken once
  (`the_depth_map_is_kept_for_the_photo_editor`);
- Remove Object replaces the picture in place at its own size, and marks
  go as an OpenAI mask or tinted red for Gemini
  (`remove_object_changes_the_picture_in_place`);
- Sketch to Curves traces the redrawn sketch into curves over it
  (`a_sketch_becomes_curves`).
