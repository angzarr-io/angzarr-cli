// Package codegen is the angzarr code generator core: it reads component
// declarations — proto services carrying (angzarr.v1.component) options,
// rpcs carrying (angzarr.v1.rejected/applies/reacts) — from a compiled
// descriptor set and emits per-language dispatch wiring through the
// registered emitters.
//
// The options are extracted DYNAMICALLY from the request's own descriptor
// pool rather than through compiled bindings, so this module carries no
// generated proto code: clients can link it in-process without colliding
// with their own angzarr.v1 registrations, and the generator works against
// whatever options.proto revision the caller compiled.
//
// Generation-time validation lives here, once, for every language:
// missing required component fields and unresolvable or short type names
// fail generation instead of producing wiring that silently never matches
// at runtime.
package codegen

import (
	"fmt"

	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/reflect/protodesc"
	"google.golang.org/protobuf/reflect/protoreflect"
	"google.golang.org/protobuf/reflect/protoregistry"
	"google.golang.org/protobuf/types/descriptorpb"
	"google.golang.org/protobuf/types/dynamicpb"
)

// Fully-qualified names of the angzarr declaration options. These — with
// their field names below — are the cross-language contract surface; the
// extension field numbers live in angzarr-project's options.proto.
const (
	extComponent = "angzarr_client.proto.angzarr.v1.component"
	extRejected  = "angzarr_client.proto.angzarr.v1.rejected"
	extApplies   = "angzarr_client.proto.angzarr.v1.applies"
	extReacts    = "angzarr_client.proto.angzarr.v1.reacts"
)

// ComponentKind identifies which dispatch component a service declares.
type ComponentKind int32

// Mirrors angzarr.v1.ComponentKind — the enum numbers are the wire contract.
const (
	KindUnspecified    ComponentKind = 0
	KindAggregate      ComponentKind = 1
	KindSaga           ComponentKind = 2
	KindProcessManager ComponentKind = 3
	KindProjector      ComponentKind = 4
)

func (k ComponentKind) String() string {
	switch k {
	case KindAggregate:
		return "AGGREGATE"
	case KindSaga:
		return "SAGA"
	case KindProcessManager:
		return "PROCESS_MANAGER"
	case KindProjector:
		return "PROJECTOR"
	default:
		return "UNSPECIFIED"
	}
}

// Component is the parsed (angzarr.v1.component) declaration.
type Component struct {
	Kind         ComponentKind
	InputDomain  string
	OutputDomain string
	State        string
}

// Rejected is the parsed (angzarr.v1.rejected) rpc declaration.
type Rejected struct {
	Command string
}

// Reacts is the parsed (angzarr.v1.reacts) rpc declaration.
type Reacts struct {
	Domain string
}

// extensions holds the dynamically-resolved option extension types for one
// generation run.
type extensions struct {
	component protoreflect.ExtensionType
	rejected  protoreflect.ExtensionType
	applies   protoreflect.ExtensionType
	reacts    protoreflect.ExtensionType
}

// resolveExtensions finds the angzarr option extensions in the request's
// own files. A request whose declarations never import options.proto has
// nothing to generate — every lookup will miss and the run emits nothing.
//
// The extension types are rebuilt against the process's own
// descriptor.proto rather than taken from protogen's universe: protobuf
// matches an extension's containing message by descriptor IDENTITY, and
// protogen rebuilds google.protobuf.ServiceOptions from the request, so
// extension types parented there silently fail to attach when reparsing
// the (globally-typed) options messages.
func resolveExtensions(gen *protogen.Plugin) extensions {
	var exts extensions
	for _, file := range gen.Files {
		if !hasAngzarrExtensions(file) {
			continue
		}
		registry := &protoregistry.Files{}
		if err := registry.RegisterFile(descriptorpb.File_google_protobuf_descriptor_proto); err != nil {
			continue
		}
		// The options file depends only on descriptor.proto; a file with
		// further dependencies cannot be rebuilt here and is skipped.
		rebuilt, err := protodesc.NewFile(protodesc.ToFileDescriptorProto(file.Desc), registry)
		if err != nil {
			continue
		}
		extDescs := rebuilt.Extensions()
		for i := 0; i < extDescs.Len(); i++ {
			d := extDescs.Get(i)
			switch d.FullName() {
			case extComponent:
				exts.component = dynamicpb.NewExtensionType(d)
			case extRejected:
				exts.rejected = dynamicpb.NewExtensionType(d)
			case extApplies:
				exts.applies = dynamicpb.NewExtensionType(d)
			case extReacts:
				exts.reacts = dynamicpb.NewExtensionType(d)
			}
		}
	}
	return exts
}

func hasAngzarrExtensions(file *protogen.File) bool {
	for _, ext := range file.Extensions {
		switch ext.Desc.FullName() {
		case extComponent, extRejected, extApplies, extReacts:
			return true
		}
	}
	return false
}

// reparse re-decodes an options message against the run's extension types
// so option bytes the global registry left unknown become readable fields.
func reparse(opts proto.Message, fresh proto.Message, exts extensions) proto.Message {
	raw, err := proto.Marshal(opts)
	if err != nil {
		return fresh
	}
	types := &dynamicTypes{exts: exts}
	_ = proto.UnmarshalOptions{Resolver: types}.Unmarshal(raw, fresh)
	return fresh
}

