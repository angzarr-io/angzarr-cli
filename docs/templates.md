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
| `github.com/<org>/<repo>@<rev>` | a git repository at a commit or tag, fetched with `git fetch --depth 1 https://github.com/<org>/<repo>.git <rev>` |
| `file:///<abs path>@<rev>` | a local git repository at a revision |
| `<path>` (absolute, `./`, `../`, `~/`, or no `@`) | a directory on disk — for developing templates |

The set is the directory holding `manifest.yaml`: either the value itself or its
`codegen/` subdirectory (the client-repo convention).

Fetched sets are cached under `$ANGZARR_TEMPLATE_CACHE`, else
`<user cache dir>/angzarr/templates` (`~/.cache/angzarr/templates` on Linux),
one entry per repository + revision. An entry is reused as is, never refreshed:
pin a commit SHA or an immutable tag, not a branch. Delete the entry to force a
re-fetch. Each entry records the fetched commit in `.angzarr-template-commit`.

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
      - templates=github.com/angzarr-io/angzarr-client-python@<commit>
    strategy: all
  - local: ["angzarr", "scaffold", "python"]
    out: src
    opt:
      - paths=source_relative
      - out_dir=src
      - templates=github.com/angzarr-io/angzarr-client-python@<commit>
    strategy: all
```

Output paths are rendered by the template set from the model (relative to the
plugin's `out:`); protogen's `paths=` option does not apply to them.

`angzarr codegen languages` lists every language the CLI generates; a language
with no built-in emitter requires `templates=`.

## The component model (schema_version 1)

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
is removed or its meaning changes; added fields keep the version. A template set
declares the version it reads and the CLI refuses a mismatch.

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
| `options` | {string: string} | the file options set: `go_package`, `java_package`, `java_outer_classname`, `java_multiple_files` (`"true"`/`"false"`), `csharp_namespace`, `objc_class_prefix`, `php_namespace`, `ruby_package`, `swift_prefix` |

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

| helper | result |
|---|---|
| `snake s` / `camel s` / `pascal s` | `OrderCreated` → `order_created` / `orderCreated` / `OrderCreated` (`HTTPGet` → `http_get`; `pascal`/`camel` also accept snake_case) |
| `upper s` / `lower s` | case |
| `quote s` | a double-quoted string literal escaping `"` `\` newline, carriage return, tab (valid in C-family languages, Python, TypeScript) |
| `join sep list` / `split sep s` | join / split |
| `replace old new s`, `trimPrefix p s`, `trimSuffix x s` | string edits |
| `hasPrefix p s`, `hasSuffix x s`, `contains sub s` | string tests |
| `repeat n s` | `s` repeated `n` times (`n` ≤ 0: empty) |
| `list a b …`, `append list a …` | a new list |
| `first list`, `last list`, `initial list` (all but last), `rest list` (all but first) | list access |
| `sortStrings list`, `has list v`, `commonPrefix a b` (count of equal leading elements) | list utilities |
| `add a b`, `sub a b` | integer arithmetic |
| `dict k v …`, `get m k`, `set m k v` (mutates, renders nothing), `hasKey m k` | maps |
| `typeRef message` | the manifest's `types.message` hook for a MessageRef |
| `include name data` | a `{{define}}`d template's output as a string |
| `fail msg` | stop rendering with an error |

Plus the `text/template` builtins (`index`, `slice`, `len`, `printf`, `eq`,
`and`, `or`, `not`, …).

## Where templates are tested

A client repository's CI renders its own templates with the CLI over the
router's conformance protos and runs the result against its runtime binding,
so a template change is validated in the repository that owns it. The CLI's
tests cover the contract itself with a fixture set
(`codegen/testdata/tmplset`).
