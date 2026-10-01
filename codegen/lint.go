package codegen

// The linter is the single declaration-validation pass shared by codegen and
// the standalone `angzarr lint` command. It reads the same component / command
// / event markers the emitters consume and reports every problem at once
// (collect-all) rather than failing on the first — so a developer fixes the
// whole proto in one round-trip. Codegen runs the same analysis and refuses to
// emit when any error-severity diagnostic fires, so generated code is only ever
// produced from declarations that resolve and wire up correctly.
//
// Three tiers, by what they protect:
//   - Tier A (error): marker string references resolve to the right kind of
//     element, and required per-kind fields are present.
//   - Tier B (error): the generated identifiers don't collide, so the emitted
//     source compiles.
//   - Tier C (warning): the wiring is coherent — emitted events are folded,
//     domains have a producer/consumer, components actually do something —
//     so the generated code that compiles also works at runtime.

import (
	"fmt"
	"sort"
	"strings"

	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/reflect/protodesc"
	"google.golang.org/protobuf/reflect/protoreflect"
)

// Severity ranks a diagnostic: errors block code generation, warnings inform.
type Severity int

const (
	// SeverityError marks a declaration that would generate broken or
	// uncompilable code; it blocks generation.
	SeverityError Severity = iota
	// SeverityWarning marks coherent-but-suspect wiring that compiles but may
	// not work as intended; it does not block generation.
	SeverityWarning
)

func (s Severity) String() string {
	if s == SeverityWarning {
		return "warning"
	}
	return "error"
}

// Position locates a diagnostic in proto source. Line/Col are 1-based and 0
// when the descriptor set carries no source-code info (e.g. an image built
// with --exclude-source-info, or an in-memory test descriptor).
type Position struct {
	File string
	Line int
	Col  int
}

// Diagnostic is one finding. Code is a stable ANZxxxx identifier so checks can
// be referenced and suppressed independently of their wording.
type Diagnostic struct {
	Severity Severity
	Code     string
	Message  string
	Pos      Position
}

func (d Diagnostic) String() string {
	loc := d.Pos.File
	if loc == "" {
		loc = "<unknown>"
	} else if d.Pos.Line > 0 {
		loc = fmt.Sprintf("%s:%d:%d", d.Pos.File, d.Pos.Line, d.Pos.Col)
	}
	return fmt.Sprintf("%s: %s[%s]: %s", loc, d.Severity, d.Code, d.Message)
}

// Lint analyzes a compiled descriptor set and returns every declaration
// diagnostic. It never fails: a request that imports no options.proto simply
// has nothing to validate and returns no diagnostics.
func Lint(gen *protogen.Plugin) []Diagnostic {
	_, diags := analyze(gen)
	return diags
}

// HasErrors reports whether any diagnostic is error-severity (i.e. generation
// must not proceed).
func HasErrors(diags []Diagnostic) bool {
	for _, d := range diags {
		if d.Severity == SeverityError {
			return true
		}
	}
	return false
}

// diagError folds the error-severity diagnostics into one generation error so
// codegen surfaces every blocking problem through the protoc plugin protocol.
func diagError(diags []Diagnostic) error {
	var msgs []string
	for _, d := range diags {
		if d.Severity == SeverityError {
			msgs = append(msgs, d.String())
		}
	}
	return fmt.Errorf("declaration validation failed:\n%s", strings.Join(msgs, "\n"))
}

