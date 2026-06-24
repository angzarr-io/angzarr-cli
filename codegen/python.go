package codegen

// Python emitter — the structural mirror of the Go emitter (golang.go) over
// the same intermediate model. For each declared component it emits two files:
//
//   - the WIRING file (*_angzarr.py): a typing.Protocol <Component>Handler
//     (one method per declared command/event), a new_<component>_dispatch
//     builder populating the binding's dispatch table with unmarshal thunks
//     that call the typed methods, and a register_<component> convenience.
//     Regenerated wholesale every run.
//
//   - the SCAFFOLD file (*_angzarr_handler.py): a <Component> class
//     implementing the Protocol with one TODO method per command/event,
//     generated ONCE and then owned by the developer (the plugin skips it when
//     the file exists).
//
// Generated Python carries no dispatch logic and no gRPC — it is a thin table
// population over the angzarr-router Python binding (angzarr_router_ffi), and
// the envelope stays google.protobuf.Any end to end. Message types are imported
// with RELATIVE imports (see pyRel) so the generated tree is position-independent
// — it drops into any host package without a baked-in prefix and never shadows
// stdlib (e.g. bare `io`). The buf pb2/grpc output is made relative the same way
// by protoletariat in the consumer's proto-gen step.
// A command declaring exactly one emitted event type returns that typed list
// and the wiring builds the EventBook via angzarr_router_ffi.pack; otherwise it
// returns the raw EventBook escape hatch.

import (
	"fmt"
	"path"
	"sort"
	"strings"
	"unicode"

	"google.golang.org/protobuf/compiler/protogen"
)

// Framework message modules referenced by generated Python, by proto path.
const (
	pyTypesModule  = "io.angzarr.v1.types_pb2"
	pyCmdHdrModule = "io.angzarr.v1.command_handler_pb2"
	pyPMModule     = "io.angzarr.v1.process_manager_pb2"
)

// Fixed aliases for the framework modules and the binding.
const (
	pyAz    = "_az" // angzarr_router_ffi
	pyTypes = "_t"  // io.angzarr.v1.types_pb2
	pyCmdH  = "_ch" // io.angzarr.v1.command_handler_pb2
	pyPM    = "_pm" // io.angzarr.v1.process_manager_pb2
)

// pyEmitter carries the python codegen options. frameworkPkg, when non-empty,
// is the package the runtime ships the angzarr framework protos under (e.g.
// "angzarr_router_ffi.gen"); framework imports resolve there instead of being
// regenerated/relative, so a consumer never registers a duplicate framework
// descriptor. Empty (the default) keeps framework imports relative — used when
// the framework protos ARE part of this generated tree (the runtime's own gen).
type pyEmitter struct{ frameworkPkg string }

func (pyEmitter) Lang() string { return "python" }

func (pyEmitter) WiringPath(file *protogen.File, s *Service) string {
	return componentFile(file, snake(s.GoName), "_angzarr.py")
}

func (pyEmitter) ScaffoldPath(file *protogen.File, s *Service) string {
	return componentFile(file, snake(s.GoName), "_angzarr_handler.py")
}

// pyRefs resolves message types to import aliases for one generated file.
type pyRefs struct {
	alias      map[string]string // proto file path -> module alias (_m0, _m1, …)
	order      []string          // alias declaration order
	needCmdHdr bool              // BusinessResponse referenced (aggregate rejections)
	needPM     bool              // ProcessManagerHandleResponse referenced
}

// newPyRefs assigns a stable alias to every distinct proto file a component's
// messages live in, and records which framework modules the file needs.
func newPyRefs(services []*Service) *pyRefs {
	r := &pyRefs{alias: map[string]string{}}
	paths := map[string]bool{}
	add := func(m *protogen.Message) {
		if m != nil {
			paths[m.Desc.ParentFile().Path()] = true
		}
	}
	for _, s := range services {
		add(s.State)
		for _, h := range s.Handlers {
			add(h.Message)
			for _, e := range h.Emits {
				add(e)
			}
		}
		for _, a := range s.Appliers {
			add(a.Message)
		}
		if s.Component.Kind == KindProcessManager {
			r.needPM = true
		}
		if s.Component.Kind == KindAggregate && len(s.Rejections) > 0 {
			r.needCmdHdr = true
		}
	}
	sorted := make([]string, 0, len(paths))
	for p := range paths {
		sorted = append(sorted, p)
	}
	sort.Strings(sorted)
	for i, p := range sorted {
		alias := "_m" + itoa(i)
		r.alias[p] = alias
		r.order = append(r.order, p)
	}
	return r
}

