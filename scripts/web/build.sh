#!/bin/sh
# Build the browser version into ./site (see docs/behavior/web.md).
set -e
cd "$(dirname "$0")/../.."
cargo build --target wasm32-unknown-unknown --profile web -p tracedraw
rm -rf site && mkdir -p site
wasm-bindgen --target web --no-typescript --out-dir site \
    target/wasm32-unknown-unknown/web/tracedraw.wasm
# Shrink the module for download with binaryen's wasm-opt (version 133,
# as in CI): optimise for size and drop the name, debug and producer
# sections, which the page does not use.
wasm-opt -Oz --strip-debug --strip-producers \
    -o site/tracedraw_bg.wasm site/tracedraw_bg.wasm
cp web/FONTS.md site/
# The page with the module's size (the download shows its progress) and
# the version.
size=$(wc -c < site/tracedraw_bg.wasm | tr -d ' ')
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
sed -e "s/data-wasm-size=\"0\"/data-wasm-size=\"$size\"/" -e "s/@VERSION@/$version/" \
    web/index.html > site/index.html
cp assets/icon/tracedraw-256.png site/icon-256.png
# The font folder (web/fonts.txt) and the font catalog when asked for:
# TRACEDRAW_FONT_FOLDER=1,
# with TRACEDRAW_GOOGLE_FONTS pointing at a google/fonts checkout to copy
# from instead of downloading.
if [ -n "$TRACEDRAW_FONT_FOLDER" ]; then
    if [ -n "$TRACEDRAW_GOOGLE_FONTS" ]; then
        python3 web/font_folder.py web/fonts.txt site/fonts --google-fonts "$TRACEDRAW_GOOGLE_FONTS"
        python3 web/font_catalog.py web/fonts.txt site/fonts --google-fonts "$TRACEDRAW_GOOGLE_FONTS"
    else
        python3 web/font_folder.py web/fonts.txt site/fonts
        python3 web/font_catalog.py web/fonts.txt site/fonts
    fi
fi
echo "Built ./site; serve it with: python3 -m http.server -d site 8080"
