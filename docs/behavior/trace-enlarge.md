# Enlarge small pictures first (Trace Bitmap)

A logo saved at a few hundred pixels, often as a JPEG, has edges two or
three pixels soft and compression noise around them; letters a few
pixels tall have no flat pixels at all. Traced at that size the curves
follow pixel steps and noise. The Trace dialog's **Enlarge small pictures
first** (Settings tab, under Image type; off by default, remembered in
the preferences as `tracing.enlarge`) traces such a picture enlarged and
puts the curves back at the picture's own size and place.

## When and how much

- The enlarged picture's longest side is about 1,600 pixels, at most four
  times the picture's (`upscale::factor_for`). A picture whose factor
  would be under 1.5 (longest side over about 1,066 pixels) is traced as
  it is.
- The curves, the gradients' ends and the centreline stroke width are
  scaled back by the enlargement, so the result lands exactly where a
  trace of the picture as it is would.
- Each enlargement is kept for the picture it was made of (the last
  three), so moving a slider traces again without enlarging again.

## The AI model

- **Download AI model (5 MB)** under the box fetches, once, the compact
  "general x4v3" model of the Real-ESRGAN project (BSD-3-Clause) from
  its own release page on GitHub (`realesr-general-x4v3.pth`, v0.2.5.0).
  Of the project's models it keeps small letters best: on a 300 pixel
  logo with letters 7 pixels tall, a B keeps its straight side and its
  two counters, where the anime model rounds them. The file is checked by
  its SHA-256 and kept in TraceDraw's own folder (`models/` beside the
  preferences). Nothing is downloaded until the button is pressed.
- With the model there, a picture of up to 1,200,000 pixels is enlarged
  four times by its network (32 convolutions of 64 features with learnt
  leaky slopes, then 16 sub-pixels per colour added to the picture
  enlarged pixel by pixel; run by TraceDraw's own code on the CPU, in
  tiles on every core, `upscale_net.rs`), then resized to the factor
  above. It takes about three seconds for a 300 x 300 picture on four
  cores; the preview
  shows the quick look meanwhile and "Enlarging with AI... n%" under it.
  The enlargement goes on when a setting changes, so the next trace
  finds it done. The picture never leaves the computer.
- Without the model, in the browser, or for a larger picture, the
  picture is enlarged with Lanczos and sharpened by an unsharp mask that
  grows with the enlargement (Image > Resample's "Detail" method).

## Trace with AI

The **Trace with AI...** button in the dialog's bottom row sends the
picture to the vectorization service of Vectorize with AI
(`behavior/ai-vectorize.md`, Options > AI > Service, the user's own key)
and closes the dialog; the curves come in as that command places them.
Until the service is set up (or with AI features off) the button opens
Options > AI > Service, and Cancel there comes back to the Trace dialog.

## Checks

- `upscale::tests::small_pictures_are_enlarged_up_to_four_times`
- `upscale::tests::an_enlarged_trace_lands_at_the_pictures_own_size`
- `upscale_net::tests::a_model_file_is_read_and_run`: a state dict
  written as a saved one is (zip, pickle, raw tensors) is read and run.
- `upscale_net::tests::a_compact_model_puts_its_sub_pixels_in_place`.
- `upscale_net::tests::a_pickled_state_dict_is_read`,
  `broken_model_files_are_refused`,
  `a_convolution_reads_its_neighbours_and_the_zero_border`.
- `upscale::tests::compare` (ignored): a picture traced as it is and
  enlarged, side by side.