// ref is the Python expression for a message class: <module alias>.<ClassName>.
func (r *pyRefs) ref(m *protogen.Message) string {
	return r.alias[m.Desc.ParentFile().Path()] + "." + string(m.Desc.Name())
}

// pyRel builds a relative import — ("from <dots><tail>", "<module>") — for a
// dotted target module (e.g. "io.angzarr.v1.types_pb2") as seen from fromPkg,
// the importing file's dotted package (e.g. "io.angzarr.examples.v1"). Relative
// imports keep the generated tree position-independent: it drops into any host
// package without a baked-in prefix and never shadows stdlib (e.g. bare `io`).
// protoletariat rewrites the buf pb2/grpc imports the same way.
func pyRel(fromPkg, target string) (relPkg, module string) {
	i := strings.LastIndex(target, ".")
	toPkg, module := target[:i], target[i+1:]
	from := strings.Split(fromPkg, ".")
	to := strings.Split(toPkg, ".")
	c := 0
	for c < len(from) && c < len(to) && from[c] == to[c] {
		c++
	}
	return strings.Repeat(".", len(from)-c+1) + strings.Join(to[c:], "."), module
}

// pyFilePkg returns a generated file's dotted Python package: its proto
// directory with "/" → "." (e.g. "io/angzarr/examples/v1/hand" →
// "io.angzarr.examples.v1").
func pyFilePkg(file *protogen.File) string {
	return strings.ReplaceAll(path.Dir(file.GeneratedFilenamePrefix), "/", ".")
}

// pyIsFramework reports whether a proto file path holds angzarr runtime framework
// types (io.angzarr.v1.* + sererr), as opposed to a consumer's own domain protos.
// When a frameworkPkg is configured these are imported from it, not regenerated.
func pyIsFramework(protoPath string) bool {
	return strings.HasPrefix(protoPath, "io/angzarr/v1/") || strings.HasPrefix(protoPath, "sererr/")
}

// emitImports writes the import block shared by wiring and scaffold files.
// fromPkg is the importing file's dotted package (its proto directory). Example
// message modules use position-independent relative imports; framework modules
// resolve from frameworkPkg when set (external, shared with the runtime) else
// relative (the runtime's own gen, where framework IS the local tree).
func (r *pyRefs) emitImports(g *protogen.GeneratedFile, fromPkg, frameworkPkg string) {
	// fw resolves a framework module's dotted path (e.g. "io.angzarr.v1.types_pb2")
	// to (package, module): external under frameworkPkg, or relative when unset.
	fw := func(target string) (string, string) {
		if frameworkPkg != "" {
			i := strings.LastIndex(target, ".")
			return frameworkPkg + "." + target[:i], target[i+1:]
		}
		return pyRel(fromPkg, target)
	}
	g.P("from typing import Optional, Protocol")
	g.P()
	g.P("import angzarr_router_ffi as ", pyAz)
	p, m := fw(pyTypesModule)
	g.P("from ", p, " import ", m, " as ", pyTypes)
	if r.needCmdHdr {
		p, m := fw(pyCmdHdrModule)
		g.P("from ", p, " import ", m, " as ", pyCmdH)
	}
	if r.needPM {
		p, m := fw(pyPMModule)
		g.P("from ", p, " import ", m, " as ", pyPM)
	}
	for _, pth := range r.order {
		pkg, module := pyModule(pth)
		alias := r.alias[pth]
		switch {
		case frameworkPkg != "" && pyIsFramework(pth):
			p, m := fw(pkg + "." + module)
			g.P("from ", p, " import ", m, " as ", alias)
		case pkg == "":
			g.P("import ", module, " as ", alias)
		default:
			p, m := pyRel(fromPkg, pkg+"."+module)
			g.P("from ", p, " import ", m, " as ", alias)
		}
	}
	g.P()
}

