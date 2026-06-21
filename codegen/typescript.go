package codegen

// TypeScript emitter — the structural mirror of the C#/C++ emitters over the
// shared intermediate model, targeting the protobuf-es (@bufbuild/protobuf v2)
// runtime and the koffi-based angzarr-router binding. For each declared
// component it emits two files:
//
//   - the WIRING file (*_angzarr.ts): a strict-seam interface
//     <Component>Handler (one method per declared command/event), a
//     new<Component>Dispatch builder populating the binding's dispatch table
//     with unmarshal thunks that call the typed methods, and a
//     register<Component> convenience. Regenerated wholesale every run.
//
//   - the SCAFFOLD file (*_angzarr_handler.ts): a <Component> class
//     implementing the interface with one TODO method per command/event,
//     generated ONCE and then owned by the developer.
//
// The dispatch surfaces are generic in the component state message, so the
// generated wiring is cast-free. Runtime types and the framework message types
// the seam references come from the binding package "@angzarr/router"; domain
// message classes and their protobuf-es schemas are imported from the generated
// *_pb modules (protoc-gen-es). A command declaring exactly one emitted event
// type returns that typed array and the wiring builds the EventBook via
// Pack.eventBook/Pack.wrap; otherwise it returns the raw EventBook escape hatch.

import (
	"path"
	"sort"
	"strings"

	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/reflect/protoreflect"
)

// The binding package and the protobuf-es runtime module the wiring imports.
const (
	tsRuntimeModule = "@angzarr/router"
	tsProtobufES    = "@bufbuild/protobuf"
)

// Runtime identifiers (exported by @angzarr/router): dispatch + builder types,
// the framework message types the seam references, and the Pack/parseAny
// helpers. Tracked per file so the import line lists only what is used.
const (
	tsRouter       = "Router"
	tsAggDispatch  = "AggregateDispatch"
	tsSagaDispatch = "SagaDispatch"
	tsProjDispatch = "ProjectorDispatch"
	tsPmDispatch   = "ProcessManagerDispatch"
	tsRebuilder    = "Rebuilder"
	tsCctx         = "CommandContext"
	tsDestinations = "Destinations"
	tsPack         = "Pack"
	tsParseAny     = "parseAny"
	tsSagaEmission = "SagaEmission"
	tsPmRejection  = "PmRejection"

	tsEventBook    = "EventBook"
	tsCommandBook  = "CommandBook"
	tsNotification = "Notification"
	tsRejNotif     = "RejectionNotification"
	tsBusinessResp = "BusinessResponse"
	tsProjection   = "Projection"
	tsPmResponse   = "ProcessManagerHandleResponse"
)

type tsEmitter struct{}

func (tsEmitter) Lang() string           { return "typescript" }
func (tsEmitter) Suffix() string         { return "_angzarr.ts" }
func (tsEmitter) ScaffoldSuffix() string { return "_angzarr_handler.ts" }

// tsRefs tracks the imports one generated file needs: the runtime identifiers
// from @angzarr/router, whether `create` is needed from protobuf-es, and the
// domain message types/schemas grouped by their generated *_pb module.
type tsRefs struct {
	fromProto  string                         // the proto path of the file being generated
	runtime    map[string]struct{}            // identifiers imported from @angzarr/router
	needCreate bool                           // `create` imported from @bufbuild/protobuf
	modules    map[string]map[string]struct{} // import path -> set of {Type, TypeSchema}
}

func newTSRefs(file *protogen.File) *tsRefs {
	return &tsRefs{
		fromProto: file.Desc.Path(),
		runtime:   map[string]struct{}{},
		modules:   map[string]map[string]struct{}{},
	}
}

func (r *tsRefs) use(id string) string {
	r.runtime[id] = struct{}{}
	return id
}

// ref records that the wiring references a domain message and returns its TS
// type name; the matching schema const is recorded alongside.
func (r *tsRefs) ref(m *protogen.Message) string {
	imp := tsImportPath(r.fromProto, m.Desc.ParentFile().Path())
	set, ok := r.modules[imp]
	if !ok {
		set = map[string]struct{}{}
		r.modules[imp] = set
	}
	name := tsName(m)
	set[name] = struct{}{}
	set[name+"Schema"] = struct{}{}
	return name
}

// schema records a domain message reference and returns its schema const name.
func (r *tsRefs) schema(m *protogen.Message) string {
	r.ref(m)
	return tsName(m) + "Schema"
}