// analyze builds the component model and collects all diagnostics in one pass.
// The model it returns is only sound when HasErrors(diags) is false; codegen
// gates emission on that, so an invalid model is never handed to an emitter.
func analyze(gen *protogen.Plugin) ([]fileComponents, []Diagnostic) {
	exts, failures := resolveExtensions(gen)
	registry := messageRegistry(gen)
	diags := unresolvedOptionDiags(gen, exts, failures)

	// pass 1: one Component per component anchor; declaration order is captured
	// for deterministic cross-checks and diagnostics.
	services := make(map[string]*Component)
	var order []string
	for _, file := range gen.Files {
		for _, m := range allMessages(file.Messages) {
			component := componentOptions(m, exts)
			if component == nil {
				continue
			}
			fq := string(m.Desc.FullName())
			if _, dup := services[fq]; dup {
				diags = append(diags, errDiag("ANZ001", m, fmt.Sprintf("duplicate component declaration %q", fq)))
				continue
			}
			s := &Component{Anchor: m, Component: component, BaseName: baseName(m, component), StubName: stubName(m, component)}
			if component.Kind != KindSaga {
				s.State = m
			}
			services[fq] = s
			order = append(order, fq)
		}
	}

	// pass 2: attach commands and events to their owning component.
	for _, file := range gen.Files {
		for _, m := range allMessages(file.Messages) {
			if cmd := commandOptions(m, exts); cmd != nil {
				diags = append(diags, attachCommand(services, registry, m, cmd)...)
			}
			for _, ev := range eventOptions(m, exts) {
				diags = append(diags, attachEvent(services, m, ev)...)
			}
		}
	}

	// Projector domain filter, computed once for every emitter.
	for _, fq := range order {
		s := services[fq]
		if s.Component.Kind == KindProjector {
			s.ProjectorDomains = projectorDomains(s)
		}
	}

	// compensation references + per-kind required-field contract.
	for _, fq := range order {
		s := services[fq]
		for _, cmd := range s.Component.Compensates {
			if !resolves(registry, cmd) {
				diags = append(diags, errDiag("ANZ007", s.Anchor, fmt.Sprintf("(component).compensates %q is not a fully-qualified message name in the compiled set (short names never match dispatch)", cmd)))
				continue
			}
			s.Rejections = append(s.Rejections, Rejection{Command: cmd, MethodName: "On" + shortName(cmd) + "Rejected"})
		}
		diags = append(diags, requiredFields(s)...)
	}

	diags = append(diags, collisionDiags(services, order)...)
	diags = append(diags, typeCollisionDiags(gen, services, order)...)
	diags = append(diags, coherenceDiags(services, order)...)

	// group by anchor file, preserving message declaration order within a file.
	var result []fileComponents
	for _, file := range gen.Files {
		if !file.Generate {
			continue
		}
		var fileSvcs []*Component
		for _, m := range allMessages(file.Messages) {
			if s, ok := services[string(m.Desc.FullName())]; ok {
				fileSvcs = append(fileSvcs, s)
			}
		}
		if len(fileSvcs) > 0 {
			result = append(result, fileComponents{File: file, Components: fileSvcs})
		}
	}
	return result, diags
}

// unresolvedOptionDiags reports every message carrying angzarr option bytes
// (component 50100, command 50104, event 50105) that no resolved extension
// decodes. Without it such a request would lint clean and generate nothing.
func unresolvedOptionDiags(gen *protogen.Plugin, exts extensions, failures []string) []Diagnostic {
	why := "no file in the request defines it (is io/angzarr/v1/options.proto imported and part of the request?)"
	if len(failures) > 0 {
		why = "its definition could not be loaded: " + strings.Join(failures, "; ")
	}
	var diags []Diagnostic
	for _, file := range gen.Files {
		for _, m := range allMessages(file.Messages) {
			for _, num := range unresolvedOptionNumbers(m, exts) {
				diags = append(diags, errDiag("ANZ009", m, fmt.Sprintf("message %q carries angzarr option %d, but %s", m.Desc.FullName(), num, why)))
			}
		}
	}
	return diags
}

// resolves reports whether a fully-qualified type reference names a message in
// the compiled set.
func resolves(registry map[string]*protogen.Message, name string) bool {
	if name == "" {
		return false
	}
	_, ok := registry[name]
	return ok
}

// attachCommand wires a command message to its aggregate owner, collecting a
// diagnostic for every unresolved reference.
func attachCommand(services map[string]*Component, registry map[string]*protogen.Message, m *protogen.Message, cmd *command) []Diagnostic {
	owner, ok := services[cmd.Component]
	if !ok {
		return []Diagnostic{errDiag("ANZ002", m, fmt.Sprintf("(command).component %q is not a declared component", cmd.Component))}
	}
	if owner.Component.Kind != KindAggregate {
		return []Diagnostic{errDiag("ANZ003", m, fmt.Sprintf("(command).component %q is a %v; commands are handled by aggregates", cmd.Component, owner.Component.Kind))}
	}
	var diags []Diagnostic
	var emits []*protogen.Message
	for _, name := range cmd.Emits {
		if name == "" {
			diags = append(diags, errDiag("ANZ004", m, "(command).emits has an empty event type"))
			continue
		}
		e, ok := registry[name]
		if !ok {
			diags = append(diags, errDiag("ANZ004", m, fmt.Sprintf("(command).emits %q is not a fully-qualified message name in the compiled set (short names never match dispatch)", name)))
			continue
		}
		emits = append(emits, e)
	}
	owner.Handlers = append(owner.Handlers, Handler{Message: m, MethodName: m.GoIdent.GoName, Emits: emits})
	return diags
}