func (e pyEmitter) EmitComponent(g *protogen.GeneratedFile, file *protogen.File, s *Service) error {
	refs := newPyRefs([]*Service{s})
	g.P("# Code generated by angzarr codegen python. DO NOT EDIT.")
	g.P("# source: ", file.Desc.Path())
	g.P()
	refs.emitImports(g, pyFilePkg(file), e.frameworkPkg)
	emitPyInterface(g, refs, s, e.pySigs(refs, s))
	switch s.Component.Kind {
	case KindSaga:
		emitPySagaDispatch(g, refs, s)
	case KindAggregate:
		emitPyAggregateDispatch(g, refs, s)
	case KindProcessManager:
		emitPyPMDispatch(g, refs, s)
	case KindProjector:
		emitPyProjectorDispatch(g, refs, s)
	default:
		return fmt.Errorf("unsupported component kind %v", s.Component.Kind)
	}
	emitPyRegister(g, s)
	return nil
}

func (e pyEmitter) EmitScaffoldComponent(g *protogen.GeneratedFile, file *protogen.File, s *Service) error {
	refs := newPyRefs([]*Service{s})
	g.P("# Scaffolded ONCE by angzarr codegen python — this file is YOURS.")
	g.P("#")
	g.P("# Regeneration will NOT overwrite this file. It is your responsibility to")
	g.P("# keep the generated <Component>Handler interface implemented: when a")
	g.P("# command or event is added to the proto, the handler will be missing a")
	g.P("# method until you add it here.")
	g.P()
	refs.emitImports(g, pyFilePkg(file), e.frameworkPkg)
	g.P("class ", s.GoName, ":")
	g.P("    \"\"\"Implements ", s.GoName, "Handler.\"\"\"")
	g.P()
	for _, m := range e.pySigs(refs, s) {
		g.P("    def ", m.name, m.params, m.returns, ":")
		g.P("        raise NotImplementedError(", pyQuote("TODO: implement "+s.GoName+"."+m.name), ")")
		g.P()
	}
	return nil
}

// pySig is one Python handler method's signature, shared by the Protocol and
// the scaffold so the two never drift.
type pySig struct {
	name    string
	params  string // includes the enclosing parens, starting with self
	returns string // includes the "-> …" arrow
}

func (e pyEmitter) pySigs(refs *pyRefs, s *Service) []pySig {
	switch s.Component.Kind {
	case KindSaga:
		return pySagaSigs(refs, s)
	case KindAggregate:
		return pyAggregateSigs(refs, s)
	case KindProcessManager:
		return pyPMSigs(refs, s)
	case KindProjector:
		return pyProjectorSigs(refs, s)
	}
	return nil
}

func emitPyInterface(g *protogen.GeneratedFile, refs *pyRefs, s *Service, sigs []pySig) {
	g.P("class ", s.GoName, "Handler(Protocol):")
	g.P("    \"\"\"Strict business seam for the ", s.GoName, " ", strings.ToLower(s.Component.Kind.String()), ".\"\"\"")
	g.P()
	for _, m := range sigs {
		g.P("    def ", m.name, m.params, m.returns, ": ...")
	}
	g.P()
}

func pySagaSigs(refs *pyRefs, s *Service) []pySig {
	var out []pySig
	for _, h := range s.Handlers {
		out = append(out, pySig{
			name:    snake(h.MethodName),
			params:  "(self, event: " + refs.ref(h.Message) + ", dests: " + pyAz + ".Destinations, source_cover: " + pyTypes + ".Cover)",
			returns: " -> tuple[list[" + pyTypes + ".CommandBook], list[" + pyTypes + ".EventBook]]",
		})
	}
	for _, r := range s.Rejections {
		out = append(out, pySig{
			name:    snake(r.MethodName),
			params:  "(self, n: " + pyTypes + ".Notification, rejection: " + pyTypes + ".RejectionNotification)",
			returns: " -> list[" + pyTypes + ".EventBook]",
		})
	}
	return out
}

