# angzarr CLI

`angzarr` is the command-line tool for the [Angzarr](https://angzarr.io)
CQRS/Event Sourcing framework. Capabilities grow as subcommands; codegen
is the first.

## Commands

| command | what it does |
|---|---|
| `angzarr codegen <lang>` | protoc/buf plugin: per-component dispatch wiring, regenerated every run |
| `angzarr scaffold <lang>` | protoc/buf plugin: a developer-owned handler stub, written once |
| `angzarr codegen languages` | list the target languages: `cpp`, `csharp`, `go`, `java`, `python`, `typescript` |
| `angzarr lint [image\|-]` | validate the declarations in a buf image / FileDescriptorSet (`--request` for a CodeGeneratorRequest) |
| `angzarr version` / `--version` | print the build version (`just build` stamps `git describe`) |

## Declarations

Components are declared by **annotating messages** with the options from
`io/angzarr/v1/options.proto` (angzarr-project) — there are no services and no
rpcs:

- `(io.angzarr.v1.component)` on the anchor message (the state message of an
  aggregate / process manager / projector, or an empty marker for a saga):
  `kind`, `domain` (the stream it owns: aggregates and process managers),
  `input_domain` (a subscription: saga source, projector filter),
  `output_domain` / `output_domains` (command targets), `name`, `compensates`
  (rejections of commands it caused elsewhere), `undoes` (aggregates only:
  commands it executed and can undo on a CASCADE COMPENSATE `Compensate`).
- `(io.angzarr.v1.command)` on a command: `component` (anchor, fully
  qualified) and `emits`.
- repeated `(io.angzarr.v1.event)` on an event, one entry per consumer:
  `component`, `domain`, `applies`.

For each component, codegen emits a strict `<Name>Handler` interface (a missing
handler is a compile error, never a silent no-op), a `New<Name>Dispatch`
constructor and a `Register<Name>` helper over the language's
**angzarr-router** binding. `<Name>` is `(component).name`, or the anchor
message's name when unset.

```yaml
# buf.gen.yaml
version: v2
plugins:
  - local: ["angzarr", "codegen", "go"]
    out: gen
    opt: paths=source_relative
    strategy: all            # required: see below
  - local: ["angzarr", "scaffold", "go"]
    out: .
    opt: [paths=source_relative, out_dir=.]
    strategy: all
```

- **`strategy: all` is required.** Components reference their commands and
  events by name, not by import, so every file declaring part of a component
  must reach the plugin in one invocation. buf's default per-directory strategy
  splits them; such a run fails with `ANZ013`.
- **Scaffold needs `out_dir`** set to the same directory as `out:` (relative to
  where buf runs). Existing stubs are looked up there and never overwritten;
  without `out_dir` scaffold refuses to run.
- Every request file needs a `go_package` (or buf managed mode), whatever the
  target language: output paths follow protogen's rules, so use
  `paths=source_relative`.
- `py_framework_package=<pkg>` (python) imports the framework protos from an
  installed package instead of relative modules.

Validation is language independent and runs before any emitter; codegen,
scaffold and `lint` share it. Errors block generation, warnings do not:

| code | severity | meaning |
|---|---|---|
| ANZ002 / ANZ005 | error | `(command)` / `(event)` names an unknown component |
| ANZ003 | error | a command targets a non-aggregate |
| ANZ004 / ANZ007 | error | `emits` / `compensates` is not a fully-qualified message in the request |
| ANZ006 | error | a process-manager trigger has no `(event).domain` |
| ANZ008 | error | a required component field is missing (`domain` for aggregates/PMs; saga source and targets) |
| ANZ009 | error | a message carries angzarr option bytes that no `options.proto` in the request defines |
| ANZ010 | error | two components share a generated name |
| ANZ011 | error | one component generates the same method twice (in any language's casing) |
| ANZ012 | error | a generated type (stub, `<Name>Handler`, …) equals a proto type in the package |
| ANZ013 | error | a command/event is generated without its component's anchor (split run) |
| ANZ015 | error | an `undoes` entry is not a fully-qualified command the aggregate handles |
| ANZ014 | error | a domain field the kind must leave empty is set (e.g. `input_domain` on an aggregate, `domain` on a saga) |
| ANZ100–103 | warning | incoherent wiring: unfolded emits, dangling domains, empty components |

The option extensions are read dynamically (by extension number) from the
request's own descriptors; this module ships no compiled angzarr protos.

## Adding a language

Implement `codegen.Emitter` (`Lang`, `WiringPath`, `EmitComponent`,
`ScaffoldPath`, `EmitScaffoldComponent`; see `codegen/generate.go`) and
register it in the `emitters` table; the `codegen` and `scaffold`
subcommands appear automatically. Generated code must be a thin table
population over that language's router binding — dispatch logic lives in the
binding, never in generated code.

`just smoke <proto-root> <out> [codegen|scaffold] [all|directory]` runs every
emitter over a proto tree; `just lint-proto [proto-root]` lints one.

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
