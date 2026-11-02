# Maintaining TraceDraw

How reviewers and maintainers look after issues and pull requests. The
roles themselves (who can do what, and how one moves up) are in
[`CONTRIBUTING.md`](CONTRIBUTING.md#roles); the engineering rules are in
[`AGENTS.md`](AGENTS.md).

## Issues

### Where to start

Work from the issues nobody has read yet, not from the labels:

- **Not read yet:** open issues without `triaged`
  ([search](https://github.com/tracedraw/tracedraw/issues?q=is%3Aissue+is%3Aopen+-label%3Atriaged+sort%3Acreated-asc)),
  oldest first.
- **The work queue:** open issues with `triaged`, by priority:
  [`P0`](https://github.com/tracedraw/tracedraw/issues?q=is%3Aissue+is%3Aopen+label%3AP0),
  then `P1`, `P2`, `P3`.
- **Answers to read:** `needs info` issues with a new comment
  ([search](https://github.com/tracedraw/tracedraw/issues?q=is%3Aissue+is%3Aopen+label%3A%22needs+info%22+sort%3Aupdated-desc)).

### Do not trust the labels an issue arrives with

The forms put `bug` or `enhancement` on an issue by themselves: they say
which form the reporter picked, not what the issue is. A crash filed as a
feature request, a missing feature filed as a bug, or several problems in
one report are common. Read the issue, then set the labels yourself:

| Label | Set by | Meaning |
|---|---|---|
| `bug`, `enhancement` | the form | the reporter's guess; correct it when it is wrong |
| `triaged` | a reviewer or maintainer | read, reproduced or judged, and given a priority |
| `P0` to `P3` | a reviewer or maintainer | the priority (below) |
| `regression` | a reviewer or maintainer | worked in an earlier release |
| `cdr` | a reviewer or maintainer | the `.cdr` reader or writer |
| `needs info` | a reviewer or maintainer | cannot be reproduced without more from the reporter |
| `duplicate` | a reviewer or maintainer | closed in favour of the issue it links to |
| `later` | a reviewer or maintainer | a feature request kept for later (below) |
| `good first issue` | a reviewer or maintainer | small, well understood, a good start for a new contributor |

Only labels set by a reviewer or maintainer are used to plan work.

### Priority

| Priority | What | Examples |
|---|---|---|
| `P0` | the program stops, freezes or loses work; a file that opened before no longer opens; saving writes a file that does not open again | a panic, a hang on opening a file, a `.cdr` saved by TraceDraw that its reader rejects |
| `P1` | a regression in the latest release; a `.cdr` that opens wrong, with the file attached; an export that is wrong; the program does not start on a platform | objects missing from a sample, a PDF that prints wrong |
| `P2` | any other bug with the steps to repeat it | a tool that misbehaves in one case |
| `P3` | small or cosmetic | a misaligned control, a typo in the interface |
| `later` | feature requests | see below |

Bugs come before features, and within one priority the issue with a file
or exact steps comes first: it is the one that can be fixed.

### Reading an issue

1. Check the version: is it the latest release? If not, ask whether it
   still happens there before spending time on it.
2. One problem per issue: ask the reporter to open one issue for each
   further problem, and keep this one to the first.
3. Repeat it: with the steps, the attached file, on the platform given.
4. Search for an older issue about the same thing; close the newer one as
   `duplicate` with a link, and thank the reporter.
5. Set the labels and the priority, add `triaged`, and say in a comment
   what you found ("repeats on 0.33.0 with the attached file: the group
   loses its transparency").
6. If it cannot be repeated without more, add `needs info` and ask for
   exactly what is missing (the file, the steps, the version). An issue
   that stays `needs info` for 30 days without an answer is closed with
   a note that it will be reopened when the details come.

### Attachments

An attached file comes from someone you do not know. Treat it that way:

- **Open it only with TraceDraw or `tracedraw-cli`** (`info`, `inspect`,
  `convert`), which are written never to panic or run anything from a
  file and to stop at the memory limits in `docs/cdr-format.md`. Do not
  open it in other programs, and never run anything that comes in a
  `.zip` (programs, scripts, shortcuts, documents with macros). An
  attachment holding any of these: remove the comment, report it to the
  owner and close the issue.
- **Keep it outside the repository**, in a folder of its own, and delete
  it when the issue is done. Files that belong to someone else are never
  committed (see `CLEANROOM.md`); a test needs a fixture made for
  TraceDraw that shows the same thing.
- **Check that it may be shared.** A file with personal data, client work
  or logos that is clearly not the reporter's: hide the comment, ask for
  a small drawing made just for the report instead.
- **Look before you believe the description.** Open the file, compare it
  with the screenshot of how it should look, and note in the issue what
  is actually different. The description often names the wrong object
  or effect.
- **Record what it teaches.** When a file shows something new about a
  format (a chunk, a field, a version), add a line to `CLEANROOM.md`
  with the date, what was learned and "file sent in issue #N", and the
  details to `docs/cdr-format.md`.

### Feature requests

Feature requests are kept, not closed, but are not worked on while bugs
are waiting:

- Label them `later` (and `triaged`), thank the reporter and say they are
  kept for when the bug list allows; a thumbs-up on the issue is how
  others show they want it too.
- Check `docs/features.md` first: a feature that exists but works wrong is
  a bug, and gets a priority.
- A request with a `.cdr` attached that shows something the reader skips
  or draws wrong is a **reader bug**, not a feature: it gets `cdr` and a
  priority like any other bug.
- Large features (new tools, new formats, new panels) are discussed in the
  issue before anyone starts, so no one writes code that is not merged.

## Pull requests

### Reviewing (reviewers and maintainers)

Read the whole diff. A pull request is approved only when all of these
hold:

- **Security:** nothing new that reaches the network, starts programs,
  reads or writes files outside what the feature needs, touches secrets,
  adds a dependency, changes a build script or a workflow, or carries
  binary or obfuscated content, without a reason in the description that
  you checked. Changes to `.github/workflows/` need the owner's review.
- **Sign-off:** every commit has a `Signed-off-by:` line with the author's
  own name and address. A sign-off by a coding assistant does not count:
  the person certifies the origin, not the tool.
- **Clean room:** no code ported from another implementation; format
  knowledge has its `CLEANROOM.md` line; no other product or vendor named
  (the trademark notice aside).
- **The rules in `AGENTS.md`:** changes go through `Command` and
  `Engine::run`; no `unwrap`, `expect`, `panic!` or raw indexing on data
  from a file or the user outside tests; interface text through the
  translation files; a `docs/behavior/` page and a `docs/features.md` row
  for a new tool or effect; no em dashes.
- **Tests that test the behaviour,** not only that it compiles: a round
  trip for format code, the case from the issue for a bug fix.
- **It works:** for anything visible, run it, or see the screenshots in the
  pull request, before approving.
- **Scope:** one change per pull request, nothing unrelated; ask for the
  rest in another one.

Ask for changes in a review with what to change and why; approve only when
everything above holds. Thank the contributor either way.

### Merging (maintainers)

- `main` is protected: a pull request needs one approval from someone other
  than its author and a green build. Nobody pushes to `main` directly.
- Merge so the contributor stays the author of their commits (merge or
  rebase, not a squash that takes their name off). When you have to
  change a contribution before merging, keep the contributor as the
  author and say what you changed.
- After merging: add the contributor to `CONTRIBUTORS.md` (first
  contribution), add a line to `CHANGELOG.md` under "Unreleased" with the
  issue number, and close the issue with a link to the pull request.
- A merge that breaks something: revert it (the Revert button on the pull
  request), then fix it in a new pull request. Never rewrite `main`.

### Releases and signing

Tags, signed builds, the Microsoft Store and Google Play submissions and
the repository's settings and secrets belong to the organization's owners
(see `docs/code-signing.md`). A maintainer may prepare a release (the
changelog, the version numbers) in a pull request and ask an owner to tag
it.

## Replying

Write in English, plainly and politely, as the rest of the project does.
Thank people for every report, file and pull request: they are giving
their time. When the answer is no (out of scope, a duplicate, a feature
for later), say why in one or two sentences.