func pyAggregateSigs(refs *pyRefs, s *Service) []pySig {
	var out []pySig
	for _, h := range s.Handlers {
		returns := " -> Optional[" + pyTypes + ".EventBook]"
		if h.TypedEmit() {
			returns = " -> list[" + refs.ref(h.Emits[0]) + "]"
		}
		out = append(out, pySig{
			name:    snake(h.MethodName),
			params:  "(self, cmd: " + refs.ref(h.Message) + ", state: " + refs.ref(s.State) + ", cctx: " + pyAz + ".CommandContext)",
			returns: returns,
		})
	}
	for _, a := range s.Appliers {
		out = append(out, pySig{
			name:    snake(a.MethodName),
			params:  "(self, state: " + refs.ref(s.State) + ", event: " + refs.ref(a.Message) + ")",
			returns: " -> None",
		})
	}
	for _, r := range s.Rejections {
		out = append(out, pySig{
			name:    snake(r.MethodName),
			params:  "(self, n: " + pyTypes + ".Notification, rejection: " + pyTypes + ".RejectionNotification, state: " + refs.ref(s.State) + ", cctx: " + pyAz + ".CommandContext)",
			returns: " -> Optional[" + pyCmdH + ".BusinessResponse]",
		})
	}
	return out
}

func pyPMSigs(refs *pyRefs, s *Service) []pySig {
	var out []pySig
	for _, h := range s.Handlers {
		out = append(out, pySig{
			name:    snake(h.MethodName),
			params:  "(self, event: " + refs.ref(h.Message) + ", state: " + refs.ref(s.State) + ", dests: " + pyAz + ".Destinations)",
			returns: " -> " + pyPM + ".ProcessManagerHandleResponse",
		})
	}
	for _, a := range s.Appliers {
		out = append(out, pySig{
			name:    snake(a.MethodName),
			params:  "(self, state: " + refs.ref(s.State) + ", event: " + refs.ref(a.Message) + ")",
			returns: " -> None",
		})
	}
	for _, r := range s.Rejections {
		out = append(out, pySig{
			name:    snake(r.MethodName),
			params:  "(self, n: " + pyTypes + ".Notification, rejection: " + pyTypes + ".RejectionNotification, state: " + refs.ref(s.State) + ")",
			returns: " -> tuple[list[" + pyTypes + ".EventBook], Optional[" + pyTypes + ".Notification]]",
		})
	}
	return out
}

func pyProjectorSigs(refs *pyRefs, s *Service) []pySig {
	var out []pySig
	for _, h := range s.Handlers {
		out = append(out, pySig{
			name:    snake(h.MethodName),
			params:  "(self, projection: " + refs.ref(s.State) + ", event: " + refs.ref(h.Message) + ")",
			returns: " -> None",
		})
	}
	out = append(out, pySig{
		name:    "finish",
		params:  "(self, projection: " + refs.ref(s.State) + ", events: " + pyTypes + ".EventBook)",
		returns: " -> " + pyTypes + ".Projection",
	})
	return out
}

func emitPySagaDispatch(g *protogen.GeneratedFile, refs *pyRefs, s *Service) {
	c := s.Component
	g.P("def new_", snake(s.GoName), "_dispatch(handler: ", s.GoName, "Handler) -> ", pyAz, ".SagaDispatch:")
	g.P("    dispatch = ", pyAz, ".SagaDispatch(", pyQuote(s.GoName), ", ", pyQuote(c.InputDomain), ", targets=[", pyQuote(c.OutputDomain), "])")
	for _, h := range s.Handlers {
		fn := "_on_" + snake(h.MethodName)
		g.P("    def ", fn, "(event_any, dests, source_cover):")
		g.P("        event = ", refs.ref(h.Message), "()")
		emitPyUnpack(g, "event", "event_any")
		g.P("        return handler.", snake(h.MethodName), "(event, dests, source_cover)")
		g.P("    dispatch.on_event(", pyQuote(fqName(h.Message)), ", ", fn, ")")
	}
	for _, r := range s.Rejections {
		g.P("    dispatch.on_rejected(", pyQuote(r.Command), ", handler.", snake(r.MethodName), ")")
	}
	g.P("    return dispatch")
	g.P()
}