func (r *tsRefs) emitImports(g *protogen.GeneratedFile) {
	if r.needCreate {
		g.P(`import { create } from "`, tsProtobufES, `";`)
	}
	if len(r.runtime) > 0 {
		ids := make([]string, 0, len(r.runtime))
		for id := range r.runtime {
			ids = append(ids, id)
		}
		sort.Strings(ids)
		g.P(`import { `, strings.Join(ids, ", "), ` } from "`, tsRuntimeModule, `";`)
	}
	imps := make([]string, 0, len(r.modules))
	for imp := range r.modules {
		imps = append(imps, imp)
	}
	sort.Strings(imps)
	for _, imp := range imps {
		names := make([]string, 0, len(r.modules[imp]))
		for n := range r.modules[imp] {
			names = append(names, n)
		}
		sort.Strings(names)
		g.P(`import { `, strings.Join(names, ", "), ` } from "`, imp, `";`)
	}
	g.P()
}

func (e tsEmitter) EmitFile(g *protogen.GeneratedFile, file *protogen.File, services []*Service) error {
	refs := newTSRefs(file)
	// Resolve every reference first so the import block is complete before it is
	// written; the body is buffered into closures that run after imports.
	var body []func()
	for _, s := range services {
		s := s
		sigs := e.sigs(refs, s)
		body = append(body, func() { emitTSInterface(g, s, sigs) })
		switch s.Component.Kind {
		case KindAggregate:
			body = append(body, e.aggregateDispatch(g, refs, s))
		case KindSaga:
			body = append(body, e.sagaDispatch(g, refs, s))
		case KindProjector:
			body = append(body, e.projectorDispatch(g, refs, s))
		case KindProcessManager:
			body = append(body, e.pmDispatch(g, refs, s))
		}
		body = append(body, e.register(g, refs, s))
	}
	g.P("// Code generated by angzarr codegen typescript. DO NOT EDIT.")
	g.P("// source: ", file.Desc.Path())
	g.P()
	refs.emitImports(g)
	for _, fn := range body {
		fn()
	}
	return nil
}

// --- the strict seam ---------------------------------------------------------

// tsSig is one handler method's signature, shared by the interface and the
// scaffold so the two never drift.
type tsSig struct {
	name    string
	params  string
	returns string
}

func (e tsEmitter) sigs(refs *tsRefs, s *Service) []tsSig {
	switch s.Component.Kind {
	case KindAggregate:
		return e.aggregateSigs(refs, s)
	case KindSaga:
		return e.sagaSigs(refs, s)
	case KindProjector:
		return e.projectorSigs(refs, s)
	case KindProcessManager:
		return e.pmSigs(refs, s)
	}
	return nil
}

func emitTSInterface(g *protogen.GeneratedFile, s *Service, sigs []tsSig) {
	g.P("// The strict business seam for the ", s.GoName, " ", strings.ToLower(s.Component.Kind.String()), ".")
	g.P("// Every declared command/event must be implemented.")
	g.P("export interface ", s.GoName, "Handler {")
	for _, m := range sigs {
		g.P("  ", m.name, "(", m.params, "): ", m.returns, ";")
	}
	g.P("}")
	g.P()
}

func (e tsEmitter) aggregateSigs(refs *tsRefs, s *Service) []tsSig {
	state := refs.ref(s.State)
	var out []tsSig
	for _, h := range s.Handlers {
		returns := refs.use(tsEventBook)
		if h.TypedEmit() {
			returns = refs.ref(h.Emits[0]) + "[]"
		}
		out = append(out, tsSig{
			name:    lowerFirst(h.MethodName),
			params:  "cmd: " + refs.ref(h.Message) + ", state: " + state + ", cctx: " + refs.use(tsCctx),
			returns: returns,
		})
	}
	for _, a := range s.Appliers {
		out = append(out, tsSig{
			name:    lowerFirst(a.MethodName),
			params:  "state: " + state + ", ev: " + refs.ref(a.Message),
			returns: "void",
		})
	}
	for _, r := range s.Rejections {
		out = append(out, tsSig{
			name:    lowerFirst(r.MethodName),
			params:  "n: " + refs.use(tsNotification) + ", rejection: " + refs.use(tsRejNotif) + ", state: " + state + ", cctx: " + refs.use(tsCctx),
			returns: refs.use(tsBusinessResp),
		})
	}
	return out
}

