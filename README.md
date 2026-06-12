# angzarr CLI

`angzarr` is the command-line tool for the [Angzarr](https://angzarr.io)
CQRS/Event Sourcing framework. Capabilities grow as subcommands; codegen
is the first.

## codegen

Generates per-language dispatch wiring from proto component declarations:
services carrying `(angzarr.v1.component)` options, with rpcs carrying
`(angzarr.v1.rejected/applies/reacts)`. For each declared component it
emits a strict handler interface (a missing handler is a compile error,
never a silent no-op) and a dispatch-table constructor over that
language's angzarr client engine.

Each language subcommand speaks the protoc plugin contract —
`CodeGeneratorRequest` on stdin, `CodeGeneratorResponse` on stdout — so
buf invokes it directly:

```yaml
# buf.gen.yaml
plugins:
  - local: ["angzarr", "codegen", "go"]
    out: proto
    opt: paths=source_relative
```

```bash
angzarr codegen languages   # list registered emitters
```

Declaration validation is language-independent and runs before any
emitter: missing required component fields (state, domains) and
unresolvable or non-fully-qualified type names fail generation
identically for every target language.

The angzarr option extensions are read dynamically from the request's own
descriptor set — this module ships no compiled proto bindings, so client
libraries can link it in-process (e.g. to drive validation suites)
without duplicate-registration conflicts.

## Adding a language

Implement `codegen.Emitter` (`Lang`, `Suffix`, `EmitFile`) and register it
in the emitter table; the subcommand appears automatically. Generated
code must be a thin table population over that language's engine —
dispatch logic lives in the engine, never in generated code.

## License

AGPL-3.0 — see [LICENSE](LICENSE).