func emitPyAggregateDispatch(g *protogen.GeneratedFile, refs *pyRefs, s *Service) {
	c := s.Component
	g.P("def new_", snake(s.GoName), "_dispatch(handler: ", s.GoName, "Handler) -> ", pyAz, ".AggregateDispatch:")
	g.P("    rebuilder = ", pyAz, ".Rebuilder(lambda: ", refs.ref(s.State), "())")
	g.P("    rebuilder.with_snapshot(lambda state, payload: state.ParseFromString(payload.value))")
	emitPyAppliers(g, refs, s)
	g.P("    dispatch = ", pyAz, ".AggregateDispatch(", pyQuote(s.GoName), ", ", pyQuote(c.InputDomain), ", rebuilder)")
	for _, h := range s.Handlers {
		fn := "_on_" + snake(h.MethodName)
		g.P("    def ", fn, "(cmd_any, state, cctx):")
		g.P("        cmd = ", refs.ref(h.Message), "()")
		emitPyUnpack(g, "cmd", "cmd_any")
		if h.TypedEmit() {
			g.P("        events = handler.", snake(h.MethodName), "(cmd, state, cctx)")
			g.P("        book = ", pyTypes, ".EventBook()")
			g.P("        for ev in events:")
			g.P("            book.pages.add().event.CopyFrom(", pyAz, ".pack(ev))")
			g.P("        return book")
		} else {
			g.P("        return handler.", snake(h.MethodName), "(cmd, state, cctx)")
		}
		g.P("    dispatch.on_command(", pyQuote(fqName(h.Message)), ", ", fn, ")")
	}
	for _, r := range s.Rejections {
		g.P("    dispatch.on_rejected(", pyQuote(r.Command), ", handler.", snake(r.MethodName), ")")
	}
	g.P("    return dispatch")
	g.P()
}

func emitPyPMDispatch(g *protogen.GeneratedFile, refs *pyRefs, s *Service) {
	c := s.Component
	g.P("def new_", snake(s.GoName), "_dispatch(handler: ", s.GoName, "Handler) -> ", pyAz, ".ProcessManagerDispatch:")
	g.P("    rebuilder = ", pyAz, ".Rebuilder(lambda: ", refs.ref(s.State), "())")
	g.P("    rebuilder.with_snapshot(lambda state, payload: state.ParseFromString(payload.value))")
	emitPyAppliers(g, refs, s)
	g.P("    dispatch = ", pyAz, ".ProcessManagerDispatch(", pyQuote(s.GoName), ", ", pyQuote(c.OutputDomain), ", rebuilder)")
	for _, h := range s.Handlers {
		fn := "_on_" + snake(h.MethodName)
		g.P("    def ", fn, "(event_any, state, dests):")
		g.P("        event = ", refs.ref(h.Message), "()")
		emitPyUnpack(g, "event", "event_any")
		g.P("        return handler.", snake(h.MethodName), "(event, state, dests)")
		g.P("    dispatch.on_event(", pyQuote(h.SourceDomain), ", ", pyQuote(fqName(h.Message)), ", ", fn, ")")
	}
	for _, r := range s.Rejections {
		g.P("    dispatch.on_rejected(", pyQuote(r.Command), ", handler.", snake(r.MethodName), ")")
	}
	g.P("    return dispatch")
	g.P()
}

