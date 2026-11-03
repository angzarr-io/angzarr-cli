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
diagnostic has a stable code and named args (angzarr core's `angzarr-diagnostic`), rendered
through the crate's en-US Fluent catalog.

## Scaffold

Every file a scaffold writes is one of two kinds. The CLI's is regenerated
on every run, opens with the generated marker (`@generated`), and lives only
where generated code goes: `OUT_DIR` in Rust, the package's `_gen/` in
Python, the one path a project's gates exclude. The user's is written once,
never rewritten, and holds the business logic alone; it implements a
generated, typed interface, so the user never edits a generated file to make
the application work and a run never overwrites business logic. A generated
path holding a file without the marker (edited by hand, or written in its
place) is refused, nothing written, naming its remedy: move the change out,
then restore or delete the file (one an older CLI wrote without the marker
is safe to delete). A marked file under `_gen/` that a run no longer writes
is an orphan, and deleted. A file an earlier CLI generated at what is now a
user's path (its marker says so) becomes the user's, written once. A
re-run reports each existing user file `kept` with what it `lacks` (required
methods it does not have), what is `obsolete` (methods for a message the
protos no longer declare there) and what is `optional` (a Python domain's
refusal handlers it may define and does not), a manifest with its pins that `drift` from this
CLI's (never editing it), and the interface files an earlier CLI wrote
beside the implementation as `obsolete`, for the user to delete.

`angzarr scaffold rust` writes each component's implementation into the
crate's `src`, once: `wallet_impl.rs` (every body `todo!()`, the state the
wire type, a projector's `async fn`s holding its projection). Its
interface is generated into the domain layer (`markup_domain`), so an
implementation in another crate implements it: a trait over the domain
types, one method per command, fact and event; a policy's per event it
reacts to, taking the source stream's root, the event and its
`PolicyContext` and returning what it sends its target, each with its target
root, as the domain layer's `<Target>Output` (its commands and the facts it
declares, each sent in the arm its `(command).fact` declares: angzarr
decision 14.69), or its `Bug`; a projector's, async, per event it projects,
taking the event and its `ProjectionContext` and returning its future of
`Result<(), Bug>`. A change to the protos reaches the user as a compile error
on their implementation, never inside the generated layer. Each command takes
its `Context` too (the instant its events are recorded at, and the router's
context of the input as `ctx.command`), and so does each fact, which returns
its events with no `Result`: a fact is recorded, never refused. Appliers stay
folds of the state and the event. The implementation imports nothing from
angzarr-embedded directly: the context types, `Failed`, `BusinessStatus`,
`RetryableStatus`, `Compensation` and `Bug` are `crate::domain`'s, which
re-exports or defines them. `--missing` adds the required methods an
existing file lacks and changes nothing else, refusing an input taken as
`cmd` where the proto declares a fact, or as `fact` where it declares a
command.

Every command a policy sends has a refusal path (angzarr decision 14.76): a
policy translates the refusal of each command of its target into one of its
source's facts. Its interface has a required `<command>_refused` per
command, taking the refused command, its typed refusal if it has one
(`Option<T>`, or for a command refusing several types
`Option<<Command>Refuses>`, the domain layer's choice of them its handler
refuses with), and the refused command's `CommandContext`, and returning
`Result<<Source>Fact, Bug>`, the domain layer's choice of the source's
facts, or a `Bug` (for a command it never sends): it cannot decline. So a
command of a policy's target that names no `refuses` type, or a policy
whose source takes no fact, fails generation and `angzarr lint`. Each is
written with the implementation, and `--missing` adds one an existing file
lacks. The domain layer's `<Target>Output` is a `markup::CommandTarget` too
(its domain and its commands): what a process manager's `Compensation` step
names as its destination. A policy's `Bug`, a diagnostic key and its args, is a defect (a policy reacts
to events the application's own domains recorded): angzarr-embedded
dead-letters it.

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
Its `Ok` is a list of the component's events, `Vec<<Component>Event>`,
empty, one or many (decision 14.75): there is no emits cardinality. A fact
returns the same list, unwrapped.