func (e tsEmitter) sagaSigs(refs *tsRefs, s *Service) []tsSig {
	var out []tsSig
	for _, h := range s.Handlers {
		out = append(out, tsSig{
			name:    lowerFirst(h.MethodName),
			params:  "ev: " + refs.ref(h.Message) + ", dests: " + refs.use(tsDestinations),
			returns: refs.use(tsSagaEmission),
		})
	}
	for _, r := range s.Rejections {
		out = append(out, tsSig{
			name:    lowerFirst(r.MethodName),
			params:  "n: " + refs.use(tsNotification) + ", rejection: " + refs.use(tsRejNotif),
			returns: refs.use(tsEventBook) + "[]",
		})
	}
	return out
}

func (e tsEmitter) projectorSigs(refs *tsRefs, s *Service) []tsSig {
	state := refs.ref(s.State)
	var out []tsSig
	for _, h := range s.Handlers {
		out = append(out, tsSig{
			name:    lowerFirst(h.MethodName),
			params:  "projection: " + state + ", ev: " + refs.ref(h.Message),
			returns: "void",
		})
	}
	out = append(out, tsSig{
		name:    "finish",
		params:  "projection: " + state + ", events: " + refs.use(tsEventBook),
		returns: refs.use(tsProjection),
	})
	return out
}

func (e tsEmitter) pmSigs(refs *tsRefs, s *Service) []tsSig {
	state := refs.ref(s.State)
	var out []tsSig
	for _, h := range s.Handlers {
		out = append(out, tsSig{
			name:    lowerFirst(h.MethodName),
			params:  "ev: " + refs.ref(h.Message) + ", state: " + state + ", dests: " + refs.use(tsDestinations),
			returns: refs.use(tsPmResponse),
		})
	}
	for _, a := range s.Appliers {
		out = append(out, tsSig{
			name:    lowerFirst(a.MethodName),
			params:  "state: " + state + ", ev: " + refs.ref(a.Message),
			returns: "void",
		})
	}
	for _, r := range s.Rejections {
		out = append(out, tsSig{
			name:    lowerFirst(r.MethodName),
			params:  "n: " + refs.use(tsNotification) + ", rejection: " + refs.use(tsRejNotif) + ", state: " + state,
			returns: refs.use(tsPmRejection),
		})
	}
	return out
}

// --- dispatch builders -------------------------------------------------------

func (e tsEmitter) aggregateDispatch(g *protogen.GeneratedFile, refs *tsRefs, s *Service) func() {
	refs.needCreate = true
	state := refs.ref(s.State)
	disp := refs.use(tsAggDispatch) + "<" + state + ">"
	refs.use(tsRebuilder)
	refs.use(tsPack)
	refs.use(tsParseAny)
	return func() {
		g.P("// Populates the aggregate dispatch table from the proto declaration.")
		g.P("export function new", s.GoName, "Dispatch(h: ", s.GoName, "Handler): ", disp, " {")
		g.P("  const rebuilder = new ", tsRebuilder, "<", state, ">(() => create(", refs.schema(s.State), "));")
		g.P("  rebuilder.withSnapshot((state, payload) => ", tsPack, ".merge(", refs.schema(s.State), ", state, payload));")
		for _, a := range s.Appliers {
			g.P("  rebuilder.apply(", tsQuote(a.fqType()), ", (state, payload) => {")
			g.P("    h.", lowerFirst(a.MethodName), "(state, ", tsParseAny, "(", refs.schema(a.Message), ", payload));")
			g.P("  });")
		}
		g.P("  const dispatch = new ", tsAggDispatch, "<", state, ">(", tsQuote(s.GoName), ", ", tsQuote(s.Component.InputDomain), ", rebuilder);")
		for _, h := range s.Handlers {
			g.P("  dispatch.onCommand(", tsQuote(string(h.Message.Desc.FullName())), ", (cmdAny, state, cctx) => {")
			g.P("    const cmd = ", tsParseAny, "(", refs.schema(h.Message), ", cmdAny);")
			if h.TypedEmit() {
				g.P("    const events = h.", lowerFirst(h.MethodName), "(cmd, state, cctx);")
				g.P("    return ", tsPack, ".eventBook(events.map((ev) => ", tsPack, ".wrap(", refs.schema(h.Emits[0]), ", ev)));")
			} else {
				g.P("    return h.", lowerFirst(h.MethodName), "(cmd, state, cctx);")
			}
			g.P("  });")
		}
		for _, r := range s.Rejections {
			g.P("  dispatch.onRejected(", tsQuote(r.Command), ", (n, rejection, state, cctx) =>")
			g.P("    h.", lowerFirst(r.MethodName), "(n, rejection, state, cctx),")
			g.P("  );")
		}
		g.P("  return dispatch;")
		g.P("}")
		g.P()
	}
}

