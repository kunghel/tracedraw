# Contributing to TraceDraw

Thank you for helping. Read `AGENTS.md` first: it holds the engineering
rules (commands, no panics on input, layering, tests, units, language).
This file covers what every contribution must also respect.

## Clean contributions

- Send only work you wrote yourself, or work under a license compatible
  with MIT OR Apache-2.0 that you have the right to submit, with its
  origin stated in the pull request.
- Never send code, data or assets that were decompiled, disassembled or
  dumped from any program, or copied from proprietary software, its
  documentation or its installation files. This includes material
  produced by AI tools from such sources.
- Never port code from other implementations of a file format. Write it
  from public format notes and from files you own, and record what you
  learned, its source and the date in `CLEANROOM.md`.
- Bundle only original or openly licensed assets (icons, fonts, images,
  palettes, templates, test files) and list each one in
  `ATTRIBUTION.md` with its author, source and license.
- TraceDraw stands on its own: do not name other vendors or their
  products in code, comments, docs, commit messages or file names, and do
  not describe a feature as modelled on another program. Give features
  descriptive names of their own.

## Sign-off (DCO)

Every commit must carry a sign-off line:

```text
Signed-off-by: Your Name <you@example.com>
```

`git commit -s` adds it. By signing off you certify the Developer
Certificate of Origin 1.1 (https://developercertificate.org/): that you
wrote the contribution or otherwise have the right to submit it under
the project's licenses, and that you understand the contribution and
your sign-off are public and kept with the project.

Pull requests with unsigned commits, or with material whose origin is
unclear, are not merged.

## Workflow

- Run `cargo fmt --all`, `cargo clippy --workspace --tests` and
  `cargo test --workspace` before pushing; the build must stay free of
  warnings.
- Every change comes with tests; format code gets round-trip and
  malformed-input tests.
- User-visible strings go through the translation files (`docs/i18n.md`).
- A new tool or effect gets a page in `docs/behavior/` and a row in
  `docs/features.md`.

## Review and credit

- Every pull request from outside the project is read in full for
  security before it is merged: network access, running programs, file
  and secret access, new dependencies, build scripts and CI changes,
  obfuscated or binary content. Keep pull requests small and focused so
  this stays quick.
- Commit with your own name and an email that belongs to your GitHub
  account (if a coding assistant makes the commits, set
  `git config user.name` and `user.email` to yours first); GitHub's
  contributor list goes by the commit's author email. A pull request that
  needs a conflict resolved is merged keeping your commits as you wrote
  them.
- Once merged, you are listed in [`CONTRIBUTORS.md`](CONTRIBUTORS.md)
  with what you contributed.

## License

Contributions are licensed under MIT OR Apache-2.0, like the rest of the
project.
