# Working rules

## Docs and code state the current design only

Version control holds the history; the docs and code do not. When a decision
changes, rewrite the text in place to say what is true now. No "previously",
"was X, now Y", dated decision notes, or sections kept for reference. A
rejected alternative may stay only as rationale ("not X, because ...").

## The shared Rust substrate

The CLI's entry and exit handling is rust-support's `cli-support` (pinned by
full commit in `Cargo.toml`), not written
here: `Failure` (a diagnostic with the `Exit` it ends the command with; an
error says its own with `ExitClass`), `Failure::say` (the failure on stderr,
its `fix` where the catalog has `<code>-fix`), `say_notices` (warnings),
`usage_exit` (the parser's error as an exit) and the exit-code convention
(`Exit`: 0 for every success, 16-31 under `--detailed-exitcode`; 32 failed, 48 refused, 49 fatal findings). Formats and the message
catalog are angzarr core's `angzarr-presentation`, on the same core as
`angzarr-diagnostic`, so the lock holds one. The CLI reads no environment
and no project directory of its own (`std::env` is touched only by the
parser), so `process-env` and `project-dir` have nothing to replace here.
Do not write a printer, an exit code or a catalog again: change the shared
crate.

## Hexagonal

The core is pure: no I/O, no clap, no tonic, no file system. Use cases sit
behind ports (traits); adapters implement them. Results are data: a command
returns one value and `angzarr-fmt` decides how it is written, so the output
format is never a branch in business logic. `angzarr-fmt` knows nothing
about angzarr or about clap; angzarr-cli's `output` module is the only place
joining them.

## Errors are localizable: a code, args, and an en-US message

Every error and diagnostic implements `angzarr_diagnostic::Diagnostic` (angzarr core's),
and its `Display` is code and args (`angzarr_diagnostic::display`), never English
(decision 14.37). English is rendered only where a person reads it: the cli through
angzarr-presentation's `Catalog` through cli-support's `Failure::say` (`output::finish`), and a build script through
`angzarr_diagnostic::catalog::render` in angzarr-build (its repository).

- Codes are stable and never reused: kebab-case, starting with the crate's
  name (`angzarr-fmt-write`). A code whose meaning changes gets a new
  code.
- No code or catalog spells its crate's name: a code is `code!("suffix")`
  (angzarr-diagnostic's macro, prefixing `CARGO_PKG_NAME`), and another
  crate's code is taken from its typed error (`HostError::Spawn { .. }.code()`)
  or matched by variant, never written out. Tests compare the typed variant.
- Args carry data, never pre-formatted English.
- A new code needs an en-US message in `crates/<crate>/locales/en-US/messages.ftl`,
  its id the code's suffix, using exactly its args, and a sample in the
  crate's `diagnostic_samples()`; the catalog test (`angzarr_diagnostic::catalog::check`)
  fails otherwise. The crate's build script (`angzarr_diagnostic::build::catalogs`)
  writes the catalog under the crate's name, exported as
  `EN_US = (CARGO_PKG_NAME, catalog)`.
- A failure that the user can act on names its remedy, itself a coded
  message whose id is the failure's code with `-fix` (`domain-required-fix`):
  cli-support looks it up by that id. (`catalogs/build-fixes.ftl` aliases
  angzarr-build's remedy, which is not named so; it goes when angzarr-build
  renames it.)

## Unicode in source is visible

The workspace denies `invisible_characters`, `unicode_not_nfc` and
`non_ascii_literal`; every crate opts in with `[lints] workspace = true`.
Write a non-ASCII character in a literal as an escape.

## The generator is angzarr's

`angzarr-codegen` and `angzarr-build` are crates of the angzarr workspace,
`github.com/angzarr-io/angzarr` (the old angzarr-build repository is merged
into it), pinned in `Cargo.toml` by full
commit, never copied here. Their tests, the spec's codegen tier among them,
run there. This repository tests the binary: its tests run `angzarr` over
the spec's protos (the `angzarr-project` submodule at the commit the
generator is built against) and the fixtures and golden files under
`crates/angzarr-cli/tests`, whose goldens are the generator's output for
them. Moving the pin means refreshing the goldens from the generator's, and
nothing the generator documents is restated here. The CLI names no default
component name: a component's name is the protos' `component_name`.

The manifests `scaffold app` writes name angzarr-build as a git dependency
on that repository at the revision the workspace manifest pins
(`ANGZARR_BUILD_REV`, read from `Cargo.toml` by the crate's build script),
or `--angzarr-build-path`.

## Use `just`

`just test` and `just lint` are the checks; both must pass before a commit.
This repository is the Rust `angzarr` CLI; the Go CLI is preserved on the
branch `go-final`.

## `angzarr probe` runs the host's probe

angzarr-cli depends on angzarr-embedded's host crate, `angzarr-embedded-host`
(the owner's ruling, 2026-11-01), for `angzarr probe` alone
(`crates/angzarr-cli/src/probe.rs`): the host's config reader
(`config::read`) and its child conformance probe (`probe`, with
`Input::command` and `Input::event`). It is pinned at the rev the
host-config crate is, and moves with it. A probe's failures carry the
spec's keys and the host's codes, so the probe renders each, where it is
shown, through `angzarr_diagnostic::catalog::render` with the catalog its
code is in (`angzarr_proto::REASONS_EN_US`, `angzarr_embedded_host::EN_US`);
the cli's own refusals around it go through `output::finish_shown`, which
writes the steps and then fails the command.

Its tests (`tests/probe.rs`) launch shell commands as fake children that
break the protocol at once, so none waits for a time limit; the child that
passes every step is client-python's shim, as the Python player runs it,
an ignored test that needs uv and `ANGZARR_PLAYER_DIR` naming the player's
checkout.
