# angzarr-cli

The `angzarr` command line: the single source of per-component wiring for
angzarr projects, generated from the `io.angzarr.v1` component declarations,
plus the tooling to develop and operate them.

| Crate | What it is |
|---|---|
| [`angzarr-fmt`](crates/angzarr-fmt) | CLI output: one result rendered as json or text; localizable failures |
| [`angzarr-codegen`](crates/angzarr-codegen) | the generator core: descriptors in, source out (domain layer, Rust and Python scaffolds, adapters, `register`) |
| [`angzarr-build`](crates/angzarr-build) | `build.rs` helpers running protoc and writing what angzarr-codegen generates |
| [`angzarr-cli`](crates/angzarr-cli) | the `angzarr` binary, with angzarr-fmt's clap side: the global `--format` and the process tail |

## Output

Every command builds one result. `--format` picks how it is written:
`json` or `text`. Unset, a terminal gets `text` and anything else (a pipe,
a file, CI) gets `json`.

```
$ angzarr version
Name: angzarr
Version: 0.1.0
Commit: (none)

$ angzarr version | jq .version
"0.1.0"
```

A failure goes to stderr in the same format: `{"error": {...}, "remedy":
{...}}` in json, `Error:` and `Fix:` lines in text. Each
diagnostic has a stable code and named args (`angzarr-embedded-diagnostic`), rendered
through the crate's en-US Fluent catalog.

## Scaffold

