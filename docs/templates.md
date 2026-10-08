# Client-repo templates: the component model and the template contract

A language's generated code is produced from templates that live in that
language's client repository (`angzarr-client-<lang>`), next to the runtime the
code targets. The CLI owns the language-neutral parts — option parsing,
validation (`lint`), the component model, output orchestration, the
scaffold-once rule — and renders a client repository's template set over the
model. No language-specific generation lives in the CLI for a language that has
a template set.

```
proto declarations ──(validate)──► component model (JSON, versioned)
                                           │
              client-<lang>/codegen/ ──────┤ manifest.yaml + *.tmpl
                                           ▼
                             <lang> wiring + scaffold files
```

## Using a template set

The `templates=` plugin option selects the set for `angzarr codegen <lang>` and
`angzarr scaffold <lang>`:

| value | meaning |
|---|---|
| `github.com/<org>/<repo>@<sha>` | a git repository at a full commit SHA, fetched with `git fetch --depth 1 https://github.com/<org>/<repo>.git <sha>` |
| `file:///<abs path>@<sha>` | a local git repository at a full commit SHA |
| `<path>` (absolute, `./`, `../`, `~/`, or no `@`) | a directory on disk — for developing templates |

The set is the directory holding `manifest.yaml`: either the value itself or its
`codegen/` subdirectory (the client-repo convention).

A git source is pinned by a **full commit SHA** (40 hex digits, or 64 for a
SHA-256 repository, lower case). Tags, branches and abbreviated SHAs are
refused: they can move, and a fetched set is cached forever. The fetch fails
unless the fetched commit is the pinned one.

Fetched sets are cached under `$ANGZARR_TEMPLATE_CACHE`, else
`<user cache dir>/angzarr/templates` (`~/.cache/angzarr/templates` on Linux),
one entry per repository + commit. An entry is reused as is; delete it to force
a re-fetch. Each entry records the fetched commit in `.angzarr-template-commit`.

`param.<name>=<value>` overrides a parameter the manifest declares; an
undeclared name is refused.

```yaml
# buf.gen.yaml
version: v2
plugins:
  - local: ["angzarr", "codegen", "python"]
    out: gen
    opt:
      - paths=source_relative
      - templates=github.com/angzarr-io/angzarr-client-python@<full commit SHA>
    strategy: all
  - local: ["angzarr", "scaffold", "python"]
    out: src
    opt:
      - paths=source_relative
      - out_dir=src
      - templates=github.com/angzarr-io/angzarr-client-python@<full commit SHA>
    strategy: all
```

