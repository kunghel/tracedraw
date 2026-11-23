#!/bin/sh
# Draw the Help's screenshots (docs/help/img) from the app itself, in
# English, headlessly: every dialog, options page and panel the Help
# shows, on a small demo drawing (src/help_shots.rs), and the scenes
# that show the tools at work on the canvas (src/help_scenes.rs). All
# languages use these images. Run from the repository root;
# HELP_SHOTS_ONLY=name,name draws only the pictures whose names start so.
set -e
out="${1:-docs/help/img}"
mkdir -p "$out"
dir="$(cd "$out" && pwd)"
HELP_SHOTS="$dir" cargo test -p tracedraw --lib help_shots::help_shots -- --ignored --nocapture
HELP_SHOTS="$dir" cargo test -p tracedraw --lib help_scenes::help_scenes -- --ignored --nocapture
rm -f "$out/shots.txt"
# Smaller files when a PNG optimizer is installed.
if command -v oxipng >/dev/null 2>&1; then
  oxipng -q -o 4 --strip safe "$out"/*.png
fi
