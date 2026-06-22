# Authoring a component: the developer experience

This is the end-to-end workflow for building an angzarr component (aggregate,
saga, process manager, projector) with `angzarr codegen`. You annotate your
domain messages in proto, run the generator, and implement a small strict
interface over your own typed messages. You never write dispatch tables,
Any-packing, rebuilders, or gRPC — the generator emits all of that against the
language binding, and the wire envelope stays `google.protobuf.Any` end to end.

The five steps:

1. [Declare components by annotating messages](#1-declare-components-in-proto)
2. [Wire the generator into buf](#2-wire-the-generator)
3. [Read what gets generated](#3-what-gets-generated)
4. [Implement the handler interface](#4-implement-the-handler)
5. [Register and run](#5-register-and-run)

Then: [regeneration & ownership](#regeneration--ownership),
[conventions](#conventions-cheat-sheet), and [gotchas](#gotchas).

---

## 1. Declare components in proto

Components are declared by **annotating messages** — there are no services and
no rpcs. The annotations *are* the declaration:

- `(io.angzarr.v1.component)` on the **anchor** message — the event-sourced
  state message for an aggregate / process manager / projector, or an empty
  marker message for the stateless saga.
- `(io.angzarr.v1.command)` on a **command** message — points at the component
  that handles it, and optionally lists the events it emits.
- `repeated (io.angzarr.v1.event)` on an **event** message — **one entry per
  consuming component** (an event folded into its aggregate *and* triggering a
  saga carries two entries).
- Compensation is declared on the **compensator** via
  `(component).compensates` — a list of fully-qualified command types whose
  rejection this component reacts to.

A minimal aggregate:

```proto
syntax = "proto3";
package shop.orders;

import "io/angzarr/v1/options.proto";

// command
message PlaceOrder {
  option (io.angzarr.v1.command) = {
    component: "shop.orders.OrderState"
    emits: "shop.orders.OrderPlaced"
  };
  string sku = 1;
  int32 quantity = 2;
}

// event — folded into the aggregate's own state
message OrderPlaced {
  option (io.angzarr.v1.event) = { component: "shop.orders.OrderState" };
  string sku = 1;
  int32 quantity = 2;
}

// anchor: the event-sourced state, and the component declaration
message OrderState {
  option (io.angzarr.v1.component) = {
    kind: COMPONENT_KIND_AGGREGATE
    input_domain: "orders"
    name: "OrderAggregate"   // generated base name; defaults to the message name
  };
  string sku = 1;
  int32 quantity = 2;
}
```

### Annotation reference

`ComponentOptions` (on the anchor message):

| field | meaning |
|---|---|
| `kind` | `COMPONENT_KIND_{AGGREGATE,SAGA,PROCESS_MANAGER,PROJECTOR}` |
| `input_domain` | domain whose events this consumes (aggregate's own domain; saga/projector filter) |
| `output_domain` | domain it issues commands to (saga); the PM's own domain |
| `name` | generated handler/dispatch base name; **defaults to the anchor message name** |
| `compensates` | repeated FQ command types whose rejection this component compensates |

`CommandOptions` (on a command message): `component` (anchor FQ), `emits`
(repeated FQ event types — see [typed emit](#typed-emit-vs-the-escape-hatch)).

`EventConsumer` — `repeated (event)` on an event message: `component` (the
consuming anchor FQ), `domain` (source-domain filter for a saga / projector /
PM trigger), `applies` (PM-only: `true` folds the PM's own state, `false` is a
cross-domain trigger reaction).

Required fields by kind: aggregate → `input_domain`; saga → `input_domain` +
`output_domain`; process manager → `output_domain` (+ every trigger event entry
needs `domain`); projector → `input_domain`. Generation **fails** (it does not
emit silently-broken wiring) on a missing required field or an unresolvable /
short type reference.

---

## 2. Wire the generator

`angzarr codegen <lang>` and `angzarr scaffold <lang>` each speak the protoc
plugin contract on stdin/stdout, so `buf` invokes them as local plugins. Two
distinct outputs, two distinct lifecycles:

```yaml
# buf.gen.yaml
version: v2
managed:
  enabled: true                       # the plugin needs a go_package on every
  override:                           # request file, even when emitting Python
    - file_option: go_package_prefix
      value: example.com/myapp/gen
plugins:
  # types
  - local: protoc-gen-go
    out: gen
    opt: paths=source_relative
  # dispatch wiring — regenerated every run (gitignore it)
  - local: ["angzarr", "codegen", "go"]
    out: gen
    opt: paths=source_relative
  # handler stub — generated ONCE into your source tree, then yours
  - local: ["angzarr", "scaffold", "go"]
    out: .
    opt: paths=source_relative
```

Then `buf generate`. (`angzarr codegen languages` lists the supported targets —
currently `go` and `python`.)

- The **wiring** plugin emits `<proto>_angzarr.pb.go` (Go) /
  `<proto>_angzarr.py` (Python). Treat it like any generated code: gitignore it,
  regenerate on demand, never edit.
- The **scaffold** plugin emits `<proto>_angzarr_handler.go` / `..._handler.py`
  **only when the file does not already exist** — so it bootstraps your impl
  once and then leaves it alone. Run it with `out: .` /
  `paths=source_relative` so its existence check resolves against your source
  tree. The scaffold is optional; you can also hand-write the impl.

### Shared framework protos: keep `go_package` native (Python)

The `go_package_prefix` override above is correct for **your own** protos. But
if you also generate the **shared angzarr framework** protos (`io/angzarr/v1/*`,
`sererr/*`) — which you do once the shared client package is no longer a
dependency — managed mode must NOT rewrite their `go_package`.

In Python, every party registers each `.proto` into the process-global
`descriptor_pool.Default()`, keyed by file name; a file may appear once,
*identically* (upb deduplicates byte-identical copies, but raises
`duplicate file name` on any divergence). `go_package` is embedded in each
pb2's serialized FileDescriptorProto, so a consumer-specific override makes your
framework descriptors diverge from every other party (the router binding,
another app) and collide in one process. The shared framework protos already
declare a native `go_package`; let it pass through so every generator emits
byte-identical descriptors:

```yaml
managed:
  enabled: true
  disable:
    - file_option: go_package
      path: google
    # Shared framework contract: native go_package → byte-identical across
    # every generator → coexists in descriptor_pool.Default().
    - file_option: go_package
      path: io/angzarr/v1
    - file_option: go_package
      path: sererr
  override:
    - file_option: go_package_prefix
      value: example.com/myapp/gen   # applies to YOUR protos only
```

Generate the shared contract through one pinned `protocolbuffers/python` plugin
version everywhere so the bytes match. (This does not arise in Go: a Go binary
links a single copy of the contract.)

---

## 3. What gets generated

For `OrderState` above, the **wiring** file gives you a strict interface, a
dispatch constructor, and a one-call registration:

```go
// OrderAggregateHandler is the strict business seam for the OrderAggregate aggregate.
// Every declared command/event must be implemented — a missing handler is a
// compile error, never a silent no-op.
type OrderAggregateHandler interface {
    PlaceOrder(cmd *PlaceOrder, state *OrderState, cctx ffirouter.CommandContext) ([]*OrderPlaced, error)
    ApplyOrderPlaced(state *OrderState, event *OrderPlaced)
}

func NewOrderAggregateDispatch(h OrderAggregateHandler) *ffirouter.AggregateDispatch[*OrderState] { … }
func RegisterOrderAggregate(r *ffirouter.Router, h OrderAggregateHandler) error { … }
```

The constructor wires the Any-unmarshal thunks, the rebuilder (appliers +
snapshot loader), and — because `PlaceOrder` declares `emits` — packs the
returned `[]*OrderPlaced` into the EventBook for you.

The **scaffold** file is a starting implementation you own:

```go
// Scaffolded ONCE by angzarr codegen go — this file is YOURS.
// Regeneration will NOT overwrite this file. It is your responsibility to keep
// the generated <Component>Handler interface implemented…

type OrderAggregate struct{}

var _ OrderAggregateHandler = OrderAggregate{}  // build breaks here if you fall behind the proto

func (OrderAggregate) PlaceOrder(cmd *PlaceOrder, state *OrderState, cctx ffirouter.CommandContext) ([]*OrderPlaced, error) {
    panic("TODO: implement OrderAggregate.PlaceOrder")
}

func (OrderAggregate) ApplyOrderPlaced(state *OrderState, event *OrderPlaced) {
    // TODO: implement OrderAggregate.ApplyOrderPlaced
}
```

The `var _ Handler = Impl{}` line is load-bearing: when you add a command or
event to the proto and regenerate the wiring, the **interface grows**, and this
assertion fails the build until you add the matching method. That's how the
generated wiring (overwritten) and your owned impl (not overwritten) stay in
sync without the generator ever touching your code.

---

## 4. Implement the handler

You write only typed methods over your own messages. For the aggregate:

```go
type OrderAggregate struct{}

// command handler: typed command in, typed events out (the wiring builds the EventBook)
func (OrderAggregate) PlaceOrder(cmd *PlaceOrder, state *OrderState, cctx ffirouter.CommandContext) ([]*OrderPlaced, error) {
    if cmd.Quantity <= 0 {
        return nil, ffirouter.Reject("QUANTITY_NOT_POSITIVE", "quantity must be positive")
    }
    return []*OrderPlaced{{Sku: cmd.Sku, Quantity: cmd.Quantity}}, nil
}

// applier: fold the event into rebuilt state (no return)
func (OrderAggregate) ApplyOrderPlaced(state *OrderState, event *OrderPlaced) {
    state.Sku = event.Sku
    state.Quantity = event.Quantity
}
```

The same component in Python implements the generated `Protocol`:

```python
class OrderAggregate:
    def place_order(self, cmd, state, cctx):
        if cmd.quantity <= 0:
            raise reject("QUANTITY_NOT_POSITIVE", "quantity must be positive")
        return [order_pb2.OrderPlaced(sku=cmd.sku, quantity=cmd.quantity)]

    def apply_order_placed(self, state, event):
        state.sku = event.sku
        state.quantity = event.quantity
```

`state` is your own state message, reconstructed by the framework from prior
events (and snapshots) before the command runs — host state never crosses the
wire. `cctx` carries the historical-state evidence (`NextSequence`,
`HadPriorEvents`). To reject a command, return/raise a coded error
(`ffirouter.Reject` / `reject`); any other error becomes
`UNHANDLED_HANDLER_ERROR`.

The other kinds follow the same shape, with kind-appropriate signatures:

- **saga** — `Increased(event, dests) ([]*CommandBook, []*EventBook, error)`:
  translate a source event into commands (stamp them from `dests`) and/or
  injected facts.
- **process manager** — a trigger handler
  `Increased(event, state, dests) (*ProcessManagerHandleResponse, error)` plus
  appliers folding its own state.
- **projector** — `Increased(projection, event) error` folds, and a generated
  `Finish(projection, events) (*Projection, error)` packs the result.

---

## 5. Register and run

`Register<Component>` hides the per-kind registration split (free function vs.
method) behind one call:

```go
router := ffirouter.NewRouter()
defer router.Close()

if err := RegisterOrderAggregate(router, OrderAggregate{}); err != nil {
    log.Fatal(err)
}
// router.Dispatch(...) now routes orders-domain commands through your handler.
```

```python
router = Router()
register_order_aggregate(router, OrderAggregate())
```

That's the whole loop. Transport, persistence, and routing are the framework's;
your code is the typed methods plus one registration call.

---

## Regeneration & ownership

| file | lifecycle |
|---|---|
| `*_pb2` / `*.pb.go` (types) | generated, overwritten, gitignored |
| `*_angzarr.*` (wiring) | generated, **overwritten every run**, gitignored |
| `*_angzarr_handler.*` (scaffold) | generated **once**, then **yours** — never overwritten, committed |

Adding a command/event to the proto → regenerate → the interface grows → the
`var _` assertion fails your build until you implement the new method. Removing
one → the method becomes unused (harmless) or, if the type is gone, a compile
error you clean up. You are never asked to merge into generated code.

---

## Conventions cheat-sheet

**Generated method names** (from your message names):

| member | method name | example |
|---|---|---|
| command handler | the command message name | `PlaceOrder` |
| event handler (saga / projector / PM trigger) | the event message name | `OrderPlaced` |
| applier (aggregate / PM own-state fold) | `Apply` + event name | `ApplyOrderPlaced` |
| compensator | `On` + short command + `Rejected` | `OnReserveRejected` |

The `Apply` prefix keeps an applier distinct from a handler for the *same*
event — a process manager can both fold an event into its state and react to it.

### Typed emit vs. the escape hatch

If a command declares **exactly one** `emits` type, its handler returns
`[]*ThatEvent` and the wiring builds the EventBook and lets the core stamp
sequences. If a command declares **no** `emits`, the handler returns the raw
`*EventBook` — the escape hatch for multi-type emission, custom covers, or
snapshots.

### Snapshots are automatic

A snapshot carries the component's own state message, so the generated
rebuilder restores it generically — you implement no snapshot method.

### Multiple subscribers = multiple components

There is one handler per (message, component). To have several reactions to one
event, declare several components and give the event one `(event)` entry per
consumer; the framework fans out to each. (Within a single component there is
one handler per type — model extra reactions as extra components.)

---

## Gotchas

- **Name your component distinctly from its state message.** The generated
  interface is `<name>Handler` and the scaffold struct is `<name>`. If `name`
  defaults to the state message name (e.g. anchor `OrderState`, no `name`), the
  scaffold struct `OrderState` collides with the generated `OrderState` type.
  Set `name` to the component (e.g. `OrderAggregate`) — distinct from the state
  message.
- **Fully-qualified type references only.** `component`, `emits`, and
  `compensates` must name fully-qualified message types present in the compiled
  set; short names never match dispatch, and generation fails on them rather
  than emitting wiring that silently never fires.
- **The wiring depends on the binding.** Generated `*_angzarr.*` imports the
  language binding (`ffirouter` / `angzarr_router_ffi`). Don't have the binding
  package consume its *own* generated wiring from inside its own package — put
  consumers in a separate package (a normal dependency direction) to avoid an
  import cycle.
