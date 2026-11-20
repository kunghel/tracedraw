# AI photo commands

When AI features are on and your AI service can make pictures (see
Tools > Options > AI), the Image menu and a bitmap's context menu have
four more commands. They send the selected picture, and your words, to
your own AI service when you run them; nothing is sent before. With AI
features off they do not show.

## Turn an object in 3D {#turn}

1. Select a picture of an object. For the object alone, remove its
   background first.
2. Click [[menu.bitmaps > menu.bitmaps.ai_turn]].
3. Set **[[aiphoto.turn_turn]]** (around its upright axis) and
   **[[aiphoto.turn_tilt]]** (up or down). Add words under
   **[[aiphoto.words]]** if you like.
4. Tick **[[aiphoto.turn_keep_background]]** to get the whole picture
   back; leave it off for the object alone on a transparent background.
5. Click [[dialog.ok]]. The new view comes over the picture; the picture
   stays underneath.

The AI service invents the sides that were hidden, so check the result.

## Remove an object {#remove}

1. Select a photo.
2. Click [[menu.bitmaps > menu.bitmaps.ai_remove_object]].
3. Say what to remove, such as "the red car".
4. Click [[dialog.ok]]. The object goes, with its shadow, reflection,
   smoke and other traces, and the background is filled in. Undo brings
   the photo back.

To remove objects on this computer without an AI service, use
[[menu.bitmaps > menu.bitmaps.remove_background_local]] or the photo
editor.

## Make a depth map {#depth}

[[menu.bitmaps > menu.bitmaps.ai_depth_map]] asks for a grey picture of
how far each part of the photo is: white is nearest. It comes over the
picture, and the photo editor's brush and stamp use it to follow the
object's 3D shape.

## Turn a sketch into curves {#sketch}

1. Select a scanned or photographed sketch.
2. Click [[menu.bitmaps > menu.bitmaps.ai_sketch]].
3. Say how it should look, such as "vectorize this lettering cleanly and
   fill it with black".
4. Click [[dialog.ok]]. The AI service redraws the sketch cleanly and
   TraceDraw traces it into curves over the sketch.
