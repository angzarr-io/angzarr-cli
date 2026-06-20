// Package codegen is the angzarr code generator core: it reads component
// declarations — messages carrying (io.angzarr.v1.component), with their
// commands/events carrying (io.angzarr.v1.command) / (io.angzarr.v1.event) —
// from a compiled descriptor set and emits per-language dispatch wiring
// through the registered emitters.
//
// Components are declared by ANNOTATING MESSAGES, not services: the anchor
// message is the event-sourced state (aggregate / process manager / projector)
// or an empty marker (the stateless saga); its commands and events are other
// messages pointing back at the anchor. There are no services and no rpcs.
//
// The options are extracted DYNAMICALLY from the request's own descriptor pool
// rather than through compiled bindings, so this module carries no generated
// proto code: clients can link it in-process without colliding with their own
// angzarr registrations, and the generator works against whatever
// options.proto revision the caller compiled. Matching is by extension NUMBER
// (not full name) so the declaration package is irrelevant — bindings compiled
// under io.angzarr.v1 and an older client-go under angzarr_client.proto.angzarr.v1
// resolve identically.
//
// Generation-time validation lives here, once, for every language: missing
// required component fields and unresolvable or short type references fail
// generation instead of producing wiring that silently never matches at
// runtime.
package codegen

import (
	"fmt"
	"strings"

	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/reflect/protodesc"
	"google.golang.org/protobuf/reflect/protoreflect"
	"google.golang.org/protobuf/reflect/protoregistry"
	"google.golang.org/protobuf/types/descriptorpb"
	"google.golang.org/protobuf/types/dynamicpb"
)

// Extension field numbers of the angzarr declaration options on
// google.protobuf.MessageOptions. These — with the option field names below —
// are the cross-language contract surface; their definitions live in
// angzarr-project's options.proto. Matching by number keeps the generator
// package-agnostic.
const (
	numComponent protoreflect.FieldNumber = 50100
	numCommand   protoreflect.FieldNumber = 50104
	numEvent     protoreflect.FieldNumber = 50105
)

// messageOptionsName is the message the angzarr options extend.
const messageOptionsName protoreflect.FullName = "google.protobuf.MessageOptions"

// ComponentKind identifies which dispatch component a message declares.
type ComponentKind int32

// Mirrors io.angzarr.v1.ComponentKind — the enum numbers are the wire contract.
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

// Component is the parsed (io.angzarr.v1.component) declaration.
type Component struct {
	Kind         ComponentKind
	InputDomain  string
	OutputDomain string
	// Name is the generated handler/dispatch base name; empty means default
	// to the anchor message name.
	Name string
	// Compensates is the fully-qualified command types whose rejection this
	// component compensates, in declaration order (C-0042).
	Compensates []string
}

// command is the parsed (io.angzarr.v1.command) declaration on a command msg.
type command struct {
	Component string   // anchor FQ name of the owning component
	Emits     []string // fully-qualified emitted event types (typed return)
}

// eventConsumer is one parsed entry of the repeated (io.angzarr.v1.event)
// option on an event message — one entry per consuming component.
type eventConsumer struct {
	Component string // anchor FQ name of the consuming component
	Domain    string // source domain (saga/projector filter, PM trigger source)
	Applies   bool   // PM only: fold into PM's own state vs. cross-domain trigger
}

// extensions holds the dynamically-resolved option extension types for one
// generation run.
type extensions struct {
	component protoreflect.ExtensionType
	command   protoreflect.ExtensionType
	event     protoreflect.ExtensionType
}

// resolveExtensions finds the angzarr option extensions in the request's own
// files by extension number on MessageOptions. A request whose declarations
// never import options.proto has nothing to generate — every lookup misses and
// the run emits nothing.
//
// The extension types are rebuilt against the process's own descriptor.proto
// rather than taken from protogen's universe: protobuf matches an extension's
// containing message by descriptor IDENTITY, and protogen rebuilds
// google.protobuf.MessageOptions from the request, so extension types parented
// there silently fail to attach when reparsing the (globally-typed) options
// messages.
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
			if d.ContainingMessage().FullName() != messageOptionsName {
				continue
			}
			switch d.Number() {
			case numComponent:
				exts.component = dynamicpb.NewExtensionType(d)
			case numCommand:
				exts.command = dynamicpb.NewExtensionType(d)
			case numEvent:
				exts.event = dynamicpb.NewExtensionType(d)
			}
		}
	}
	return exts
}

