# angzarr-cli

The `angzarr` command line: the single source of per-component wiring for
angzarr projects, generated from the `io.angzarr.v1` component declarations,
plus the tooling to develop and operate them.

| Crate | What it is |
|---|---|
| [`angzarr-fmt`](crates/angzarr-fmt) | CLI output: one result rendered as json, yaml, toml, text or markdown; localizable failures and warnings |
| [`angzarr-fmt-clap`](crates/angzarr-fmt-clap) | angzarr-fmt for clap: the global `--format`, format resolution, warnings and the process tail |
| [`angzarr-cli-core`](crates/angzarr-cli-core) | the component model read from the `io.angzarr.v1` options, the lint rules, the run domain (annals), and a project's settings and their validation; pure |
| [`angzarr-cli-app`](crates/angzarr-cli-app) | the use cases (lint, model, init) behind the `DescriptorSource` and `Project` ports |
| [`angzarr-cli-adapters`](crates/angzarr-cli-adapters) | a descriptor set from a file or stdin; `.angzarr/` on disk and in memory |
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

## The project directory

`.angzarr/` at a project's root holds the committed settings that make
every checkout generate the same thing. Nothing about a run is kept there.

- `config.yaml`: the enabled language targets (`rust`, `python`), where the
  protos are (`protos`, relative to the project root), where each target's
  generated wiring goes (`targets.<target>.out`), and output defaults
  (`output.non_tty_format`).
- `lock.yaml`: the angzarr-project spec commit the project builds against
  (`spec.commit`, the full commit id), and the client API version each
  target emits for (`targets.<target>.client_api`).
- `cache/` is not committed: `.angzarr/.gitignore` ignores it.

```yaml
# .angzarr/config.yaml
version: 1
protos: proto
targets:
  rust:
    out: gen/rust
output:
  non_tty_format: yaml

# .angzarr/lock.yaml
version: 1
spec:
  commit: b1f00d2710eb7bee60308f6df520893d782012a6
targets: {}
```

Both are versioned documents (a root `version`, read by annals-yaml). A
key either file does not define is an error naming the file and the key,
as is an unknown target, a format that is not one of the five, a commit
id that is not 40 (or 64) hex digits, and a `version` newer than this
build reads.

`angzarr init --spec-commit <sha> [--target rust] [--target python]
[--dry-run]` creates `.angzarr/` in the current directory: a config
enabling the targets given (none by default, each generating into
`gen/<target>`, protos in `proto/`), a lock on the commit, and the
`.gitignore`. It refuses when `.angzarr/` is already there. `--dry-run`
writes the same result and creates nothing.

## Output

Every command builds one result. `--format` picks how it is written:
`json`, `yaml`, `toml`, `text` or `markdown`. Unset, a terminal gets `text`;
anything else (a pipe, a file, CI) gets the `output.non_tty_format` of the
nearest `.angzarr/config.yaml` from the current directory up, or `json`
when there is none. A config that does not load is a warning, and `json`
is used.

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
