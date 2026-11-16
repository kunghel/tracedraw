# Benchmark

Help > Benchmark (also the Benchmark button under Use hardware
acceleration in Options > General) measures how fast this computer is
for TraceDraw and says, in numbers, whether slowness comes from the
computer. Code: `apps/tracedraw/src/benchmark.rs` (the tests and the
scores), `apps/tracedraw/src/ui/benchmark.rs` (the window).

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
  (`ui::benchmark::tests::a_slow_computer_turns_light_zoom_on`). The
  window has the Light zoom check box too.

## With and without hardware acceleration

- Each result is saved in the settings (`benchmarks`), one for a run
  with the window drawn by a graphics card and one for a run with it
  drawn by the processor; a new run replaces the one of the same kind
  (`benchmark::tests::one_result_is_kept_for_each_acceleration_setting`).
- The window lists both overall scores ("not measured yet" for a missing
  one) and says how to compare: change Use hardware acceleration in
  Options > General, restart and run it again. The renderer cannot
  change without a restart.

## Buttons

- Run Benchmark (greyed while it runs), Copy Results (greyed until a run
  ends: the scores, the measures, the adapter, the acceleration and the
  system as text, `benchmark::report`), Options (the General page) and
  Close.
- Each run is written to the log in one line.

## Checks

- Given 0.4 s for the all-core test, its score is 5.0; 0.2 s gives 6.0,
  0.8 s gives 4.0; nothing goes under 1.0 or over 9.9
  (`benchmark::tests::scores_go_up_one_for_each_halving_of_the_time`).
- Given the window open, when the graphics test has run its frames, a
  result with a frame rate above 0 is shown and saved
  (`ui::benchmark::tests::the_window_draws_runs_and_shows_the_scores`).
- The sample drawing has 1501 objects and paints the middle of the page
  (`benchmark::tests::the_sample_drawing_draws`).
- `cargo test --release -p tracedraw -- --ignored benchmark_here
  --nocapture` prints the processor and memory scores of the computer
  it runs on.
