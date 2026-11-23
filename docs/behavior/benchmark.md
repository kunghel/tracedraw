# Benchmark

Help > Benchmark (also the Benchmark button under Use hardware
acceleration in Options > General) measures how fast this computer is
for TraceDraw and says, in numbers, whether slowness comes from the
computer. Code: `apps/tracedraw/src/benchmark.rs` (the tests and the
scores), `apps/tracedraw/src/ui/benchmark.rs` (the window). Everything
happens in that one window: the two tests, the results, the zoom
suggestion and the test with and without the graphics card.

## Quick and advanced test

- **Quick Test** runs the four parts below: some seconds.
- **Advanced Test** runs them, then real tasks on two drawings like the
  sample one with 10 000 and 50 000 objects (`benchmark::ADVANCED_SIZES`,
  `benchmark::time_tasks`): draw the whole page, draw it zoomed in 400 %,
  move every object (with the copy Undo keeps), save and open it as
  `.tdraw` in memory, export it as PDF. When the graphics card can blur
  shadows in this start (`gpu_fx::available`), sixty rectangles with
  shadows are drawn with the card blurring them and without it
  (`benchmark::time_shadows`, `gpu_fx::force`). A grey line under the
  buttons says the advanced test takes from one to several minutes and
  to leave the computer alone. The browser build has no Advanced Test
  (no thread to run it on).
- While a test runs, the window shows a spinner, what is being measured
  and a progress bar; both buttons are greyed. Close stays usable and
  stops the run: the measuring thread starts no further test and lets
  its drawings go, and the graphics test's pictures are dropped, so a
  test too slow to finish (the window drawn on the processor) can be
  left and the processor and graphics go back to idle (#3665,
  `ui::benchmark::tests::closing_the_window_stops_a_run`,
  `benchmark::tests::a_stopped_run_times_no_more_tasks`).
- Each task is listed with its drawing's size, a bar and its time in
  seconds. The bar is green up to a quarter of a second, amber up to a
  second and red after (files: 1 s and 4 s), and full at the slow limit
  (`benchmark::rating`).
- Above the scores, in large letters: "Comfortable up to about N objects
  in a drawing": the objects whose redraw of the whole view takes a
  quarter of a second, from the largest drawing timed (the sample
  drawing for the quick test), rounded down to 500, 1 000, 2 000, 5 000,
  10 000... (`Measured::comfortable_objects`); green, red on a slow
  computer.

## The tests

| Part | What is measured | Score 5.0 at |
|---|---|---|
| Processor (all cores) | A sample A4 drawing (1500 closed curves of six segments, a third with a linear gradient, a fifth 60 % opaque, one in fifty with a drop shadow, all outlined 0.3 mm, on a filled rectangle) drawn into a 1920 x 1080 picture with `render_page`, the canvas's own renderer on every core; best of 3 after one untimed run | 0.40 s |
| Processor (one core) | The same with `render_page_on_one_thread` (how previews and the backdrop are drawn); best of 2 | 1.20 s |
| Memory | A 64 MB block copied; best of 5 | 4 GB/s |
| Graphics | 60 frames of the window (after 5 untimed), each handing it four new 1920 x 1080 pictures and showing them small: what the window does at every step of a zoom or pan | 15 frames a second |

The processor and memory tests run on a thread of their own (in the
browser, in one frame); then the graphics test runs over the window's
frames. The window shows a spinner and what is being measured.

## Scores

- The references are four cores at about 2 GHz: a 4-core 2.1 GHz
  server processor scored 5.1 (all cores, 0.37 s), 5.1 (one core,
  1.06 s) and 5.8 (memory, 7 GB/s).
- Each score is `5.0 + log2(reference / measured)` for a time
  (`log2(measured / reference)` for a rate), cut to one decimal and kept
  between 1.0 and 9.9: one more for each halving of the time.
- A screen at 60 frames a second scores 7.0 in Graphics; frames never
  come faster than the screen refreshes.
- The overall score is the lowest part's, since the slowest part sets
  the pace (`benchmark::tests::the_overall_score_is_the_lowest_part`).

## The warning and Light zoom