Output paths are rendered by the template set from the model (relative to the
plugin's `out:`); protogen's `paths=` option does not apply to them.

A run that renders a template set, the model plugin and `angzarr lint` emit no
Go of their own, so the protos need no `go_package` option (and no buf managed
mode) for them; the model reports only the options the files set. Only a
built-in emitter still requires each file's Go import path.

`angzarr codegen languages` lists every language the CLI generates; a language
with no built-in emitter requires `templates=`.

## The component model (schema_version 1)

The model is frozen at `schema_version` 1. Its machine-readable definition is
the JSON Schema [`model.v1.schema.json`](model.v1.schema.json) (draft
2020-12, every object closed): the CLI's tests validate every model it emits —
including the conformance and blackjack goldens — against it, and fail when a
schema key is missing from this page.

`angzarr codegen model` is a protoc plugin that writes the model of the request
as one file, `angzarr.model.json`:

```yaml
plugins:
  - local: ["angzarr", "codegen", "model"]
    out: model
    strategy: all
```

The same document — decoded from JSON, so templates address it by these keys —
is the data every template renders. `schema_version` changes only when a field
is removed or its meaning changes; an added field keeps the version (and lands
in the schema file, this page and the goldens in the same change). A template
set declares the version it reads and the CLI refuses a mismatch.

All lists are present (possibly empty); absent optional strings are `""`.

### Document

| key | type | meaning |
|---|---|---|
| `schema_version` | int | `1` |
| `files` | [File] | every proto file declaring a component anchor, in request order |
| `proto_files` | {path: ProtoFile} | every proto file a component message lives in |

### ProtoFile

| key | type | meaning |
|---|---|---|
| `path` | string | proto path, e.g. `io/angzarr/examples/v1/player.proto` |
| `dir` | string | its directory (`.` at the proto root) |
| `stem` | string | file name without `.proto` |
| `package` | string | proto package |
| `options` | {string: string} | the file options set (unset ones are absent): `go_package`, `java_package`, `java_outer_classname`, `java_multiple_files` (`"true"`/`"false"`), `csharp_namespace`, `objc_class_prefix`, `php_namespace`, `ruby_package`, `swift_prefix` |
| `top_level_messages` | [string] | message names declared at the file's top level, in declaration order |
| `top_level_enums` | [string] | enum names declared at the file's top level, in declaration order |
| `top_level_services` | [string] | service names declared in the file, in declaration order |

The `top_level_*` lists let a template apply a language's collision rule for a
type or module named after the file (Java's outer class gains `OuterClass` when
a top-level declaration has its name).

### File

A ProtoFile plus `components`: [Component] in declaration order.

### Component

| key | type | meaning |
|---|---|---|
| `kind` | string | `AGGREGATE`, `SAGA`, `PROCESS_MANAGER`, `PROJECTOR` |
| `name` | string | handler/dispatch base name and runtime component name: `(component).name`, else the anchor message name |
| `stub_name` | string | scaffold type name: `(component).name`, else `<Anchor>Impl` |
| `anchor` | MessageRef | the message carrying `(component)` |
| `state` | MessageRef \| null | the event-sourced state (the anchor); `null` for a saga |
| `domain` | string | the stream an aggregate / process manager owns |
| `input_domain` | string | a saga's source domain; a projector's declared filter |
| `output_domains` | [string] | command targets: `output_domain` then `output_domains`, deduplicated |
| `projector_domains` | [string] | a projector's filter: sorted union of `input_domain` and its handlers' source domains; empty = every domain (and for other kinds) |
| `emits_facts` | [string] | fact types a saga / process manager injects |
| `handlers` | [Handler] | command handlers (aggregate) or trigger-event handlers |
| `appliers` | [Applier] | events folded into the state (aggregate, process manager) |
| `rejections` | [Rejection] | declared compensations |
| `undos` | [Undo] | declared undo handlers (aggregate) |
| `facts` | [Fact] | declared fact handlers (aggregate) |
| `finish_method` | string | the projector's finish method base name (`Finish`); `""` otherwise |
| `referenced_files` | [string] | sorted distinct proto paths of every message the generated code names: state, handler messages, emitted events, appliers, facts |

Method names (`method`, `finish_method`) are PascalCase base names; templates
apply their language's casing (`snake`, `camel`, …). `ANZ011` already refuses
declarations whose methods collide in any supported casing.

| type | keys |
|---|---|
| MessageRef | `full_name`, `name` (innermost), `nested_names` (outermost first), `package`, `file` (a key of `proto_files`) |
| Handler | `message`, `method`, `source_domain` (trigger source; `""` on commands), `emits` [MessageRef], `typed_emit` (exactly one emitted type: the handler returns that type's list) |
| Applier | `message`, `method` (`Apply<Event>`) |
| Rejection | `key` (the declared entry, `fq.Type` or `domain:fq.Type`, registered verbatim), `command`, `domain` (`""` = any), `method` |
| Undo | `command`, `method` |
| Fact | `message`, `method` (`On<Event>Fact`) |

## The template contract

### manifest.yaml

```yaml
schema_version: 1            # the model schema the templates read
language: python             # must equal the codegen/scaffold subcommand
params:                      # parameters and their defaults (param.<name>= overrides)
  runtime_module: angzarr_client.router
imports:                     # per-component import aliases
  alias_prefix: "_"
  reserved_aliases: [_az, _t]
types:                       # type mapping hooks (template strings)
  message: '{{ .import.alias }}.{{ join "." .message.nested_names }}'
outputs:                     # one rendered file per matching component
  - mode: codegen            # codegen (every run) | scaffold (once, never overwritten)
    kinds: [AGGREGATE]       # optional filter; omitted = every kind
    path: '{{ .file.dir }}/{{ snake .component.name }}_angzarr.py'
    template: wiring.py.tmpl
```

Unknown keys are refused. Every `*.tmpl` file in the manifest's directory is
parsed into one Go [`text/template`](https://pkg.go.dev/text/template) set, so a
file of shared `{{define}}` blocks is visible to every output template. Missing
map keys are errors (`missingkey=error`).

**Imports.** For each component the CLI builds `imports`: one entry per
`referenced_files` path, in that (sorted) order, holding the ProtoFile keys plus
`alias` = `alias_prefix` + stem. An alias that is reserved or already taken gets
the entry's index appended (`_t` → `_t1`).

**Type hooks.** `types.message` renders a MessageRef as a language expression,
with data `{message, import, file}` (the message, its import entry, its
ProtoFile). The `typeRef` helper invokes it; a message whose file is not in the
component's imports is an error.

**Outputs.** `path` and the template are rendered with the render data below.
Paths are cleaned and must stay inside the output directory; two outputs
rendering one path is an error. For `scaffold`, an output whose path already
exists under the plugin's `out_dir` is skipped before its content renders.

### Render data

| key | value |
|---|---|
| `.schema_version` | the model schema version |
| `.language` | the manifest language |
| `.mode` | `codegen` or `scaffold` |
| `.params` | the resolved parameters |
| `.file` | the File declaring the component |
| `.component` | the Component |
| `.imports` | the component's import entries |
| `.proto_files` | the document's `proto_files` |

### Helpers

The helper set is language-neutral: casing, string, list, map and path
utilities plus the model hooks. Anything that encodes one language's rules —
type mapping, keyword escaping, import syntax, package/namespace derivation,
file layout — lives in that language's templates and manifest, built from
these helpers.

**Casing** (identifiers in the model are PascalCase base names)

| helper | result |
|---|---|
| `snake s` | `OrderCreated` → `order_created`; an acronym run ends before a lower-case letter (`HTTPGet` → `http_get`) |
| `pascal s` | `order_created` → `OrderCreated`; PascalCase input unchanged |
| `camel s` | `pascal` with the first letter lower-cased (`OrderCreated` → `orderCreated`) |
| `protocPascal s` | protoc's rule for names derived from file names and package segments: a letter after `_`, a digit or any other non-alphanumeric is upper-cased, non-alphanumerics dropped (`buy_in` → `BuyIn`, `foo2bar` → `Foo2Bar`, `buy-in.v1` → `BuyInV1`) |
| `lowerFirst s` / `upperFirst s` | the first letter lower- / upper-cased, the rest untouched |
| `upper s` / `lower s` | whole-string case |
| `identifier s` | every rune other than a letter, digit or `_` becomes `_`; a leading digit (or an empty result) gets a `_` prefix (`examples-v1` → `examples_v1`, `2fa` → `_2fa`); keywords are the template's to escape |

**Strings**

| helper | result |
|---|---|
| `quote s` | a double-quoted literal escaping `"` `\` newline, carriage return, tab (valid in C-family languages, Java, C#, Go, Rust and Python for the identifier-like strings the model holds) |
| `join sep list` / `split sep s` | join / split |
| `replace old new s`, `trimPrefix p s`, `trimSuffix x s`, `trim s` (surrounding whitespace) | edits |
| `hasPrefix p s`, `hasSuffix x s`, `contains sub s` | tests |
| `repeat n s` | `s` repeated `n` times (`n` ≤ 0: empty) |
| `indent n s` | every line holding a non-space character prefixed with `n` spaces (`n` ≤ 0: unchanged); blank lines stay blank — for nesting an `include`d block |

**Lists**

| helper | result |
|---|---|
| `list a b …`, `append list a …` | a new list |
| `first list`, `last list`, `initial list` (all but last), `rest list` (all but first) | access (`first`/`last` of an empty list are errors) |
| `sortStrings list`, `uniq list` (first occurrences, in order) | ordering |
| `has list v`, `commonPrefix a b` (count of equal leading elements) | tests |

**Maps and arithmetic**

| helper | result |
|---|---|
| `dict k v …`, `get m k`, `set m k v` (mutates, renders nothing), `hasKey m k` | maps — `set` on a `dict` accumulates state across a `range` (e.g. imports already emitted) |
| `add a b`, `sub a b` | integers |

**Paths** (slash-separated: proto paths and output paths)

| helper | result |
|---|---|
| `pathJoin a b …` | joined and cleaned (`pathJoin "." "a" "b.go"` → `a/b.go`) |
| `pathDir p` / `pathBase p` | directory (`.` for none) / last element |

**Model hooks and control**

| helper | result |
|---|---|
| `typeRef message` | the manifest's `types.message` hook for a MessageRef |
| `include name data` | a `{{define}}`d template's output as a string |
| `fail msg` | stop rendering with an error |

Plus the `text/template` builtins (`index`, `slice`, `len`, `printf`, `eq`,
`and`, `or`, `not`, …).

## Porting a language: what the templates own

The helper set was reviewed against everything the CLI's built-in Go, Java,
C#, C++ and (scaffold-only) Rust generation needs, so a port is template work
only. What each concern maps to:

| concern | how a template set expresses it |
|---|---|
| message type names | `types.message` hook over `nested_names`, `package`, the import entry and `.file.options`: Go `alias.Outer_Inner` (or unqualified when the import's Go package is the file's own), Java `java_package` + outer class + `join "." nested_names`, C# namespace + `join ".Types." nested_names`, C++ `replace "." "::" package` + `join "::" nested_names`, Rust module path from `snake` of each package segment |
| Java outer class | `java_outer_classname`, else `protocPascal .stem`, plus `OuterClass` when `has` of the file's `top_level_messages` / `top_level_enums` / `top_level_services` holds that name; skipped when `java_multiple_files` is `"true"` |
| C# namespace | `csharp_namespace`, else each `split "." package` segment through `protocPascal`, joined with `.` |
| Go package and imports | import path and name from `options.go_package` (`path;name`, else the last path element through `identifier`); one import per distinct Go package — `uniq`, or a `dict` of seen paths — since several proto files share a package |
| C++ includes | each import's `trimSuffix ".proto" .path` + `.pb.h` |
| method names | the model's PascalCase `method` as is (Go, C#, C++), through `lowerFirst` (Java) or `snake` (Python, Rust) |
| string literals | `quote` |
| file layout | output `path` templates over `.file.dir`, `.file.stem`, the component name and options (e.g. a Java package directory from `replace "." "/" java_package`) |
| keyword escaping, reserved import aliases | `has (list …) $name` in the template; `imports.reserved_aliases` in the manifest |

Known edge cases left to templates by design: protoc-gen-go's `GoCamelCase`
for message names containing `_` or digits before lower-case letters, and
`heck`-style Rust casing of names with digits. The fixtures (conformance and
blackjack) contain no such names.

## Versioning

- **The CLI pins nothing language-specific.** It carries no default template
  source; every `codegen`/`scaffold` run for a templated language names its
  set with `templates=`.
- **Consumers pin template sources by full commit SHA.** An examples repo's
  `buf.gen.yaml` (or the just recipe writing it) names
  `templates=github.com/angzarr-io/angzarr-client-<lang>@<sha>`; bumping the
  SHA is a deliberate commit that regenerates the wiring in the same change.
- **A client repo renders its own templates from its working tree**
  (`templates=<path>/codegen`), so a template change is tested with the code it
  targets. Its CI pins the CLI version it renders with.
- **The model schema version is the compatibility line.** A template set
  declares the `schema_version` it reads; adding model fields keeps the version,
  so existing pinned sets keep rendering. Removing or redefining a field bumps
  the version, and every client repo's manifest moves with it.

## Where templates are tested

A client repository's CI renders its own templates with the CLI over the
router's conformance protos and runs the result against its runtime binding,
so a template change is validated in the repository that owns it.

The CLI tests the contract:

- `just test` covers the manifest, render and helper rules with a fixture set
  (`codegen/testdata/tmplset`) and validates emitted models, and the committed
  golden models, against `model.v1.schema.json`.
- `just golden` renders every pinned client-repo template set
  (`codegen/testdata/golden/templates.pins`: `<lang> <repository> <full SHA>`)
  over the conformance protos (angzarr-router at the justfile's `ROUTER_REV`)
  and the blackjack protos (the angzarr-project submodule), codegen and
  scaffold, plus the model itself, and diffs the result against
  `codegen/testdata/golden/<suite>/`. CI runs it. A change to the model, a
  helper, or a pin shows up as a golden diff; review it and commit the output
  of `just golden-update`. Adding a language is one pins line plus its
  goldens.