func (e tsEmitter) sagaDispatch(g *protogen.GeneratedFile, refs *tsRefs, s *Service) func() {
	disp := refs.use(tsSagaDispatch)
	refs.use(tsParseAny)
	return func() {
		g.P("// Populates the saga dispatch table from the proto declaration.")
		g.P("export function new", s.GoName, "Dispatch(h: ", s.GoName, "Handler): ", disp, " {")
		g.P("  const dispatch = new ", tsSagaDispatch, "(", tsQuote(s.GoName), ", ", tsQuote(s.Component.InputDomain), ", [", tsQuote(s.Component.OutputDomain), "]);")
		for _, h := range s.Handlers {
			g.P("  dispatch.onEvent(", tsQuote(string(h.Message.Desc.FullName())), ", (eventAny, dests) => {")
			g.P("    const ev = ", tsParseAny, "(", refs.schema(h.Message), ", eventAny);")
			g.P("    return h.", lowerFirst(h.MethodName), "(ev, dests);")
			g.P("  });")
		}
		for _, r := range s.Rejections {
			g.P("  dispatch.onRejected(", tsQuote(r.Command), ", (n, rejection) => h.", lowerFirst(r.MethodName), "(n, rejection));")
		}
		g.P("  return dispatch;")
		g.P("}")
		g.P()
	}
}

func (e tsEmitter) projectorDispatch(g *protogen.GeneratedFile, refs *tsRefs, s *Service) func() {
	refs.needCreate = true
	state := refs.ref(s.State)
	disp := refs.use(tsProjDispatch) + "<" + state + ">"
	refs.use(tsParseAny)
	return func() {
		g.P("// Populates the projector dispatch table from the proto declaration.")
		g.P("export function new", s.GoName, "Dispatch(h: ", s.GoName, "Handler): ", disp, " {")
		g.P("  const dispatch = new ", tsProjDispatch, "<", state, ">(", tsQuote(s.GoName), ", () => create(", refs.schema(s.State), "));")
		g.P("  dispatch.forDomains(", tsQuote(s.Component.InputDomain), ");")
		for _, h := range s.Handlers {
			g.P("  dispatch.onEvent(", tsQuote(string(h.Message.Desc.FullName())), ", (projection, eventAny) => {")
			g.P("    h.", lowerFirst(h.MethodName), "(projection, ", tsParseAny, "(", refs.schema(h.Message), ", eventAny));")
			g.P("  });")
		}
		g.P("  dispatch.finish((projection, events) => h.finish(projection, events));")
		g.P("  return dispatch;")
		g.P("}")
		g.P()
	}
}

func (e tsEmitter) pmDispatch(g *protogen.GeneratedFile, refs *tsRefs, s *Service) func() {
	refs.needCreate = true
	state := refs.ref(s.State)
	disp := refs.use(tsPmDispatch) + "<" + state + ">"
	refs.use(tsRebuilder)
	refs.use(tsPack)
	refs.use(tsParseAny)
	return func() {
		g.P("// Populates the process-manager dispatch table from the proto declaration.")
		g.P("export function new", s.GoName, "Dispatch(h: ", s.GoName, "Handler): ", disp, " {")
		g.P("  const rebuilder = new ", tsRebuilder, "<", state, ">(() => create(", refs.schema(s.State), "));")
		g.P("  rebuilder.withSnapshot((state, payload) => ", tsPack, ".merge(", refs.schema(s.State), ", state, payload));")
		for _, a := range s.Appliers {
			g.P("  rebuilder.apply(", tsQuote(a.fqType()), ", (state, payload) => {")
			g.P("    h.", lowerFirst(a.MethodName), "(state, ", tsParseAny, "(", refs.schema(a.Message), ", payload));")
			g.P("  });")
		}
		g.P("  const dispatch = new ", tsPmDispatch, "<", state, ">(", tsQuote(s.GoName), ", ", tsQuote(s.Component.OutputDomain), ", rebuilder);")
		for _, h := range s.Handlers {
			g.P("  dispatch.onEvent(", tsQuote(h.SourceDomain), ", ", tsQuote(string(h.Message.Desc.FullName())), ", (eventAny, state, dests) => {")
			g.P("    const ev = ", tsParseAny, "(", refs.schema(h.Message), ", eventAny);")
			g.P("    return h.", lowerFirst(h.MethodName), "(ev, state, dests);")
			g.P("  });")
		}
		for _, r := range s.Rejections {
			g.P("  dispatch.onRejected(", tsQuote(r.Command), ", (n, rejection, state) =>")
			g.P("    h.", lowerFirst(r.MethodName), "(n, rejection, state),")
			g.P("  );")
		}
		g.P("  return dispatch;")
		g.P("}")
		g.P()
	}
}