- Under 4.0 overall the window says, in red, that large drawings will be
  slow on this computer (zoom, panning, dragging, effects), that this
  comes from the computer and not from a fault in TraceDraw, and which
  part is the slowest. When the window is drawn by a software renderer
  and Graphics is under 4.0, it adds how to turn on hardware
  acceleration.
- Under 4.0, Light zoom (Options > Tools > Zoom/Pan, see
  `zoom-and-pan.md`) is turned on and the window says so; a faster
  result leaves the option as it is
  (`ui::benchmark::tests::a_slow_computer_turns_light_zoom_on`).
- A start made by Test Without / With the Graphics Card draws with
  another renderer than a normal start uses, only to compare: a low
  score there leaves Light zoom as it is, and the window says so. Light
  zoom is turned on only by a run with the usual renderer (#3664,
  `ui::benchmark::tests::a_comparison_run_leaves_light_zoom_alone`).

## Zoom

The window lists three zoom styles, the one in use picked
(`benchmark::ZoomStyle`); a click uses one at once:

- **Smooth (TraceDraw)**: Smooth zoom on, Light zoom off.
- **Instant**: both off: a wheel notch zooms at once by the
  fixed step and is drawn sharp at once, the point under the pointer
  kept (Wheel zoom centers off).
- **Light (slow computers)**: Light zoom on.

Under each style a grey line says what it does (the same text as its
tip), and under the three a line says it can be changed later in
Options > Tools > Zoom/Pan.

Once a test has run, one is marked Recommended, with why: Light on a
slow computer (under 4.0), else Instant, which draws less than the
glide
(`Measured::suggested_zoom`).

## With and without hardware acceleration

- Each result is saved in the settings (`benchmarks`), one for a run
  with the window drawn by a graphics card and one for a run with it
  drawn by the processor; a new run replaces the one of the same kind
  (`benchmark::tests::one_result_is_kept_for_each_acceleration_setting`).
- The window lists both overall scores with their graphics score ("not
  measured yet" for a missing one).
- The renderer cannot change without a restart. When a graphics card
  is found (`gpu::card_found`), a button tests the other way: **Test
  Without the Graphics Card** when the window is drawn by the card,
  **Test With the Graphics Card (name)** when it is drawn by the
  processor. It asks TraceDraw to close (the usual questions about
  unsaved drawings; Cancel cancels the restart too), starts it again
  with `--renderer=software` or `--renderer=hardware`
  (`gpu::restart_after_exit`), and the new start opens this window and
  runs the quick test (`settings.benchmark_resume`,
  `ui::benchmark::resume`). Options are not changed: the next normal
  start draws the way Use hardware acceleration says. Without a card the
  window says it can only be measured with the processor.

## Buttons

- Quick Test and Advanced Test at the top (greyed while a test runs);
  at the bottom Copy Results (greyed until a run ends: the scores, the
  measures, the adapter, the acceleration, the system, and the
  comfortable objects and the tasks, as text, `benchmark::report`),
  Options (closes this window and opens Options at the General page,
  #3351) and Close (usable while a test runs, and stops it, #3665).
- Each run is written to the log in one line.

## Checks

- Given 0.4 s for the all-core test, its score is 5.0; 0.2 s gives 6.0,
  0.8 s gives 4.0; nothing goes under 1.0 or over 9.9
  (`benchmark::tests::scores_go_up_one_for_each_halving_of_the_time`).
- Given the window open, when the graphics test has run its frames, a
  result with a frame rate above 0 is shown and saved; the advanced
  test's steps show while its thread runs, and its tasks are kept with
  the result: 50 000 objects drawn in 2.5 s give "about 5 000"
  (`ui::benchmark::tests::the_window_draws_runs_and_shows_the_scores`).
- A zoom style sets the options it stands for; a slow computer is told
  Light, the others Instant
  (`ui::benchmark::tests::a_zoom_style_is_used_from_the_window`).
- A start for the benchmark opens it and runs the test once
  (`ui::benchmark::tests::a_start_for_the_benchmark_runs_it`).
- Options opens the Options window
  (`ui::dialogs::tests::benchmark_options_opens_the_options_window`).
- The sample drawing has 1501 objects and paints the middle of the page
  (`benchmark::tests::the_sample_drawing_draws`).
- `cargo test --release -p tracedraw -- --ignored benchmark_here
  --nocapture` prints the processor and memory scores of the computer
  it runs on.