// dynamicTypes resolves only the angzarr extensions; everything else stays
// unknown, which is all the generator needs.
type dynamicTypes struct {
	exts extensions
}

func (t *dynamicTypes) FindExtensionByName(field protoreflect.FullName) (protoreflect.ExtensionType, error) {
	for _, ext := range []protoreflect.ExtensionType{t.exts.component, t.exts.rejected, t.exts.applies, t.exts.reacts} {
		if ext != nil && ext.TypeDescriptor().FullName() == field {
			return ext, nil
		}
	}
	return nil, fmt.Errorf("extension %s not part of the angzarr option surface", field)
}

func (t *dynamicTypes) FindExtensionByNumber(message protoreflect.FullName, field protoreflect.FieldNumber) (protoreflect.ExtensionType, error) {
	for _, ext := range []protoreflect.ExtensionType{t.exts.component, t.exts.rejected, t.exts.applies, t.exts.reacts} {
		if ext == nil {
			continue
		}
		d := ext.TypeDescriptor()
		if d.ContainingMessage().FullName() == message && d.Number() == field {
			return ext, nil
		}
	}
	return nil, fmt.Errorf("no angzarr extension %d on %s", field, message)
}

func (t *dynamicTypes) FindMessageByName(protoreflect.FullName) (protoreflect.MessageType, error) {
	return nil, fmt.Errorf("message types are not resolved dynamically")
}

func (t *dynamicTypes) FindMessageByURL(string) (protoreflect.MessageType, error) {
	return nil, fmt.Errorf("message types are not resolved dynamically")
}

// stringField reads a string field from a dynamic extension message by the
// contract field name.
func stringField(msg protoreflect.Message, name string) string {
	fd := msg.Descriptor().Fields().ByName(protoreflect.Name(name))
	if fd == nil {
		return ""
	}
	return msg.Get(fd).String()
}

// componentOptions extracts the (angzarr.v1.component) declaration.
func componentOptions(svc *protogen.Service, exts extensions) *Component {
	if exts.component == nil {
		return nil
	}
	opts, ok := svc.Desc.Options().(*descriptorpb.ServiceOptions)
	if !ok || opts == nil {
		return nil
	}
	reparsed := reparse(opts, &descriptorpb.ServiceOptions{}, exts)
	if !proto.HasExtension(reparsed, exts.component) {
		return nil
	}
	msg, ok := proto.GetExtension(reparsed, exts.component).(protoreflect.Message)
	if !ok {
		dyn, ok := proto.GetExtension(reparsed, exts.component).(*dynamicpb.Message)
		if !ok {
			return nil
		}
		msg = dyn
	}
	kindField := msg.Descriptor().Fields().ByName("kind")
	component := &Component{
		InputDomain:  stringField(msg, "input_domain"),
		OutputDomain: stringField(msg, "output_domain"),
		State:        stringField(msg, "state"),
	}
	if kindField != nil {
		component.Kind = ComponentKind(msg.Get(kindField).Enum())
	}
	if component.Kind == KindUnspecified {
		return nil
	}
	return component
}

func methodOptions(m *protogen.Method) *descriptorpb.MethodOptions {
	opts, _ := m.Desc.Options().(*descriptorpb.MethodOptions)
	return opts
}

// rejectedOptions extracts the (angzarr.v1.rejected) rpc declaration.
func rejectedOptions(m *protogen.Method, exts extensions) *Rejected {
	if exts.rejected == nil {
		return nil
	}
	opts := methodOptions(m)
	if opts == nil {
		return nil
	}
	reparsed := reparse(opts, &descriptorpb.MethodOptions{}, exts)
	if !proto.HasExtension(reparsed, exts.rejected) {
		return nil
	}
	msg, ok := proto.GetExtension(reparsed, exts.rejected).(protoreflect.Message)
	if !ok {
		dyn, ok := proto.GetExtension(reparsed, exts.rejected).(*dynamicpb.Message)
		if !ok {
			return nil
		}
		msg = dyn
	}
	return &Rejected{Command: stringField(msg, "command")}
}

// isApplier reports whether the rpc carries (angzarr.v1.applies) = true.
func isApplier(m *protogen.Method, exts extensions) bool {
	if exts.applies == nil {
		return false
	}
	opts := methodOptions(m)
	if opts == nil {
		return false
	}
	reparsed := reparse(opts, &descriptorpb.MethodOptions{}, exts)
	if !proto.HasExtension(reparsed, exts.applies) {
		return false
	}
	applies, _ := proto.GetExtension(reparsed, exts.applies).(bool)
	return applies
}