func (e tsEmitter) register(g *protogen.GeneratedFile, refs *tsRefs, s *Service) func() {
	refs.use(tsRouter)
	method := map[ComponentKind]string{
		KindAggregate:      "registerAggregate",
		KindSaga:           "registerSaga",
		KindProjector:      "registerProjector",
		KindProcessManager: "registerProcessManager",
	}[s.Component.Kind]
	return func() {
		g.P("// Registers a ", s.GoName, "Handler with the router.")
		g.P("export function register", s.GoName, "(r: ", tsRouter, ", h: ", s.GoName, "Handler): void {")
		g.P("  r.", method, "(new", s.GoName, "Dispatch(h));")
		g.P("}")
		g.P()
	}
}

// EmitScaffold writes the generate-once developer stub: one class per component
// implementing the Handler interface, with TODO bodies.
func (e tsEmitter) EmitScaffold(g *protogen.GeneratedFile, file *protogen.File, services []*Service) error {
	refs := newTSRefs(file)
	sigs := map[*Service][]tsSig{}
	for _, s := range services {
		sigs[s] = e.sigs(refs, s)
		refs.use(s.GoName + "Handler")
	}
	g.P("// Scaffolded ONCE by angzarr codegen typescript — this file is YOURS.")
	g.P("// Regeneration will NOT overwrite it; keep the generated Handler")
	g.P("// interfaces implemented as commands/events are added to the proto.")
	g.P()
	refs.emitImports(g)
	for _, s := range services {
		g.P("export class ", s.GoName, " implements ", s.GoName, "Handler {")
		for _, m := range sigs[s] {
			g.P("  ", m.name, "(", m.params, "): ", m.returns, " {")
			g.P("    throw new Error(", tsQuote("TODO: implement "+s.GoName+"."+m.name), ");")
			g.P("  }")
		}
		g.P("}")
		g.P()
	}
	return nil
}

// --- TypeScript naming helpers -----------------------------------------------

// tsName is the protoc-gen-es type name for a message: nested messages are
// flattened with "_" (Outer.Inner -> Outer_Inner).
func tsName(m *protogen.Message) string {
	parts := []string{string(m.Desc.Name())}
	md := m.Desc
	for {
		parent, ok := md.Parent().(protoreflect.MessageDescriptor)
		if !ok {
			break
		}
		parts = append([]string{string(parent.Name())}, parts...)
		md = parent
	}
	return strings.Join(parts, "_")
}

// tsImportPath is the ESM import specifier (extensionless, bundler resolution)
// for the generated *_pb module of toProto, relative to fromProto's directory.
func tsImportPath(fromProto, toProto string) string {
	base := strings.TrimSuffix(path.Base(toProto), ".proto") + "_pb"
	rel := relSlash(path.Dir(fromProto), path.Dir(toProto))
	if rel == "." {
		return "./" + base
	}
	if strings.HasPrefix(rel, "..") {
		return rel + "/" + base
	}
	return "./" + rel + "/" + base
}

// relSlash computes a slash-separated relative path from -> to (both clean,
// slash-separated dirs), returning "." when equal.
func relSlash(from, to string) string {
	if from == to {
		return "."
	}
	fromSeg := splitClean(from)
	toSeg := splitClean(to)
	i := 0
	for i < len(fromSeg) && i < len(toSeg) && fromSeg[i] == toSeg[i] {
		i++
	}
	var out []string
	for j := i; j < len(fromSeg); j++ {
		out = append(out, "..")
	}
	out = append(out, toSeg[i:]...)
	if len(out) == 0 {
		return "."
	}
	return strings.Join(out, "/")
}

func splitClean(p string) []string {
	if p == "" || p == "." {
		return nil
	}
	return strings.Split(p, "/")
}

// tsQuote renders a Go string as a double-quoted TypeScript string literal.
func tsQuote(s string) string {
	var b strings.Builder
	b.WriteByte('"')
	for _, r := range s {
		switch r {
		case '"':
			b.WriteString("\\\"")
		case '\\':
			b.WriteString("\\\\")
		default:
			b.WriteRune(r)
		}
	}
	b.WriteByte('"')
	return b.String()
}
