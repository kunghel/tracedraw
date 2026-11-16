#!/usr/bin/env bash
# Write TraceDraw's Flatpak manifest (io.github.tracedraw.TraceDraw.yml) to
# stdout. It repackages the release's Linux tar.gz, with the desktop entry,
# icons, file type and AppStream description of this folder.
#
#   manifest.sh local <version>
#     For CI, written into this folder: the tar.gz is
#     tracedraw-linux-x86_64.tar.gz beside it, the other files are taken
#     from this checkout.
#   manifest.sh flathub <version> <commit> <x86_64 tar.gz> <arm64 tar.gz>
#     For Flathub: the tar.gz files and the repository at tag v<version>
#     (commit <commit>) are downloaded from GitHub, checked by SHA-256.
set -euo pipefail
MODE="$1"; VERSION="$2"
DATE="$(date -u +%Y-%m-%d)"
REPO=https://github.com/tracedraw/tracedraw

cat <<YML
app-id: io.github.tracedraw.TraceDraw
runtime: org.freedesktop.Platform
runtime-version: '24.08'
sdk: org.freedesktop.Sdk
command: tracedraw
rename-desktop-file: tracedraw.desktop
rename-icon: tracedraw
rename-mime-file: tracedraw.xml
finish-args:
  - --share=ipc
  - --share=network
  - --socket=wayland
  - --socket=fallback-x11
  - --device=dri
  # Drawings are opened and saved anywhere in the home folder, and on
  # mounted drives, as with the other packages.
  - --filesystem=home
  - --filesystem=/media
  - --filesystem=/run/media
modules:
  - name: tracedraw
    buildsystem: simple
    build-commands:
      - install -Dm755 tracedraw /app/bin/tracedraw
      - install -Dm755 tracedraw-cli /app/bin/tracedraw-cli
      - install -Dm644 src/packaging/linux/tracedraw.desktop /app/share/applications/tracedraw.desktop
      - install -Dm644 src/packaging/linux/tracedraw-mime.xml /app/share/mime/packages/tracedraw.xml
      - for s in 16 32 64 128 256 512; do install -Dm644 src/assets/icon/tracedraw-\$s.png /app/share/icons/hicolor/\${s}x\${s}/apps/tracedraw.png; done
      - sed -e 's/@VERSION@/${VERSION}/' -e 's/@DATE@/${DATE}/' src/packaging/linux/flatpak/io.github.tracedraw.TraceDraw.metainfo.xml > io.github.tracedraw.TraceDraw.metainfo.xml
      - install -Dm644 io.github.tracedraw.TraceDraw.metainfo.xml /app/share/metainfo/io.github.tracedraw.TraceDraw.metainfo.xml
    sources:
YML

case "$MODE" in
  local)
    cat <<YML
      - type: archive
        path: tracedraw-linux-x86_64.tar.gz
        strip-components: 0
      - type: file
        path: ../tracedraw.desktop
        dest: src/packaging/linux
      - type: file
        path: ../tracedraw-mime.xml
        dest: src/packaging/linux
      - type: file
        path: io.github.tracedraw.TraceDraw.metainfo.xml
        dest: src/packaging/linux/flatpak
YML
    for s in 16 32 64 128 256 512; do
      cat <<YML
      - type: file
        path: ../../../assets/icon/tracedraw-$s.png
        dest: src/assets/icon
YML
    done
    cat <<YML
YML
    ;;
  flathub)
    COMMIT="$3"; X64="$4"; ARM="$5"
    cat <<YML
      - type: archive
        only-arches: [x86_64]
        url: ${REPO}/releases/download/v${VERSION}/tracedraw-${VERSION}-linux-x86_64.tar.gz
        sha256: $(sha256sum "$X64" | cut -d' ' -f1)
        strip-components: 0
      - type: archive
        only-arches: [aarch64]
        url: ${REPO}/releases/download/v${VERSION}/tracedraw-${VERSION}-linux-arm64.tar.gz
        sha256: $(sha256sum "$ARM" | cut -d' ' -f1)
        strip-components: 0
      - type: git
        url: ${REPO}.git
        tag: v${VERSION}
        commit: ${COMMIT}
        dest: src
YML
    ;;
  *) echo "mode: local or flathub" >&2; exit 2 ;;
esac
