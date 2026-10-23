# Tracing bitmaps

Tracing turns a bitmap into vector shapes that you can edit, recolor and
scale without losing quality: a scanned logo, a sketch, a photo.

## Quick trace {#quick-trace}

Select a bitmap and choose [[menu.bitmaps > menu.bitmaps.quick_trace]]. It uses
the preset chosen on the [[options.bitmap_tracing]] page of the global options
(Clipart at first).

[[menu.bitmaps.repeat_last_trace]] traces the selected bitmap again as the last trace did, without the dialog box, and [[menu.bitmaps.last_trace_settings]] opens the dialog box with the last trace's settings. A bitmap's right-click menu has these commands too, with [[menu.bitmaps.centerline_trace]] and [[menu.bitmaps.outline_trace]].

## The Trace Bitmap dialog box {#trace}

[[menu.bitmaps > menu.bitmaps.outline_trace]] and
[[menu.bitmaps.centerline_trace]] list presets; choose one to open the trace
dialog box.

- **Outline trace** fills areas of color with shapes:
  [[menu.bitmaps.trace_line_art]], [[menu.bitmaps.trace_logo]],
  [[menu.bitmaps.trace_detailed_logo]], [[menu.bitmaps.trace_clipart]],
  [[menu.bitmaps.trace_low_quality]] and [[menu.bitmaps.trace_high_quality]].
- **Centerline trace** follows lines with open curves:
  [[menu.bitmaps.trace_technical]] and [[menu.bitmaps.trace_line_drawing]].

![The Trace Bitmap dialog box](../../img/bitmaps-tracing.png)

The dialog box shows the bitmap **before** and its trace **after**, updated as
you change the settings:

- the preset buttons at the top;
- **Detail** (0 to 100): more detail keeps smaller areas;
- **Smoothing** (0 to 100, 25 at first): smoother curves, fewer nodes;
- **Corner smoothness** (0 to 100): rounder corners;
- **Black and white**, or the number of **Colors** (1 to 256);
- **Remove background** (on at first): leaves out the background color;
- **Delete original image** (off at first);
- **Preview** counts the curves of the full trace.

Click [[dialog.ok]] to trace. The shapes are placed over the bitmap, darkest
first.