// attachEvent classifies one event-consumer entry as an applier or a trigger
// handler on its owning component, collecting diagnostics for unresolved or
// underspecified entries.
func attachEvent(services map[string]*Component, m *protogen.Message, ev eventConsumer) []Diagnostic {
	owner, ok := services[ev.Component]
	if !ok {
		return []Diagnostic{errDiag("ANZ005", m, fmt.Sprintf("(event).component %q is not a declared component", ev.Component))}
	}
	switch owner.Component.Kind {
	case KindAggregate:
		owner.Appliers = append(owner.Appliers, Applier{Message: m, MethodName: applierName(m)})
	case KindProcessManager:
		if ev.Applies {
			owner.Appliers = append(owner.Appliers, Applier{Message: m, MethodName: applierName(m)})
		} else {
			if ev.Domain == "" {
				return []Diagnostic{errDiag("ANZ006", m, fmt.Sprintf("process-manager trigger for %q requires (event).domain", ev.Component))}
			}
			owner.Handlers = append(owner.Handlers, Handler{Message: m, MethodName: m.GoIdent.GoName, SourceDomain: ev.Domain})
		}
	case KindSaga, KindProjector:
		owner.Handlers = append(owner.Handlers, Handler{Message: m, MethodName: m.GoIdent.GoName, SourceDomain: ev.Domain})
	default:
		return []Diagnostic{errDiag("ANZ005", m, fmt.Sprintf("(event).component %q has unsupported kind %v", ev.Component, owner.Component.Kind))}
	}
	return nil
}

// requiredFields enforces the per-kind required-field contract: an omitted
// domain would wire a component that silently receives or targets nothing.
func requiredFields(s *Component) []Diagnostic {
	c := s.Component
	switch c.Kind {
	case KindAggregate:
		if c.InputDomain == "" {
			return []Diagnostic{errDiag("ANZ008", s.Anchor, "aggregate requires input_domain (its own domain)")}
		}
	case KindSaga:
		if c.InputDomain == "" || c.OutputDomain == "" {
			return []Diagnostic{errDiag("ANZ008", s.Anchor, "saga requires input_domain and output_domain")}
		}
	case KindProcessManager:
		if c.OutputDomain == "" {
			return []Diagnostic{errDiag("ANZ008", s.Anchor, "process manager requires output_domain (its pm_domain and command-target domain)")}
		}
	case KindProjector:
		// No required field: the domain filter (Component.ProjectorDomains)
		// is the union of input_domain and the handlers' (event).domain, so
		// a projector spanning several domains may declare them per event
		// and leave input_domain empty.
	default:
		return []Diagnostic{errDiag("ANZ008", s.Anchor, fmt.Sprintf("unsupported component kind %v", c.Kind))}
	}
	return nil
}

// projectorDomains computes a projector's domain filter: the sorted,
// deduplicated union of its declared input_domain and every handler's
// (event).domain. Every emitter renders this one list.
func projectorDomains(s *Component) []string {
	seen := make(map[string]bool)
	var domains []string
	add := func(d string) {
		if d != "" && !seen[d] {
			seen[d] = true
			domains = append(domains, d)
		}
	}
	add(s.Component.InputDomain)
	for _, h := range s.Handlers {
		add(h.SourceDomain)
	}
	sort.Strings(domains)
	return domains
}