A typed refusal is a diagnostic: its code is `refusal_key` of its full name
and, when it holds a nested `Reason` enum as `reason`, the reason's wire name
(the value's name without the enum's prefix, `TableRefused` refusing
`REASON_ALREADY_BET` as `io-angzarr-examples-v1-tablerefused-already-bet`);
its args are its set fields, an enum as its wire name.
`angzarr_codegen::refusal_key` is public, so a hand-written refusal
reproduces the key. A refusal with a `Reason` is refused under its code's
status (decision 14.52): the value's `(io.angzarr.v1.refusal_status)`,
FAILED_PRECONDITION where unmarked. Rust's has `status()` and
`with_code_status`, which the adapter applies whatever status the handler
gave (a choice of several applies the chosen type's, a type with no
`Reason` keeping the handler's); Python's has `status()`, a
`google.rpc.Code`, which the dispatch's `Refused` goes out under. The
option anywhere but a non-zero value of the nested `Reason` of a message a
command refuses with fails generation. An `(event)` entry naming a component outside the protos
generated is a reference to one generated elsewhere, skipped.

A component's name in an application (the host config's `component`
entry's `name`, a policy's or a projector's registered name) is its
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
runtime's types), or a domain type taking a component's interface's name or
its facts' (`<Component>Fact`), fails generation, naming the message; rename
it, or name the component.

```
$ angzarr scaffold rust -I proto --src src buyin/v1/wallet.proto buyin/v1/buy_in.proto buyin/v1/policies.proto
```

The build script generates the rest into `OUT_DIR` with `angzarr-build`, the
CLI's and regenerated every build. `markup_domain` is generated for prost
types built with `wire`'s settings (bytes as `Vec<u8>`, maps as `BTreeMap`),
and a prost crate built otherwise fails to compile with
`angzarr-codegen-wire-bytes` or `angzarr-codegen-wire-map` at the field:
`wire` (prost's types, and `angzarr-wire.rs`, their modules nested as their
packages are, each carrying the allowance prost's output needs, for the
crate root to `include!`), `markup_domain` (the domain types, their
`DomainMessage` impls, `Rejection`, `Bug`, the re-exported `Context`,
`PolicyContext`, `CommandContext`, `Failed`, `BusinessStatus`,
`RetryableStatus` and `Compensation`, each component's `<Component>Command`
and `<Component>Event` enums, each with its `Payload`, the domain's payloads,
a domain's `<Component>Fact`, and each component's interface),
`adapters` (the module `adapters`, carrying the allowances it needs, so no
include site does: each component's angzarr-embedded markup, naming those
enums (`command = ..`, `event = ..`), named `policy-<input>-<output>` for a
policy, delegating to its interface, a policy's with an `#[on_refused]` per
command of its target, each translating; a domain served by a child process is instead its command set,
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
`register` (one `register(&mut Registry) -> Result<(), Misconfigured>`
for every adapter, refused as the registry refuses a component). An
application's own `src/main.rs` runs `register` with its host config
(`angzarr_embedded_host::run("angzarr.yaml", register)`). A crate that
writes its policies by hand beside a domain a child process serves takes
that domain's command set and event set from `stand_ins` over the
domain's proto (`angzarr-stand-ins.rs`: the same `CommandTarget`,
`Decides`, `EventSource` and `Ingests` the adapters give a remote domain,
over `crate::domain`'s types, and nothing else), rather than writing them
by hand.

`angzarr scaffold python --domain <name>` writes one component's uv
project, served to angzarr-embedded as a child process through
angzarr-client-python's harness (`angzarr_client.child`). `--domain` names
a domain (its aggregate or process manager) or a component's name in an
application: its `component_name`, else its kind's default
(`policy-<input>-<output>`, `projector-` and its anchor in kebab case), as
the ledger's `projector-wallet-table-ledger`. The dispatch and the
descriptor carry that name. The package's root holds the user's files
alone, written once: `pyproject.toml` (angzarr-client pinned by commit,
`CLIENT_PYTHON_REV`; its script is `<package>.__main__:main`),
`__init__.py`, `__main__.py`, the composition root (it builds the logic, its
dependencies its constructor's arguments, and hands it to `_gen.main.run`),
and `logic_impl.py`, the implementation (each method a TODO) of the
generated interface `_gen/logic.py`: an `abc.ABC` of the required methods
(`<Domain>Logic` for a domain, per command or fact a decision taking the
input, the state and its `CommandContext`, per event a fold taking the state
and the event and returning the next state; `<Name>Logic` for a policy, per
event a reaction taking the event, its source root and its `PageContext` and
returning the target's commands and facts, each with its root, each sent in
the arm the target declares; for a projector, per event a coroutine taking
the projection, the event and its `PageContext`, the source domain its
cover's, and returning the next projection), so an implementation lacking
one fails to instantiate, naming it. A decision returns a list of its events
(decision 14.75: of its emits types, or, declaring none, of any of its
domain's events) or one of its typed refusals, several to a command, answered as the client's `Refused`
with the refusal on the wire; a refusal's `Reason` enum is its rejection
code, and its plain type gives the generator's diagnostic `key()` (the Rust
domain layer's code for the same refusal, `refusal_key`; an unset reason
keys as the refusal alone) and `args()` (its set fields). A fact's events
flag it, recorded as received. A refusal of a command a policy sends comes
back to the policy's source (decision 14.31). A policy's
`<command>_refused`, per command of its target (decision 14.76), takes the
refused command, its typed refusal or None and the refused command's
`CommandContext` and returns the fact of its source the refusal becomes; it
is an abstract method of the interface, required, and the dispatch
registers each with the client's `on_refused`. The implementation is
created with each, the one `--translate <policy>:<command>=<fact>` names
returning that fact. A domain's `on_refused_<command>`, per command its
policies send (decision 14.73), takes the refused command, its typed
refusal or None, the state and its `CommandContext`, and returns the events
that record a hand-written policy's pass-through `CommandRefused`, as a
fact's flag it; it is optional, declared in a reference `Protocol`
(`<Domain>Refusals`) the implementation never subclasses, and registered
when the logic has it (`hasattr`), so deleting one declines that refusal.
The implementation is created with one per such command but one
`--translate` names; `--missing` never adds one back, and `--on-refused
<command>` writes the one it names into an existing implementation. The logic takes
and returns the plain types and imports nothing of angzarr or protobuf
directly: `_gen/domain.py` re-exports the context from
`angzarr_client.router`, with the classes the logic fails with, both
angzarr-client-python's and each a diagnostic key and its args: `Rejection`
(a domain's), raised to refuse its command (`raise Rejection("key",
name=value)`, a business "no" under a business status: FAILED_PRECONDITION
unless given second, or INVALID_ARGUMENT, NOT_FOUND, ALREADY_EXISTS,
OUT_OF_RANGE or PERMISSION_DENIED), and `Bug`, raised for a defect (`raise
Bug("key", name=value)`, answered ROUTER_REASON_UNHANDLED_HANDLER_ERROR with
the key and args as its cause; angzarr-embedded alerts it and dead-letters
the command); a process manager's also `Compensation`, its rollback tooling
(decision 14.74). Everything under `_gen/` is the CLI's, rewritten on every
run, each import of the project's own relative and none of a user module:
`domain.py` (the messages as frozen dataclasses and integer enums, a map a
dict; each event a command emits gets a classmethod building it from that
command, `MoveDecided.from_command(cmd, action=...)`, copying every field
the two share by name and type and taking the rest as keywords, or
`from_<command>` per command when several emit it), `logic.py`,
`_descriptor.py` (the component's `ComponentDescriptor`, generated from its
protos with `--rules-version`, for the child to answer `Describe` with
unchanged), `main.py` (the child's host over the logic it is handed),
`_version.py`, the dispatch, which converts the wire's messages to the plain
types and back around each call (a projector's coroutines run on the
dispatch's one event loop), and buf's message types; so is the project's
`buf.gen.yaml`. Each generated file stands on angzarr-client-python's
public API alone (`angzarr_client.router`'s `AggregateDispatch`,
`ProcessManagerDispatch`, `PolicyDispatch` and `ProjectorDispatch`,
`ComponentHost.add_<kind>(dispatch, descriptor)`, `run_child`), so a
hand-written dispatch can be hosted in its place from `__main__.py`; the
client's `describe(dispatch)` of a generated dispatch is its generated
descriptor, so a hand-written one is described without the hex. Every run
ends with `buf generate` (the message types, headed with the generated
marker, their imports rerooted: angzarr's own under `angzarr_client.proto`,
the project's relative) and `uv lock`, so the project is ready for
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

`angzarr scaffold app -I proto --src . [--name N] [--language NAME=LANGUAGE]...
[--expose NAME=loopback|ip]... [--max-retries N] [--max-race-runs N]
[--in-memory] [--angzarr-build-path DIR] [--domain-crate CRATE
[--domain-wire PATH]] <protos>` writes an
angzarr-embedded application's startup into the crate root. The user's, written
once and never rewritten: `angzarr.yaml`, the host config, seeded from the
flags at the current `schema_version`; `Cargo.toml`, angzarr-embedded's crates pinned at the commit this CLI
generates for, angzarr-build the directory given or this CLI's repository;
`src/main.rs`, a module per in-process component's implementation, the
driver's, and an async `main` running the host with the generated
`register()`, which registers the in-process components the config lists,
and the user's own modules beside them; and `src/driver.rs`, what the
application does once it is up. The CLI's, rewritten on every run from the
protos and the `angzarr.yaml` on disk: `build.rs`, one call to
`angzarr_build::app`, which writes the generated layer into `OUT_DIR` every
build, `angzarr-app.rs` (the wire types, the domain layer, the adapters and
`register()`) among it, and `src/main.rs` includes it.

An application whose domain layer a library crate already generates (its
build script running angzarr-build's `wire` and `markup_domain`, as
examples-rust's `examples_proto` does) uses that crate's types rather than
generating its own: `--domain-crate examples_proto` writes
`[package.metadata.angzarr.domain]` into the new manifest (`crate`, and
`wire` from `--domain-wire examples_proto::v1`), and while the manifest on
disk names one, `build.rs` calls `angzarr_build::app_without_domain`, which
generates the adapters, descriptors and `register()` alone, its
`angzarr-app.rs` `pub use examples_proto::domain;` (and `pub use
examples_proto::v1 as wire;`) in place of the generated modules. Add the
crate to `[dependencies]`; it must be generated from the same protos.

The config lists every component, in-process policies and projectors too,
each by its name from the protos (`component_name`, or its kind's default)
and with its `kind` and `language`: every aggregate and process manager of
the protos, every policy between two of them, and every projector whose
sources are all among them (a Python child's events are recorded by the
host, so a projector of them may run in process). Each is Rust, in process,
unless `--language <name>=<language>` names it (a domain by its name or its
domain): a `python` one is a child, the project `angzarr scaffold python`
writes for it, `dir` and `entry: [angzarr-<project>]` its domain's for a
domain and its name for a policy or a projector (`--language
policy-table-player=python` is `dir: policy-table-player`). A language
the host has no default launch for is written as given, and the lint
refuses it until the config gives it a `launch`. A child's policy or
projector has no adapter, descriptor or module in the application. A
component is
exposed only when
`--expose` names it; `retry` holds only the keys seeded (`max_retries`,
`max_race_runs`); `store` is `in_memory: true` or the `angzarr.db` file;
`policy` and `supervision` are left to the host's defaults.

`angzarr lint app [--config angzarr.yaml] <protos>` checks a config against
the protos, as `scaffold app` does before it writes anything (a config with
lints is refused, and nothing is written). It reads the config through
angzarr-embedded-host-config, the schema angzarr-embedded-host runs: a newer
`schema_version`, an unknown key or value, or a component with no `kind`, is
refused, naming its key path; a file with no stamp, or stamped version 1, reads as the current version 2, each key it leaves out its default. The lints: a listed domain or policy no component of the
protos is; a component listed twice; a domain of the protos the config does
not list; a listed policy whose source or target domain no listed component
owns (list the target, with its `launch`, if it runs elsewhere); an exposed
component that is not an aggregate (a process manager, a policy, a projector
or an upcaster); a language with no default launch and no `launch`; an empty
`launch`. A declaration of the protos the contract forbids is refused (a
process manager subscribing to another domain or declaring an output
domain, a policy with none, `starts` on anything but a process manager's
input, a command refusing with another domain's type). The protos'
warnings are shown and fail nothing: a command emitting an event its
component folds none of, a policy whose source or target domain no
component of the protos owns, a component naming nothing it handles, a
process manager nothing starts.

## Probing a child

`angzarr probe <component> [--config angzarr.yaml] [--command TYPE_URL |
--event TYPE_URL] [--domain DOMAIN] [--payload FILE]` takes one component of
the host config through the child protocol (angzarr-project's
`docs/design/child-protocol.md`) with angzarr-embedded-host's conformance
probe: it launches the child as the host would, from its `component` entry
and the config's `supervision` time limits, and runs six steps in order,
`launch`, `handshake`, `health` (SERVING on grpc.health.v1), `describe` (a
valid descriptor of the entry's `kind`), `handle` (one input answered) and
`shutdown` (an exit with status 0 on its stdin's end). A step that fails
ends the probe; the steps after it are `not-run`, and an upcaster sidecar
runs neither `describe` nor `handle`. It is what a client library for a new
language runs against its own child shim.

The input is the command a domain is handed (`--command`) or the event a
policy or a projector is (`--event`), its payload's type URL, on the domain
`--domain` names (the component's name when unset) and the probe's fixed
root, its payload the encoded message in `--payload` (an empty message, every
field its default, when unset). An upcaster needs none. A language other
than `rust` and `python` is launched by its entry's `launch`.

Each step's outcome is `passed`, `failed` or `not-run`; a failure is its
code, its args (a nested cause of the same shape) and its English, rendered
through the catalog its code is in (the spec's `reasons.en-US.ftl` for a
framework key such as `HOST_REASON_HANDSHAKE_TIMEOUT`, angzarr-embedded-host's
for the host's own codes) by `angzarr_diagnostic::catalog::render`. A code in
neither, such as a child's own refusal key, is shown as its code and args.

```
$ angzarr probe player --command type.googleapis.com/io.angzarr.examples.v1.DecideMove --format text
Component: player
Passed: false
Step       Outcome  Detail
launch     passed
handshake  failed   component "player": stdout closed before a handshake [angzarr-embedded-host-handshake]
health     not-run
describe   not-run
handle     not-run
shutdown   not-run
```

In json the result is `{"component", "passed", "steps": [{"step",
"outcome", "failure": {"code", "message", "args"}}]}`, `failure` only on
a failed step. Any failed step fails the command: the result is written to
stdout, then `angzarr-cli-probe-failed` (the component, the first step that
failed) to stderr, and the exit status is 1. A config that cannot be read,
a component it does not list, a domain or policy given no input, and a
payload file that cannot be read are refused before anything starts.

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

A test's scratch directory, under the temporary directory, is a `TestDir`
(angzarr core's `angzarr-test-dir`, every crate's dev-dependency): it is
removed when the test ends, passing or failing. To keep a failing test's
directory for debugging, set `ANGZARR_KEEP_TEST_DIRS=1`; the test's stderr
names it.

The toolchain is pinned in `rust-toolchain.toml`.

### The spec's codegen tier

`just test` runs the spec's codegen tier, the submodule's
`angzarr-project/features/codegen`, against angzarr-codegen with cucumber
(`crates/angzarr-codegen/tests/spec.rs`): each scenario's protos are
written and compiled, then linted (every check generation makes) or
generated from. A scenario the runner does not run is tagged, with why,
in `tests/spec/tags.rs`: the one record of where angzarr-cli and the tier
part. `SPEC_RUN_TAGGED=1` runs the tagged ones too.

## Licence

AGPL-3.0-only ([LICENSE](LICENSE)), except `angzarr-build`, which build
scripts depend on, and `angzarr-codegen`, which it depends on: licensed under
either of Apache License 2.0 or BSD 3-Clause, at your option (each crate's
`LICENSE-APACHE` and `LICENSE-BSD`:
[crates/angzarr-build/](crates/angzarr-build/),
[crates/angzarr-codegen/](crates/angzarr-codegen/)).

The generated-output exception: [LICENSE-EXCEPTION](LICENSE-EXCEPTION).

Commercial licensing: the AGPL-3.0 parts of this project (not the
dual-licensed `angzarr-build` and `angzarr-codegen` above) are also
available under a commercial licence. The author is open to it; contact
Benjamin Abbitt.
