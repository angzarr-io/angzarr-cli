# angzarr-cli

The `angzarr` command line: the single source of per-component wiring for
angzarr projects, generated from the `io.angzarr.v1` component declarations,
plus the tooling to develop and operate them.

| Crate | What it is |
|---|---|
| [`angzarr-fmt`](crates/angzarr-fmt) | CLI output: one result rendered as json, yaml, toml, text or markdown; localizable failures and warnings |
| [`angzarr-fmt-clap`](crates/angzarr-fmt-clap) | angzarr-fmt for clap: the global `--format`, format resolution, warnings and the process tail |
| [`angzarr-cli`](crates/angzarr-cli) | the `angzarr` binary |

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

A command can write its result and still fail: a lint writes its findings
(a list of diagnostics, each with a severity and an optional location) and
exits 1 when any is an error.

## Building

```
just build    # cargo build --workspace --all-targets
just test     # cargo test --workspace
just lint     # clippy -D warnings and the rustfmt check
```

The toolchain is pinned in `rust-toolchain.toml`.

## Licence

AGPL-3.0-only ([LICENSE](LICENSE)).
