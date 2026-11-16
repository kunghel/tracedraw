# Packages

How each package is made and published. Every package is built by
[`build.yml`](../.github/workflows/build.yml) from the same release
files; nothing is built elsewhere.

## AppImage updates (zsync, #2341)

`packaging/linux/build-appimage.sh` writes the update information into
each AppImage:

    gh-releases-zsync|tracedraw|tracedraw|latest|tracedraw-*-linux-amd64.AppImage.zsync

(`-arm64` for the arm64 one) and puts `tracedraw-<version>-linux-amd64.AppImage.zsync`
beside it, attached to the release. AppImageUpdate, Gear Lever and other
AppImage updaters read it, find the newest release (the nightly is a
pre-release and is never taken) and download only the blocks that
changed. When the AppImage plugin writes no `.zsync`, the script makes it
with `zsyncmake` (the `zsync` package).

## Flatpak and Flathub (#2054)

- App id `io.github.tracedraw.TraceDraw`, runtime
  `org.freedesktop.Platform` 24.08. The package repackages the release's
  Linux tar.gz (the program is the same as in the other packages) with
  the desktop entry, icons, file type and the AppStream description in
  `packaging/linux/flatpak/`.
- `packaging/linux/flatpak/manifest.sh` writes the manifest. The
  `flatpak` job builds `tracedraw-<version>-linux-x86_64.flatpak` from the
  x86_64 tar.gz on every run and the release attaches it. A version's
  release also attaches `io.github.tracedraw.TraceDraw.yml`, the manifest
  for Flathub, with the download addresses and SHA-256 of both tar.gz
  files and the tag's commit.
- Publishing on Flathub is a one-time step that needs a GitHub account:
  fork `flathub/flathub`, branch from `new-pr`, add the release's
  `io.github.tracedraw.TraceDraw.yml` and open a pull request against
  `new-pr` (see Flathub's submission guide). Once accepted, Flathub makes
  a repository `flathub/io.github.tracedraw.TraceDraw`; each new version
  is a pull request there with the new release's manifest. The
  `io.github.tracedraw` id can be verified on Flathub by a member of the
  `tracedraw` GitHub organization.

## Chocolatey (#2689)

- `packaging/windows/chocolatey/`: `tracedraw.nuspec` and the install
  and uninstall scripts. The package downloads the release's setup (64
  or 32-bit), checks its SHA-256 and runs it silently for all users
  (`/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- /ALLUSERS`); the
  uninstall runs the setup's own uninstaller silently.
- The `chocolatey` job of a version's release fills in the version and
  the checksums (`pack.ps1`), attaches `tracedraw.<version>.nupkg` to the
  release and, when the repository secret `CHOCOLATEY_API_KEY` is set,
  pushes it to the community repository. The key is free: an account on
  community.chocolatey.org, then Account > API Keys. Each version passes
  Chocolatey's automatic checks and, for the first versions, a moderator,
  before `choco install tracedraw` finds it.