// collisionDiags catches generated-identifier clashes that would produce
// uncompilable source: two components emitting the same base name, or one
// component generating the same method twice. Handlers, appliers, rejections
// and the projector's fixed Finish all land on the same generated interface,
// so the duplicate check runs over their union as one namespace.
func collisionDiags(services map[string]*Component, order []string) []Diagnostic {
	var diags []Diagnostic

	first := make(map[string]string) // BaseName -> first anchor FQ
	for _, fq := range order {
		s := services[fq]
		if owner, dup := first[s.BaseName]; dup {
			diags = append(diags, errDiag("ANZ010", s.Anchor, fmt.Sprintf("generated name %q collides with component %q; set a distinct (component).name", s.BaseName, owner)))
			continue
		}
		first[s.BaseName] = fq
	}

	for _, fq := range order {
		s := services[fq]
		names := make([]string, 0, len(s.Handlers)+len(s.Appliers)+len(s.Rejections)+1)
		names = append(names, handlerNames(s.Handlers)...)
		names = append(names, applierNames(s.Appliers)...)
		names = append(names, rejectionNames(s.Rejections)...)
		if s.Component.Kind == KindProjector {
			names = append(names, projectorFinishMethod)
		}
		diags = append(diags, dupMethods(s, fq, names)...)
	}
	return diags
}

// typeCollisionDiags catches generated top-level identifiers that coincide with
// a proto-generated type in the anchor's package. The wiring and the scaffold
// stub are emitted beside the proto types (same Go package, Java package, C#
// and C++ namespace), so such a name is a redeclaration in those languages.
func typeCollisionDiags(gen *protogen.Plugin, services map[string]*Component, order []string) []Diagnostic {
	types := make(map[protoreflect.FullName]map[string]bool) // package -> type names
	for _, f := range gen.Files {
		pkg := f.Desc.Package()
		if types[pkg] == nil {
			types[pkg] = make(map[string]bool)
		}
		for _, e := range f.Enums {
			types[pkg][e.GoIdent.GoName] = true
		}
		for _, m := range allMessages(f.Messages) {
			types[pkg][m.GoIdent.GoName] = true
			for _, e := range m.Enums {
				types[pkg][e.GoIdent.GoName] = true
			}
		}
	}
	var diags []Diagnostic
	for _, fq := range order {
		s := services[fq]
		pkgTypes := types[s.Anchor.Desc.ParentFile().Package()]
		for _, id := range generatedTypeNames(s) {
			if pkgTypes[id] {
				diags = append(diags, errDiag("ANZ012", s.Anchor, fmt.Sprintf("component %q generates %q, which collides with the proto type %q in package %q; set a (component).name distinct from every message and enum in the package", fq, id, id, s.Anchor.Desc.ParentFile().Package())))
			}
		}
	}
	return diags
}

// generatedTypeNames lists the top-level identifiers emitted for a component
// across the target languages: the scaffold stub type, the handler interface,
// the Java/C# wiring class, and the Go/C++ dispatch constructor and register
// functions.
func generatedTypeNames(s *Component) []string {
	return []string{
		s.StubName,
		s.BaseName + "Handler",
		s.BaseName + "Angzarr",
		"New" + s.BaseName + "Dispatch",
		"Register" + s.BaseName,
	}
}

// projectorFinishMethod is the fixed method every projector interface carries
// alongside its handlers.
const projectorFinishMethod = "Finish"

// methodRenderings are the per-language spellings of a generated method name.
// Two distinct names collide when any rendering coincides: "HTTPGet" and
// "HttpGet" are separate Go methods but the same Python http_get.
var methodRenderings = []struct {
	langs  string
	render func(string) string
}{
	{"every language", func(n string) string { return n }},
	{"java/typescript", lowerFirst},
	{"python", snake},
}

func handlerNames(hs []Handler) []string {
	out := make([]string, len(hs))
	for i, h := range hs {
		out[i] = h.MethodName
	}
	return out
}

func applierNames(as []Applier) []string {
	out := make([]string, len(as))
	for i, a := range as {
		out[i] = a.MethodName
	}
	return out
}

func rejectionNames(rs []Rejection) []string {
	out := make([]string, len(rs))
	for i, r := range rs {
		out[i] = r.MethodName
	}
	return out
}

