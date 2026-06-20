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

## FFI bindings (what the generated wiring targets)

The dispatch table the emitter populates is a thin layer over each language's
angzarr client *binding* — the router semantics live once in the shared Rust
core (`angzarr-router` + its `router-ffi` C-ABI crate), consumed **in-process**
by every language. In-process is a hard requirement, not a preference: the
rebuild fold calls a business applier per event, and that fine-grained boundary
cannot afford a network or IPC hop (see the shared-router ADR). So each binding
loads the core through a language-native FFI mechanism and the generated wiring
calls into it:

| Language   | Core artifact | In-process FFI mechanism            |
|------------|---------------|-------------------------------------|
| Rust       | `rlib`        | direct (the core's native API)      |
| Go         | `cdylib`      | cgo                                 |
| Python     | `cdylib`      | cffi (dlopen)                       |
| Java       | `cdylib`      | Panama / FFM (`java.lang.foreign`)  |
| C#         | `cdylib`      | P/Invoke (`[LibraryImport]`)        |
| C++        | `staticlib`   | direct link (no runtime `.so`)      |
| TypeScript | `cdylib`      | koffi (pure-JS FFI, no addon build) |

The C-ABI is identical across all of them: a serialized descriptor registers a
component, a single host-callback gateway is routed by `callback_id`, and bytes
(protobuf `Any` end to end) cross the boundary with copy-at-the-boundary
ownership. The emitted code never touches the ABI — it only populates the
binding's typed dispatch table.

### Why Java 25

The JVM binding uses the Foreign Function & Memory API (Panama), not JNA. FFM's
**upcall stubs** make the single host-callback gateway a first-class native
function pointer — exactly what the dispatch trampoline needs — without JNA's
reflection cost or a hand-written JNI shim. FFM was preview in JDK 19–21 and was
**finalized in JDK 22** (JEP 454). We target **JDK 25**, the current LTS on which
FFM is final: the binding compiles and runs with **no `--enable-preview` flags**
and rides long-term support, rather than pinning to a preview API on JDK 21 or a
non-LTS release (22–24).

## License

AGPL-3.0 — see [LICENSE](LICENSE).