func emitPyProjectorDispatch(g *protogen.GeneratedFile, refs *pyRefs, s *Service) {
	g.P("def new_", snake(s.GoName), "_dispatch(handler: ", s.GoName, "Handler) -> ", pyAz, ".ProjectorDispatch:")
	g.P("    dispatch = ", pyAz, ".ProjectorDispatch(", pyQuote(s.GoName), ", lambda: ", refs.ref(s.State), "())")
	// A projector consumes events from every domain its handlers source from
	// (a display can fold table, hand and player events) — not a single input
	// domain. Restrict folding to that exact set; empty means consume all.
	seen := map[string]bool{}
	var domains []string
	for _, h := range s.Handlers {
		if h.SourceDomain != "" && !seen[h.SourceDomain] {
			seen[h.SourceDomain] = true
			domains = append(domains, h.SourceDomain)
		}
	}
	sort.Strings(domains)
	if len(domains) > 0 {
		quoted := make([]string, len(domains))
		for i, d := range domains {
			quoted[i] = pyQuote(d)
		}
		g.P("    dispatch.for_domains(", strings.Join(quoted, ", "), ")")
	}
	for _, h := range s.Handlers {
		fn := "_on_" + snake(h.MethodName)
		g.P("    def ", fn, "(projection, event_any):")
		g.P("        event = ", refs.ref(h.Message), "()")
		emitPyUnpack(g, "event", "event_any")
		g.P("        handler.", snake(h.MethodName), "(projection, event)")
		g.P("    dispatch.on_event(", pyQuote(fqName(h.Message)), ", ", fn, ")")
	}
	g.P("    dispatch.finish(handler.finish)")
	g.P("    return dispatch")
	g.P()
}

// emitPyAppliers registers each event applier on the rebuilder: a nested fn
// that unpacks the payload into the typed event and folds it into state.
// Identical for aggregates and process managers (both rebuild their own state).
func emitPyAppliers(g *protogen.GeneratedFile, refs *pyRefs, s *Service) {
	for _, a := range s.Appliers {
		fn := "_apply_" + snake(a.MethodName)
		g.P("    def ", fn, "(state, payload):")
		g.P("        event = ", refs.ref(a.Message), "()")
		emitPyUnpack(g, "event", "payload")
		g.P("        handler.", snake(a.MethodName), "(state, event)")
		g.P("    rebuilder.apply(", pyQuote(fqName(a.Message)), ", ", fn, ")")
	}
}

// emitPyUnpack parses an Any payload into a typed message, raising the binding's
// coded decode error on failure.
func emitPyUnpack(g *protogen.GeneratedFile, target, any string) {
	g.P("        try:")
	g.P("            ", target, ".ParseFromString(", any, ".value)")
	g.P("        except Exception as exc:")
	g.P("            raise ", pyAz, ".any_decode_error(", any, ".type_url, exc)")
}

func emitPyRegister(g *protogen.GeneratedFile, s *Service) {
	name := snake(s.GoName)
	g.P("def register_", name, "(router: ", pyAz, ".Router, handler: ", s.GoName, "Handler) -> None:")
	switch s.Component.Kind {
	case KindSaga:
		g.P("    router.register_saga(new_", name, "_dispatch(handler))")
	case KindAggregate:
		g.P("    router.register_aggregate(new_", name, "_dispatch(handler))")
	case KindProcessManager:
		g.P("    router.register_process_manager(new_", name, "_dispatch(handler))")
	case KindProjector:
		g.P("    router.register_projector(new_", name, "_dispatch(handler))")
	}
	g.P()
}

// pyModule splits a proto file path into the Python (package, module) the
// protocolbuffers generator emits: "a/b/c.proto" -> ("a.b", "c_pb2").
func pyModule(path string) (pkg, module string) {
	path = strings.TrimSuffix(path, ".proto")
	dotted := strings.ReplaceAll(path, "/", ".")
	if i := strings.LastIndex(dotted, "."); i >= 0 {
		return dotted[:i], dotted[i+1:] + "_pb2"
	}
	return "", dotted + "_pb2"
}

// snake converts an exported CamelCase identifier to Python snake_case.
func snake(name string) string {
	var b strings.Builder
	for i, r := range name {
		if unicode.IsUpper(r) {
			if i > 0 && (!unicode.IsUpper(rune(name[i-1])) || (i+1 < len(name) && unicode.IsLower(rune(name[i+1])))) {
				b.WriteByte('_')
			}
			b.WriteRune(unicode.ToLower(r))
		} else {
			b.WriteRune(r)
		}
	}
	return b.String()
}

// pyQuote renders a Go string as a double-quoted Python string literal.
func pyQuote(s string) string {
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

// itoa is a tiny strconv-free int formatter for small alias indices.
func itoa(i int) string {
	if i == 0 {
		return "0"
	}
	var digits []byte
	for i > 0 {
		digits = append([]byte{byte('0' + i%10)}, digits...)
		i /= 10
	}
	return string(digits)
}
