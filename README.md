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
`Bug`; a projector's, async, per event it projects, taking the event and
its `ProjectionContext` and returning its future of `Result<(), Bug>`),
and `wallet_impl.rs`, its implementation (every body `todo!()`, the state
the wire type, a projector's `async fn`s holding its projection). Each command takes its `Context` too (the instant
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
command, its typed refusal if it has one (`Option<T>`, or for a command
refusing several types `Option<<Command>Refuses>`, the domain layer's choice
of them its handler refuses with), and the refused command's
`CommandContext`, and returns `Result<Fact, Bug>`. A policy's `Bug`, a
diagnostic key and its args, is a defect (a policy reacts to events the
application's own domains recorded): angzarr-embedded dead-letters it.

A handler fails in one of four classes: a business rejection, a "no" the
command's issuer is told of; an immediate return, "re-read and re-decide";
a fatal bug, a defect angzarr-embedded alerts and dead-letters; or a
retryable bug, a defect outside the handler that may clear. A command
returns `Result<_, Failed<Rejection>>` (the domain's untyped rejection, a
diagnostic key and its args), refusing one typed refusal
`Result<_, Failed<R>>`, or refusing several `Result<_,
Failed<<Command>Refuses>>`, a choice `From` each: `Err(rejection)?` refuses under
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
Its `Ok` follows the input's `emits_cardinality` (decision 14.50): ONE, the
default, is one event of each emits type, the event or a tuple
(`(TableCreated, ShoeShuffled)`); OPTIONAL is that or nothing (`Option<E>`,
`Option<(A, B)>`); MANY is any number of any of its events,
`Vec<<Component>Event>`, and is what an input with no emits takes. ONE or
OPTIONAL with no emits, or with more than four (the markup's widest tuple),
fails generation. A fact returns the same shapes, unwrapped.

A typed refusal is a diagnostic: its code is `refusal_key` of its full name
and, when it holds a nested `Reason` enum as `reason`, the reason's wire name
(the value's name without the enum's prefix, `TableRefused` refusing
`REASON_ALREADY_BET` as `io-angzarr-examples-v1-tablerefused-already-bet`);
its args are its set fields, an enum as its wire name.
`angzarr_codegen::refusal_key` is public, so a hand-written refusal
reproduces the key. An `(event)` entry naming a component outside the protos
generated is a reference to one generated elsewhere, skipped.

A component's name in an application (the host config's `[[component]]`
`name`, a policy's or a projector's registered name) is its
`component_name`, or by default: an aggregate's domain (`table`), `pmg-` and
a process manager's domain (`pmg-buy-in`), `policy-<input>-<output>` for a
policy (`policy-table-player`), `projector-` and the kebab case of a
projector's anchor message (`projector-ledger-projection`). A name that is
not lower-case letters, digits and `-`, starting with a letter, fails
generation. `angzarr_codegen::component_name` computes it from the options
and `component_names` reads every component's from a descriptor set, so
hand-written code and its config take the generator's names.

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
CLI's and regenerated every build. `markup_domain` is generated for prost
types built with `wire`'s settings (bytes as `Vec<u8>`, maps as `BTreeMap`),
and a prost crate built otherwise fails to compile with
`angzarr-codegen-wire-bytes` or `angzarr-codegen-wire-map` at the field:
`markup_domain` (the domain types, their
`DomainMessage` impls, `Rejection`, `Bug`, the re-exported `Context`,
`PolicyContext`, `CommandContext`, `Failed`, `BusinessStatus` and `RetryableStatus`, each component's `<Component>Command`
and `<Component>Event` enums), `adapters` (each component's angzarr-embedded
markup, named `policy-<input>-<output>` for a policy, delegating to its
interface; a domain served by a child process is instead its command set,
a `CommandTarget` with `Decides` of each command, and its event set, an
`EventSource` with `Ingests` of each fact, which a policy names as its
target or source with no stand-in aggregate),
a projector's one `#[projector]` over all its sources,
its `input_domains` (or its one `input_domain`, else the domains its
`(event)` entries name), registered with `registry.projector`; a
projector written by hand is angzarr-router's `ProjectorHandler`, registered
beside `register` with `registry.projector_handler(&[sources], ..)`;
`descriptors` (`DESCRIPTORS`: each component's `ComponentDescriptor`, by
its name in an application, generated from its protos and encoded, the crate's version its
`rules_version`; a domain served by a child process has its own) and
`register` (one `register(&mut Registry)` for every adapter). An
application's own `src/main.rs` runs `register` with its host config
(`angzarr_embedded_host::run("angzarr.toml", register)`).

`angzarr scaffold python --domain <name>` writes one component's uv
project, served to angzarr-embedded as a child process through
angzarr-client-python's harness (`angzarr_client.child`). `--domain` names
a domain (its aggregate or process manager) or a component's name in an
application: its `component_name`, else its kind's default
(`policy-<input>-<output>`, `projector-` and its anchor in kebab case), as
the ledger's `projector-wallet-table-ledger`. The dispatch and the
descriptor carry that name. The user's files are written once and
completed with `--missing`: `pyproject.toml` (angzarr-client pinned by
commit, `CLIENT_PYTHON_REV`), `logic.py`, the logic interface (a
`typing.Protocol`: `<Domain>Logic` for a domain, per command or fact a
decision taking the input, the state and its `CommandContext`, per event a
fold taking the state and the event and returning the next state;
`<Name>Logic` for a policy, per event a reaction taking the event, its
source root and its `PageContext` and returning the target's commands, each
with its root; for a projector, per event a coroutine taking the
projection, the event and its `PageContext`, the source domain its cover's,
and returning the next projection) and `logic_impl.py`, its implementation
(each method a TODO). A decision returns its events as its command's emits
cardinality declares (one event, a tuple of several, that or None, or a
list of any of them; a command declaring no emits, a list of any of its
domain's events, as Rust's `Vec<<Component>Event>`) or one of its typed
refusals, several to a command,
answered as the client's `Refused` with the refusal on the wire; a
refusal's `Reason` enum is its rejection code, and its plain type gives the
generator's diagnostic `key()` (the Rust domain layer's code for the same
refusal, `refusal_key`; an unset reason keys as the refusal alone) and
`args()` (its set fields). A fact's events flag it, recorded as received.
Both take and return the plain types and import nothing of angzarr or
protobuf directly: `domain.py` re-exports the context from
`angzarr_client.router`, with the classes the logic fails with, both
angzarr-client-python's and each a diagnostic key and its args: `Rejection`
(a domain's), raised to refuse its command (`raise Rejection("key",
name=value)`, a business "no" under a business status: FAILED_PRECONDITION
unless given second, or INVALID_ARGUMENT, NOT_FOUND, ALREADY_EXISTS,
OUT_OF_RANGE or PERMISSION_DENIED), and `Bug`, raised for a defect (`raise
Bug("key", name=value)`, answered ROUTER_REASON_UNHANDLED_HANDLER_ERROR with
the key and args as its cause; angzarr-embedded alerts it and dead-letters
the command). The CLI's files are rewritten on every run: `domain.py` (the
messages as frozen dataclasses and integer enums, a map a dict; each event
a command emits gets a classmethod building it from that command,
`MoveDecided.from_command(cmd, action=...)`, copying every field the two
share by name and type and taking the rest as keywords, or
`from_<command>` per command when several emit it), `_descriptor.py` (the
component's `ComponentDescriptor`, generated from its protos with
`--rules-version`, for the child to answer `Describe` with unchanged),
`buf.gen.yaml`, `main.py`, `__main__.py`, `_version.py` and the dispatch
under `_gen`, which converts the wire's messages to the plain types and
back around each call; a projector's coroutines run on the dispatch's one
event loop. Each generated file stands on angzarr-client-python's public
API alone (`angzarr_client.router`'s `AggregateDispatch`,
`ProcessManagerDispatch`, `PolicyDispatch` and `ProjectorDispatch`,
`ComponentHost.add_<kind>(dispatch, descriptor)`, `run_child`), so a
hand-written one can take its place; the client's `describe(dispatch)` of a
generated dispatch is its generated descriptor, so a hand-written one is
described without the hex. Every run ends with `buf generate`
(the message types, their imports rerooted, angzarr's own under
`angzarr_client.proto`) and `uv lock`, so the project is ready for
angzarr-embedded's `uv run --locked angzarr-<name>`, the lock and the
message types on disk to commit; `--no-lock` skips the lock. `buf generate`
reads the protos from `--proto-git` at `--proto-ref` (under its `proto`, as
angzarr-project lays them out), else from the first `-I`.

```
$ angzarr scaffold python -I proto --src player --domain player \
    --proto-git https://github.com/angzarr-io/angzarr-project --proto-ref <sha> \
    io/angzarr/examples/v1/player.proto
```

Every scaffold reads the other protos of its protos' directories too, so a
policy or a projector a scaffolded message names is found without being
scaffolded.

## An application's startup

`angzarr scaffold app -I proto --src . [--name N] [--python DOMAIN]...
[--expose NAME=loopback|ip]... [--max-retries N] [--max-race-runs N]
[--in-memory] [--angzarr-build-path DIR] [--domain-crate CRATE
[--domain-wire PATH]] <protos>` writes an
angzarr-embedded application's startup into the crate root. The user's, written
once and never rewritten: `angzarr.toml`, the host config, seeded from the
flags; `Cargo.toml`, angzarr-embedded's crates pinned at the commit this CLI
generates for, angzarr-build the directory given or this CLI's repository;
and `src/driver.rs`, what the application does once it is up. The CLI's,
rewritten on every run from the protos and the `angzarr.toml` on disk:
`build.rs`, one call to `angzarr_build::app`, and `src/main.rs`, the generated
layer's modules, each in-process component's interface and implementation,
and an async `main` running the host with the generated `register()`, which
registers the in-process components the config lists.

An application whose domain layer a library crate already generates (its
build script running angzarr-build's `wire` and `markup_domain`, as
examples-rust's `examples_proto` does) uses that crate's types rather than
generating its own: `--domain-crate examples_proto` writes
`[package.metadata.angzarr.domain]` into the new manifest (`crate`, and
`wire` from `--domain-wire examples_proto::v1`), and while the manifest on
disk names one, `src/main.rs` is `pub use examples_proto::domain;` (and
`pub use examples_proto::v1 as wire;`) in place of the generated modules,
and `build.rs` calls `angzarr_build::app_without_domain`, which generates the
adapters, descriptors and `register()` alone. Add the crate to
`[dependencies]`; it must be generated from the same protos.

The config lists every component, in-process policies and projectors too,
each by its name from the protos (`component_name`, or its kind's default)
and with its `kind`: every aggregate and process manager of the protos (in
Rust, or a Python child for a `--python` domain, `dir = "<domain>"` and
`entry = ["angzarr-<domain>"]`), every policy between two of them, and every
projector whose sources are all among them (a Python child's events are
recorded by the host, so its projector runs in process). A component is
exposed only when
`--expose` names it; `[retry]` holds only the keys seeded (`max_retries`,
`max_race_runs`); `[store]` is `in_memory = true` or the `angzarr.db` file;
`[policy]` and `[supervision]` are left to the host's defaults.

`angzarr lint app [--config angzarr.toml] <protos>` checks a config against
the protos, as `scaffold app` does before it writes anything (a config with
lints is refused, and nothing is written). It reads the config as
angzarr-embedded-host does: an unknown key or value, or a component with no
`kind`, is refused. The lints: a listed domain or policy no component of the
protos is; a component listed twice; a domain of the protos the config does
not list; a listed policy whose source or target domain no listed component
owns (list the target, with its `launch`, if it runs elsewhere); an exposed
component that is not an aggregate (a process manager, a policy, a projector
or an upcaster); a language with no default launch and no `launch`; an empty
`launch`.

## Building

The spec's protos, the blackjack example's among them, are the
`angzarr-project` submodule's, pinned by commit: run `git submodule update
--init` once per clone.

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
pushed commit, in a tree of its own. Failing tests may be pushed to a
branch, with a warning, but not to main. No hook runs mutation testing:
`just mutants-staged` runs it by hand, and `just mutants-all` (weekly in
CI) judges each crate against its `mutation_thresholds` (justfile).

The toolchain is pinned in `rust-toolchain.toml`.

## Licence

AGPL-3.0-only ([LICENSE](LICENSE)), except `angzarr-build`, which build
scripts depend on, and `angzarr-codegen`, which it depends on: BSD-3-Clause
([crates/angzarr-build/LICENSE](crates/angzarr-build/LICENSE),
[crates/angzarr-codegen/LICENSE](crates/angzarr-codegen/LICENSE)).
