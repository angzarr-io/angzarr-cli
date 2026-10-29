# angzarr-cli

The `angzarr` command line: the single source of per-component wiring for
angzarr projects, generated from the `io.angzarr.v1` component declarations,
plus the tooling to develop and operate them.

| Crate | What it is |
|---|---|
| [`angzarr-fmt`](crates/angzarr-fmt) | CLI output: one result rendered as json, yaml, toml, text or markdown; localizable failures and warnings |
| [`angzarr-fmt-clap`](crates/angzarr-fmt-clap) | angzarr-fmt for clap: the global `--format`, format resolution, the process tail |
| [`angzarr-codegen`](crates/angzarr-codegen) | the generator core: descriptors in, Rust source out (domain layer, markup scaffold, `register`) |
| [`angzarr-build`](crates/angzarr-build) | `build.rs` helpers running protoc and writing what angzarr-codegen generates |
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

A failure goes to stderr in the same format: `{"error": {...}, "remedy":
{...}}` in a structured format, `Error:` and `Fix:` lines in text. Each
diagnostic has a stable code and named args (`angzarr-embedded-diagnostic`), rendered
through the crate's en-US Fluent catalog.

## Scaffold

`angzarr scaffold rust` writes each component's markup (an aggregate,
process manager or policy, with a `todo!()` stub per input and event) into
the crate's `src`, one file per component, once: an existing file is the
user's and is kept. `--missing` adds the stubs an existing file lacks and
changes nothing else, refusing a `#[handles_cmd]` on a fact or a
`#[handles_fact]` on a command. `--translate <policy>:<command>=<fact>` sets
a policy's refusal switch to translate that command's refusals into a fact
of its source, adding an `#[on_refused]` callback stub.

```
$ angzarr scaffold rust -I proto --src src buyin/v1/wallet.proto buyin/v1/buy_in.proto buyin/v1/policies.proto
```

The build script generates the rest into `OUT_DIR` with `angzarr-build`:
`markup_domain` (the domain types and their `DomainMessage` impls) and
`register` (one `register(&mut Registry)` for every component).

## Building

```
just build    # cargo build --workspace --all-targets
just test     # cargo test --workspace
just lint     # clippy -D warnings and the rustfmt check
```

The toolchain is pinned in `rust-toolchain.toml`.

## Licence

AGPL-3.0-only ([LICENSE](LICENSE)), except `angzarr-build`, which build
scripts depend on: BSD-3-Clause ([crates/angzarr-build/LICENSE](crates/angzarr-build/LICENSE)).
