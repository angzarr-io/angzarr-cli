# angzarr-cli

The `angzarr` command line: the single source of per-component wiring for
angzarr projects, generated from the `io.angzarr.v1` component declarations,
plus the tooling to develop and operate them.

| Crate | What it is |
|---|---|
| [`angzarr-fmt`](crates/angzarr-fmt) | CLI output: one result rendered as json, yaml, toml, text or markdown; localizable failures and warnings |
| [`angzarr-fmt-clap`](crates/angzarr-fmt-clap) | angzarr-fmt for clap: the global `--format`, format resolution, warnings and the process tail |
| [`angzarr-cli-core`](crates/angzarr-cli-core) | the component model read from the `io.angzarr.v1` options, the lint rules, and the run domain (annals); pure |
| [`angzarr-cli-app`](crates/angzarr-cli-app) | the use cases (lint, model) behind the `DescriptorSource` port |
| [`angzarr-cli-adapters`](crates/angzarr-cli-adapters) | a descriptor set from a file or stdin |
| [`angzarr-cli`](crates/angzarr-cli) | the `angzarr` binary |

## Commands

`angzarr lint [<image>|-]` and `angzarr model [<image>|-]` read a compiled
descriptor set: a binary `FileDescriptorSet` (`protoc --include_imports
--descriptor_set_out=image.binpb`) or buf image (`buf build -o
image.binpb`), from a path or, given `-` or nothing, from stdin. The set
must include `io/angzarr/v1/options.proto`.

`model` writes every component the set declares: its anchor message, kind,
generated name, domain roles, commands, events, `(state)` option and
`(conflict)` markers.

`lint` writes `{"diagnostics": [...]}`: each finding has a code, a message,
a severity, a location (`file#message` or `file#message.field`) and, where
there is one, a remedy. It checks that commands and events name a declared
component of the right kind, that emitted events are fully-qualified
messages in the set, that each kind sets the domain roles it needs and
none it forbids, that option bytes are defined and decode, that generated
names do not collide, that no command or event is generated apart from its
component, and that `(state)` and `(conflict)` follow options.proto. It
exits 1 when any finding is an error.

```
$ buf build -o - | angzarr lint
$ angzarr model image.binpb --format yaml
```

## Output

Every command builds one result. `--format` picks how it is written:
`json`, `yaml`, `toml`, `text` or `markdown`. Unset, a terminal gets `text`
and anything else (a pipe, a file, CI) gets `json`.

```
$ angzarr version
Name: angzarr
Version: 0.1.0
Commit: (none)

$ angzarr version | jq .version
"0.1.0"
```

A failure goes to stderr: `{"error": {...}, "remedy": {...}}` in the
selected structured format when `--format` was given or stderr is not a
terminal, otherwise `Error:` and `Fix:` lines. Warnings follow the same
rule, as JSON Lines when structured. Each diagnostic has a stable code and
named args (`annals-diagnostic`), rendered through the crate's en-US Fluent
catalog.

A command can write its result and still fail: `angzarr lint` writes its
findings (diagnostics, each with a severity, a location and an optional
remedy) and exits 1 when any is an error.

## Building

```
just build    # cargo build --workspace --all-targets
just test     # cargo test --workspace
just lint     # clippy -D warnings and the rustfmt check
```

The toolchain is pinned in `rust-toolchain.toml`. Building needs `protoc`
on `PATH` (or `PROTOC` set): `angzarr-proto` generates its types with it.

## Licence

AGPL-3.0-only ([LICENSE](LICENSE)).