// dupMethods reports each pair of method names that render identically in
// some target language, once per pair, naming the first rendering that
// collides.
func dupMethods(s *Component, fq string, names []string) []Diagnostic {
	var diags []Diagnostic
	for j := 1; j < len(names); j++ {
		for i := 0; i < j; i++ {
			for _, r := range methodRenderings {
				if rendered := r.render(names[i]); rendered == r.render(names[j]) {
					diags = append(diags, errDiag("ANZ011", s.Anchor, fmt.Sprintf("component %q generates duplicate method %q in %s (from %q and %q; handler, applier, rejection and projector Finish names share one generated namespace)", fq, rendered, r.langs, names[i], names[j])))
					break
				}
			}
		}
	}
	return diags
}

// coherenceDiags warns when wiring compiles but is unlikely to work: emitted
// events with no applier, output/source domains with no counterpart aggregate,
// and components that handle nothing. These are warnings, not errors: the
// counterpart may legitimately live in a proto set not part of this compile.
func coherenceDiags(services map[string]*Component, order []string) []Diagnostic {
	var diags []Diagnostic

	aggDomains := make(map[string]bool)
	for _, fq := range order {
		if s := services[fq]; s.Component.Kind == KindAggregate && s.Component.InputDomain != "" {
			aggDomains[s.Component.InputDomain] = true
		}
	}

	for _, fq := range order {
		s := services[fq]
		c := s.Component

		if len(s.Handlers) == 0 && len(s.Appliers) == 0 {
			diags = append(diags, warnDiag("ANZ103", s.Anchor, fmt.Sprintf("component %q declares no commands or events; it dispatches nothing", fq)))
		}

		if c.Kind == KindAggregate {
			applied := make(map[string]bool)
			for _, a := range s.Appliers {
				applied[string(a.Message.Desc.FullName())] = true
			}
			for _, h := range s.Handlers {
				for _, e := range h.Emits {
					en := string(e.Desc.FullName())
					if !applied[en] {
						diags = append(diags, warnDiag("ANZ100", h.Message, fmt.Sprintf("command %q emits %q but the aggregate has no applier for it; rebuilt state will ignore the event", h.Message.Desc.FullName(), en)))
					}
				}
			}
		}

		if (c.Kind == KindSaga || c.Kind == KindProcessManager) && c.OutputDomain != "" && !aggDomains[c.OutputDomain] {
			diags = append(diags, warnDiag("ANZ101", s.Anchor, fmt.Sprintf("%v %q targets output_domain %q, but no aggregate declares it as input_domain; emitted commands reach no handler", c.Kind, fq, c.OutputDomain)))
		}

		for _, h := range s.Handlers {
			if h.SourceDomain != "" && !aggDomains[h.SourceDomain] {
				diags = append(diags, warnDiag("ANZ102", h.Message, fmt.Sprintf("%v %q triggers on domain %q, but no aggregate produces events there", c.Kind, fq, h.SourceDomain)))
			}
		}
	}
	return diags
}

func errDiag(code string, m *protogen.Message, msg string) Diagnostic {
	return Diagnostic{Severity: SeverityError, Code: code, Message: msg, Pos: posOf(m)}
}

func warnDiag(code string, m *protogen.Message, msg string) Diagnostic {
	return Diagnostic{Severity: SeverityWarning, Code: code, Message: msg, Pos: posOf(m)}
}

// posOf resolves a message's source position from the file's source-code info,
// falling back to file-only (Line 0) when the descriptor carries no spans.
func posOf(m *protogen.Message) Position {
	pos := Position{File: m.Location.SourceFile}
	if pos.File == "" {
		pos.File = m.Desc.ParentFile().Path()
	}
	fdp := protodesc.ToFileDescriptorProto(m.Desc.ParentFile())
	sci := fdp.GetSourceCodeInfo()
	if sci == nil {
		return pos
	}
	want := pathKey(m.Location.Path)
	for _, loc := range sci.GetLocation() {
		if len(loc.Span) >= 2 && pathKey(loc.Path) == want {
			pos.Line = int(loc.Span[0]) + 1
			pos.Col = int(loc.Span[1]) + 1
			return pos
		}
	}
	return pos
}

func pathKey(path []int32) string {
	parts := make([]string, len(path))
	for i, p := range path {
		parts[i] = fmt.Sprint(p)
	}
	return strings.Join(parts, ",")
}
