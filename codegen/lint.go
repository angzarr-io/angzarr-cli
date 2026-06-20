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
	"strings"

	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/reflect/protodesc"
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
func analyze(gen *protogen.Plugin) ([]fileServices, []Diagnostic) {
	exts := resolveExtensions(gen)
	registry := messageRegistry(gen)
	var diags []Diagnostic

	// pass 1: one Service per component anchor; declaration order is captured
	// for deterministic cross-checks and diagnostics.
	services := make(map[string]*Service)
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
			s := &Service{Anchor: m, Component: component, GoName: baseName(m, component)}
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
	diags = append(diags, coherenceDiags(services, order)...)

	// group by anchor file, preserving message declaration order within a file.
	var result []fileServices
	for _, file := range gen.Files {
		if !file.Generate {
			continue
		}
		var fileSvcs []*Service
		for _, m := range allMessages(file.Messages) {
			if s, ok := services[string(m.Desc.FullName())]; ok {
				fileSvcs = append(fileSvcs, s)
			}
		}
		if len(fileSvcs) > 0 {
			result = append(result, fileServices{File: file, Services: fileSvcs})
		}
	}
	return result, diags
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
func attachCommand(services map[string]*Service, registry map[string]*protogen.Message, m *protogen.Message, cmd *command) []Diagnostic {
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
func attachEvent(services map[string]*Service, m *protogen.Message, ev eventConsumer) []Diagnostic {
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
func requiredFields(s *Service) []Diagnostic {
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
		if c.InputDomain == "" {
			return []Diagnostic{errDiag("ANZ008", s.Anchor, "projector requires input_domain (its subscribed domains)")}
		}
	default:
		return []Diagnostic{errDiag("ANZ008", s.Anchor, fmt.Sprintf("unsupported component kind %v", c.Kind))}
	}
	return nil
}

// collisionDiags catches generated-identifier clashes that would produce
// uncompilable source: two components emitting the same base name, or one
// component generating the same handler/applier method twice (events sharing a
// short name across packages).
func collisionDiags(services map[string]*Service, order []string) []Diagnostic {
	var diags []Diagnostic

	first := make(map[string]string) // GoName -> first anchor FQ
	for _, fq := range order {
		s := services[fq]
		if owner, dup := first[s.GoName]; dup {
			diags = append(diags, errDiag("ANZ010", s.Anchor, fmt.Sprintf("generated name %q collides with component %q; set a distinct (component).name", s.GoName, owner)))
			continue
		}
		first[s.GoName] = fq
	}

	for _, fq := range order {
		s := services[fq]
		diags = append(diags, dupMethods(s, fq, "handler", handlerNames(s.Handlers))...)
		diags = append(diags, dupMethods(s, fq, "applier", applierNames(s.Appliers))...)
	}
	return diags
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

func dupMethods(s *Service, fq, kind string, names []string) []Diagnostic {
	var diags []Diagnostic
	seen := make(map[string]bool, len(names))
	for _, n := range names {
		if seen[n] {
			diags = append(diags, errDiag("ANZ011", s.Anchor, fmt.Sprintf("component %q generates duplicate %s method %q (events sharing a name across packages)", fq, kind, n)))
		}
		seen[n] = true
	}
	return diags
}

// coherenceDiags warns when wiring compiles but is unlikely to work: emitted
// events with no applier, output/source domains with no counterpart aggregate,
// and components that handle nothing. These are warnings, not errors: the
// counterpart may legitimately live in a proto set not part of this compile.
func coherenceDiags(services map[string]*Service, order []string) []Diagnostic {
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