// reactsOptions extracts the (angzarr.v1.reacts) rpc declaration.
func reactsOptions(m *protogen.Method, exts extensions) *Reacts {
	if exts.reacts == nil {
		return nil
	}
	opts := methodOptions(m)
	if opts == nil {
		return nil
	}
	reparsed := reparse(opts, &descriptorpb.MethodOptions{}, exts)
	if !proto.HasExtension(reparsed, exts.reacts) {
		return nil
	}
	msg, ok := proto.GetExtension(reparsed, exts.reacts).(protoreflect.Message)
	if !ok {
		dyn, ok := proto.GetExtension(reparsed, exts.reacts).(*dynamicpb.Message)
		if !ok {
			return nil
		}
		msg = dyn
	}
	return &Reacts{Domain: stringField(msg, "domain")}
}

// ----------------------------------------------------------------------------
// Component model — what emitters consume
// ----------------------------------------------------------------------------

// Handler is one declared rpc, classified.
type Handler struct {
	Method *protogen.Method
	// Reacts is set on process-manager handler rpcs.
	Reacts *Reacts
}

// Rejection is one declared compensation rpc.
type Rejection struct {
	Method  *protogen.Method
	Command string // fully-qualified rejected command type
}

// Service is one validated component declaration ready for emission.
type Service struct {
	Proto      *protogen.Service
	Component  *Component
	Handlers   []Handler
	Appliers   []*protogen.Method
	Rejections []Rejection
	// State resolves Component.State in the compiled set (nil for sagas).
	State *protogen.Message
}

// messageRegistry indexes every message in the compiled set by full name
// so state/rejected references resolve (or fail generation).
func messageRegistry(gen *protogen.Plugin) map[string]*protogen.Message {
	registry := make(map[string]*protogen.Message)
	var walk func(msgs []*protogen.Message)
	walk = func(msgs []*protogen.Message) {
		for _, m := range msgs {
			registry[string(m.Desc.FullName())] = m
			walk(m.Messages)
		}
	}
	for _, f := range gen.Files {
		walk(f.Messages)
	}
	return registry
}

// validateFQ enforces the fully-qualified-name contract at generation time.
func validateFQ(registry map[string]*protogen.Message, name string) error {
	if name == "" {
		return fmt.Errorf("missing command type")
	}
	if _, ok := registry[name]; !ok {
		return fmt.Errorf("%q is not a fully-qualified message name in the compiled set (short names never match rejection dispatch)", name)
	}
	return nil
}

func resolveState(registry map[string]*protogen.Message, component *Component) (*protogen.Message, error) {
	if component.State == "" {
		return nil, fmt.Errorf("(angzarr.v1.component).state is required for %v", component.Kind)
	}
	state, ok := registry[component.State]
	if !ok {
		return nil, fmt.Errorf("state %q is not a fully-qualified message name in the compiled set", component.State)
	}
	return state, nil
}

// buildService classifies and validates one declared component.
func buildService(svc *protogen.Service, component *Component, registry map[string]*protogen.Message, exts extensions) (*Service, error) {
	s := &Service{Proto: svc, Component: component}
	for _, m := range svc.Methods {
		rejected := rejectedOptions(m, exts)
		switch {
		case rejected != nil:
			if err := validateFQ(registry, rejected.Command); err != nil {
				return nil, fmt.Errorf("rpc %s: (angzarr.v1.rejected).command: %w", m.Desc.Name(), err)
			}
			s.Rejections = append(s.Rejections, Rejection{Method: m, Command: rejected.Command})
		case isApplier(m, exts):
			s.Appliers = append(s.Appliers, m)
		default:
			s.Handlers = append(s.Handlers, Handler{Method: m, Reacts: reactsOptions(m, exts)})
		}
	}

	switch component.Kind {
	case KindSaga:
		if component.InputDomain == "" || component.OutputDomain == "" {
			return nil, fmt.Errorf("saga requires input_domain and output_domain")
		}
	case KindAggregate:
		if component.InputDomain == "" {
			return nil, fmt.Errorf("aggregate requires input_domain (its own domain)")
		}
		state, err := resolveState(registry, component)
		if err != nil {
			return nil, err
		}
		s.State = state
	case KindProcessManager:
		// output_domain is the PM's own domain and its command-target
		// domain in one declaration: it feeds the engine's pmDomain and
		// stamps emitted commands.
		if component.OutputDomain == "" {
			return nil, fmt.Errorf("process manager requires output_domain (its pm_domain and command-target domain)")
		}
		state, err := resolveState(registry, component)
		if err != nil {
			return nil, err
		}
		s.State = state
		for _, h := range s.Handlers {
			if h.Reacts.GetDomain() == "" {
				return nil, fmt.Errorf("rpc %s: process-manager handler rpcs require (angzarr.v1.reacts).domain", h.Method.Desc.Name())
			}
		}
	case KindProjector:
		// A projector must declare which domains it subscribes to — an
		// undeclared subscription would silently receive nothing.
		if component.InputDomain == "" {
			return nil, fmt.Errorf("projector requires input_domain (its subscribed domains)")
		}
		state, err := resolveState(registry, component)
		if err != nil {
			return nil, err
		}
		s.State = state
	default:
		return nil, fmt.Errorf("unsupported component kind %v", component.Kind)
	}
	return s, nil
}

// GetDomain is a nil-safe accessor mirroring generated-code conventions.
func (r *Reacts) GetDomain() string {
	if r == nil {
		return ""
	}
	return r.Domain
}
