# Patched dependencies

Copies of published crates with a small change TraceDraw needs before the
upstream release that carries it can be used. They are not workspace
members (`exclude` in the root `Cargo.toml`) and are wired in with
`[patch.crates-io]`. Each is the published crate as downloaded from
crates.io, licence and authors unchanged (see `ATTRIBUTION.md`), with the
change marked `TraceDraw:` in a comment.

## eframe 0.36.2

- Change: `native/glow_integration.rs` creates the OpenGL context with
  three attempts in turn, glutin's default (OpenGL 3.3 core), OpenGL ES,
  then a compatibility profile of any version (`context_attributes_to_try`,
  `create_first_context` and their tests), backported unchanged from
  eframe 0.37.0. 0.36.2 stopped after OpenGL ES, which WGL cannot make on
  older Intel drivers, so a card with OpenGL 2.1 only (Intel HD Graphics
  Ironlake, #616) could not open the window with `--renderer=gl`.
- `create_first_context` sits in `native/first_context.rs` (0.37 has it in
  `glow_integration.rs`) with its tests, so that TraceDraw's tests include
  and run it (`gpu::eframe_context_fallback::tests`); the copy's own tests
  run in no gate. The licence texts, which the published crate leaves
  out, are egui's `LICENSE-MIT` and `LICENSE-APACHE` at tag 0.36.2.
- Drop the copy and the `[patch.crates-io]` entry when the workspace moves
  to egui/eframe 0.37 or later: what holds that back is the egui 0.37
  API migration across the app.