func hasAngzarrExtensions(file *protogen.File) bool {
	for _, ext := range file.Extensions {
		if ext.Desc.ContainingMessage().FullName() != messageOptionsName {
			continue
		}
		switch ext.Desc.Number() {
		case numComponent, numCommand, numEvent:
			return true
		}
	}
	return false
}

// dynamicTypes resolves only the angzarr extensions; everything else stays
// unknown, which is all the generator needs.
type dynamicTypes struct {
	exts extensions
}

func (t *dynamicTypes) all() []protoreflect.ExtensionType {
	return []protoreflect.ExtensionType{t.exts.component, t.exts.command, t.exts.event}
}

func (t *dynamicTypes) FindExtensionByName(field protoreflect.FullName) (protoreflect.ExtensionType, error) {
	for _, ext := range t.all() {
		if ext != nil && ext.TypeDescriptor().FullName() == field {
			return ext, nil
		}
	}
	return nil, fmt.Errorf("extension %s not part of the angzarr option surface", field)
}

func (t *dynamicTypes) FindExtensionByNumber(message protoreflect.FullName, field protoreflect.FieldNumber) (protoreflect.ExtensionType, error) {
	for _, ext := range t.all() {
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

// reparse re-decodes a message's options against the run's extension types so
// option bytes the global registry left unknown become readable fields.
func reparse(m *protogen.Message, exts extensions) protoreflect.Message {
	fresh := &descriptorpb.MessageOptions{}
	opts, _ := m.Desc.Options().(*descriptorpb.MessageOptions)
	if opts == nil {
		return fresh.ProtoReflect()
	}
	raw, err := proto.Marshal(opts)
	if err != nil {
		return fresh.ProtoReflect()
	}
	_ = proto.UnmarshalOptions{Resolver: &dynamicTypes{exts: exts}}.Unmarshal(raw, fresh)
	return fresh.ProtoReflect()
}

// reflString reads a string field from a dynamic option message by name.
func reflString(m protoreflect.Message, name string) string {
	fd := m.Descriptor().Fields().ByName(protoreflect.Name(name))
	if fd == nil {
		return ""
	}
	return m.Get(fd).String()
}

// reflBool reads a bool field from a dynamic option message by name.
func reflBool(m protoreflect.Message, name string) bool {
	fd := m.Descriptor().Fields().ByName(protoreflect.Name(name))
	if fd == nil {
		return false
	}
	return m.Get(fd).Bool()
}

// reflStrings reads a repeated string field from a dynamic option message.
func reflStrings(m protoreflect.Message, name string) []string {
	fd := m.Descriptor().Fields().ByName(protoreflect.Name(name))
	if fd == nil {
		return nil
	}
	list := m.Get(fd).List()
	out := make([]string, list.Len())
	for i := range out {
		out[i] = list.Get(i).String()
	}
	return out
}

// componentOptions extracts the (io.angzarr.v1.component) declaration off a
// message, or nil when absent / unspecified.
func componentOptions(m *protogen.Message, exts extensions) *Component {
	if exts.component == nil {
		return nil
	}
	opts := reparse(m, exts)
	fd := exts.component.TypeDescriptor()
	if !opts.Has(fd) {
		return nil
	}
	sub := opts.Get(fd).Message()
	c := &Component{
		InputDomain:  reflString(sub, "input_domain"),
		OutputDomain: reflString(sub, "output_domain"),
		Name:         reflString(sub, "name"),
		Compensates:  reflStrings(sub, "compensates"),
	}
	if kindFD := sub.Descriptor().Fields().ByName("kind"); kindFD != nil {
		c.Kind = ComponentKind(sub.Get(kindFD).Enum())
	}
	if c.Kind == KindUnspecified {
		return nil
	}
	return c
}

// commandOptions extracts the (io.angzarr.v1.command) declaration off a
// message, or nil when absent.
func commandOptions(m *protogen.Message, exts extensions) *command {
	if exts.command == nil {
		return nil
	}
	opts := reparse(m, exts)
	fd := exts.command.TypeDescriptor()
	if !opts.Has(fd) {
		return nil
	}
	sub := opts.Get(fd).Message()
	return &command{Component: reflString(sub, "component"), Emits: reflStrings(sub, "emits")}
}

// eventOptions extracts the repeated (io.angzarr.v1.event) entries off a
// message — one per consuming component.
func eventOptions(m *protogen.Message, exts extensions) []eventConsumer {
	if exts.event == nil {
		return nil
	}
	opts := reparse(m, exts)
	fd := exts.event.TypeDescriptor()
	if !opts.Has(fd) {
		return nil
	}
	list := opts.Get(fd).List()
	out := make([]eventConsumer, 0, list.Len())
	for i := 0; i < list.Len(); i++ {
		e := list.Get(i).Message()
		out = append(out, eventConsumer{
			Component: reflString(e, "component"),
			Domain:    reflString(e, "domain"),
			Applies:   reflBool(e, "applies"),
		})
	}
	return out
}

// ----------------------------------------------------------------------------
// Component model — what emitters consume
// ----------------------------------------------------------------------------

// Handler is one command or trigger-event handler method.
type Handler struct {
	// Message is the typed command (aggregate) or event (saga / projector /
	// process-manager trigger) the handler receives.
	Message *protogen.Message
	// MethodName is the generated Go method name (the message's Go name).
	MethodName string
	// SourceDomain is the event source domain on saga / projector /
	// process-manager trigger handlers; empty for aggregate command handlers.
	SourceDomain string
	// Emits is the resolved typed-emit event messages for an aggregate command
	// handler. When it holds exactly one type the handler returns that typed
	// slice and the wiring builds the EventBook; otherwise the handler returns
	// the raw EventBook escape hatch.
	Emits []*protogen.Message
}

// TypedEmit reports whether this command handler returns a typed event slice
// (exactly one declared emit type) rather than a raw EventBook.
func (h Handler) TypedEmit() bool {
	return len(h.Emits) == 1
}

// Applier folds one event into the rebuilding state (aggregate, or a process
// manager's own state).
type Applier struct {
	Message    *protogen.Message
	MethodName string
}

// Rejection is one declared compensation.
type Rejection struct {
	Command    string // fully-qualified rejected command type
	MethodName string // On<ShortCommand>Rejected
}

// Service is one validated component declaration ready for emission.
type Service struct {
	// Anchor is the message carrying (component): the state message for the
	// stateful kinds, or an empty marker for the saga.
	Anchor *protogen.Message
	// GoName is the generated handler/dispatch base name.
	GoName     string
	Component  *Component
	Handlers   []Handler
	Appliers   []Applier
	Rejections []Rejection
	// State is the anchor message for stateful kinds; nil for the saga.
	State *protogen.Message
}

// messageRegistry indexes every message in the compiled set by full name so
// component / command / event references resolve (or fail generation).
func messageRegistry(gen *protogen.Plugin) map[string]*protogen.Message {
	registry := make(map[string]*protogen.Message)
	for _, f := range gen.Files {
		for _, m := range allMessages(f.Messages) {
			registry[string(m.Desc.FullName())] = m
		}
	}
	return registry
}

// allMessages flattens a message tree in declaration order (nested messages
// after their parent), giving deterministic generated output.
func allMessages(msgs []*protogen.Message) []*protogen.Message {
	var out []*protogen.Message
	for _, m := range msgs {
		out = append(out, m)
		out = append(out, allMessages(m.Messages)...)
	}
	return out
}

// applierName is the generated method name for an event applier: the event's
// Go name prefixed with "Apply". The prefix keeps an applier distinct from a
// handler for the SAME event — a process manager folds an event into its own
// state (applier) AND reacts to it (handler), so the two methods must not
// collide.
func applierName(m *protogen.Message) string {
	return "Apply" + m.GoIdent.GoName
}

// shortName returns the trailing segment of a fully-qualified type name.
func shortName(fq string) string {
	if i := strings.LastIndex(fq, "."); i >= 0 {
		return fq[i+1:]
	}
	return fq
}

// fileServices is one generated file's components.
type fileServices struct {
	File     *protogen.File
	Services []*Service
}

// baseName resolves the component's generated base name: the declared name, or
// the anchor message's own name.
func baseName(m *protogen.Message, c *Component) string {
	if c.Name != "" {
		return c.Name
	}
	return string(m.Desc.Name())
}