`angzarr scaffold rust` writes each component's interface and
implementation into the crate's `src`, once: `wallet.rs`, a trait over the
domain types (one method per command, fact and event; a policy's per event it
reacts to, taking the source stream's root, the event and its `PolicyContext`
and returning the commands to send, each with its target root, or its
`Bug`), and `wallet_impl.rs`, its implementation (every body `todo!()`,
the state the wire type). Each command takes its `Context` too (the instant
its events are recorded at, and the router's context of the input as
`ctx.command`), and so does each fact, which returns its event with no
`Result`: a fact is recorded, never refused. Appliers stay folds of the
state and the event. Neither file imports anything from angzarr-embedded
directly: the context types, `Failed`, `BusinessStatus`,
`RetryableStatus` and `Bug` are `crate::domain`'s, which re-exports or defines them. An existing file is the user's and is kept. `--missing` adds
the methods an existing file lacks and changes nothing else, refusing an
input taken as `cmd` where the proto declares a fact, or as `fact` where it
declares a command. `--translate <policy>:<command>=<fact>` sets a policy's
refusal switch to translate that command's refusals into a fact of its
source, adding a `<command>_refused` method, which takes the refused
command, its typed refusal if it has one, and the refused command's
`CommandContext`, and returns `Result<Fact, Bug>`. A policy's `Bug`, a
diagnostic key and its args, is a defect (a policy reacts to events the
application's own domains recorded): angzarr-embedded dead-letters it.

A handler fails in one of four classes: a business rejection, a "no" the
command's issuer is told of; an immediate return, "re-read and re-decide";
a fatal bug, a defect angzarr-embedded alerts and dead-letters; or a
retryable bug, a defect outside the handler that may clear. A command
returns `Result<_, Failed<Rejection>>` (the domain's untyped rejection, a
diagnostic key and its args) or, refusing one typed refusal,
`Result<_, Failed<R>>`: `Err(rejection)?` refuses under
FAILED_PRECONDITION, `Failed::from(rejection).with_status(..)` under another
`BusinessStatus`; `Err(Failed::immediate(&Bug::new("key")))` returns
ABORTED under its key, back to a caller at once, a policy's step reloaded
and run again under the capped backoff, then dead-lettered;
`Err(Bug::new("key").arg(..))?` fails with a fatal bug, dead-lettered as
ROUTER_REASON_UNHANDLED_HANDLER_ERROR with its key as the cause; and
`Err(Failed::retryable_bug(RetryableStatus::Unavailable, &bug))` fails with
a retryable bug under the status it picks, retried with capped backoff,
then dead-lettered with its retry count. The statuses are not
interchangeable: `Unavailable` means resend, the dependency is down for
now; `DeadlineExceeded` means resend only if the call is idempotent, since
the work may have happened (a command is, by its `command_id`);
`ResourceExhausted` means wait longer before resending, a quota or a
capacity limit was hit. A policy's bug is its `Bug`.
Its `Ok` is its one event, which may be changed to any shape
angzarr-embedded takes (`Option<E>`, `Vec<E>`), or, for a command declaring
several, `Vec<<Component>Event>`: any number of any of its events.

A message named `Context`, `PolicyContext`, `CommandContext`, `Rejection`,
`Bug`, `Failed`, `BusinessStatus` or `RetryableStatus` (the names the generated domain layers give the
runtime's types)
fails generation, naming the message; rename it. Files scaffolded before the
handlers took their context are not migrated by `--missing`, which takes a
method taking the same message as present: edit their signatures by hand.

```
$ angzarr scaffold rust -I proto --src src buyin/v1/wallet.proto buyin/v1/buy_in.proto buyin/v1/policies.proto
```

The build script generates the rest into `OUT_DIR` with `angzarr-build`, the
CLI's and regenerated every build: `markup_domain` (the domain types, their
`DomainMessage` impls, `Rejection`, `Bug`, the re-exported `Context`,
`PolicyContext`, `CommandContext`, `Failed`, `BusinessStatus` and `RetryableStatus`, each component's `<Component>Command`
and `<Component>Event` enums), `adapters` (each component's angzarr-embedded
markup, named `policy-<input>-<output>` for a policy, delegating to its
interface; a domain served by a child process gets a contract instead),
`descriptors` (`DESCRIPTORS`: each component's `ComponentDescriptor`,
generated from its protos and encoded, the crate's version its
`rules_version`; a domain served by a child process has its own) and
`register` (one `register(&mut Registry)` for every adapter). An
application's own `src/main.rs` runs `register` with its host config
(`angzarr_embedded_host::run("angzarr.toml", register)`).

`angzarr scaffold python --domain <domain>` writes one domain's uv project,
served to angzarr-embedded as a child process through angzarr-client-python's
harness (`angzarr_client.child`). The user's files are written once and
completed with `--missing`: `pyproject.toml` (angzarr-client pinned by
commit, `CLIENT_PYTHON_REV`), `logic.py`, the domain's logic interface
(`<Domain>Logic`, a `typing.Protocol`: per command a decision taking the
command and the state and returning its events, per event a fold taking the
state and the event and returning the next state) and `logic_impl.py`, its
implementation (each method a TODO). Both take and return the domain's plain
types, each decision its `CommandContext` too, and import nothing of angzarr
or protobuf directly: `domain.py` re-exports the context from
`angzarr_client.router`, with the two classes a decision fails with, both
angzarr-client-python's and each a diagnostic key and its args: `Rejection`,
raised to refuse its command (`raise Rejection("key", name=value)`, a
business "no" under a business status: FAILED_PRECONDITION unless given
second, or INVALID_ARGUMENT, NOT_FOUND, ALREADY_EXISTS, OUT_OF_RANGE or
PERMISSION_DENIED), and `Bug`, raised for a defect (`raise
Bug("key", name=value)`, answered ROUTER_REASON_UNHANDLED_HANDLER_ERROR with the key and
args as its cause; angzarr-embedded alerts it and dead-letters the command). The CLI's files are rewritten
on every run: `domain.py` (the domain's messages as frozen dataclasses and
integer enums; each event a command emits gets a classmethod building it
from that command, `MoveDecided.from_command(cmd, action=...)`, copying
every field the two share by name and type and taking the rest as keywords,
or `from_<command>` per command when several emit it), `_descriptor.py`
(the domain's `ComponentDescriptor`, generated from its protos with
`--rules-version`, for the child to answer `Describe` with unchanged),
`buf.gen.yaml`, `main.py`, `__main__.py`, `_version.py` and
the dispatch under `_gen`, which converts the wire's messages to the plain
types and back around each call. Every run ends with `buf generate` (the
message types, their imports rerooted, angzarr's own under
`angzarr_client.proto`) and `uv lock`, so the project is ready for
angzarr-embedded's `uv run --locked angzarr-<domain>`, the lock and the
message types on disk to commit; `--no-proto-gen` and `--no-lock` skip them.

```
$ angzarr scaffold python -I proto --src player --domain player \
    --proto-git https://github.com/angzarr-io/angzarr-project --proto-ref <sha> \
    io/angzarr/examples/v1/player.proto
```

Every scaffold reads the other protos of its protos' directories too, so a
policy or a projector a scaffolded message names is found without being
scaffolded; a projector is never scaffolded.

## Building

```
just build    # cargo build --workspace --all-targets
just test     # cargo test --workspace
just lint     # clippy -D warnings and the rustfmt check
just test-pkg angzarr-codegen   # one package's tests
just mutants-staged            # cargo-mutants over the staged Rust diff
just mutants-all               # cargo-mutants over the workspace, judged per crate
just hooks-install             # once per clone: lefthook's git hooks
```

The git hooks (`lefthook.yml`): pre-commit formats the staged Rust, lints
and runs every test (failing tests may be committed off main, with a
warning); pre-push (`just push-check`) runs every test of each
pushed commit, in a tree of its own, then mutation-tests what the push adds
against each crate's `mutation_thresholds` (justfile). Failing tests may be
pushed to a branch, with a warning, but not to main.

The toolchain is pinned in `rust-toolchain.toml`.

## Licence

AGPL-3.0-only ([LICENSE](LICENSE)), except `angzarr-build`, which build
scripts depend on, and `angzarr-codegen`, which it depends on: BSD-3-Clause
([crates/angzarr-build/LICENSE](crates/angzarr-build/LICENSE),
[crates/angzarr-codegen/LICENSE](crates/angzarr-codegen/LICENSE)).
