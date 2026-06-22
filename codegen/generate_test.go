package codegen_test

// These tests build descriptor sets entirely in memory — including the
// angzarr options file itself — proving the generator reads declarations
// dynamically with no compiled options bindings, that it matches the
// component/command/event extensions BY NUMBER (so the declaration package is
// irrelevant), and that every generation-time validation fires
// language-independently.

import (
	"sort"
	"strings"
	"testing"

	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/reflect/protodesc"
	"google.golang.org/protobuf/reflect/protoreflect"
	"google.golang.org/protobuf/reflect/protoregistry"
	"google.golang.org/protobuf/types/descriptorpb"
	"google.golang.org/protobuf/types/dynamicpb"
	"google.golang.org/protobuf/types/pluginpb"

	"github.com/angzarr-io/angzarr-cli/codegen"
)

// optionsPath is the import path the test proto declares; the options package
// is parameterized to prove number-based (package-agnostic) matching.
const (
	optionsPath = "io/angzarr/v1/options.proto"
	ioPkg       = "io.angzarr.v1"
	legacyPkg   = "angzarr_client.proto.angzarr.v1"
	testPath    = "validation_test.proto"
	testPkg     = "validation.test"
)

func str(s string) *string { return &s }
func i32(i int32) *int32   { return &i }

func enumValue(name string, number int32) *descriptorpb.EnumValueDescriptorProto {
	return &descriptorpb.EnumValueDescriptorProto{Name: str(name), Number: i32(number)}
}

func field(name string, number int32, typ descriptorpb.FieldDescriptorProto_Type) *descriptorpb.FieldDescriptorProto {
	return &descriptorpb.FieldDescriptorProto{
		Name:     str(name),
		Number:   i32(number),
		Label:    descriptorpb.FieldDescriptorProto_LABEL_OPTIONAL.Enum(),
		Type:     typ.Enum(),
		JsonName: str(name),
	}
}

func repeatedField(name string, number int32, typ descriptorpb.FieldDescriptorProto_Type) *descriptorpb.FieldDescriptorProto {
	f := field(name, number, typ)
	f.Label = descriptorpb.FieldDescriptorProto_LABEL_REPEATED.Enum()
	return f
}

func enumField(name string, number int32, enumPkg string) *descriptorpb.FieldDescriptorProto {
	f := field(name, number, descriptorpb.FieldDescriptorProto_TYPE_ENUM)
	f.TypeName = str("." + enumPkg + ".ComponentKind")
	return f
}

func messageField(name string, number int32, msgPkg, msgName string) *descriptorpb.FieldDescriptorProto {
	f := field(name, number, descriptorpb.FieldDescriptorProto_TYPE_MESSAGE)
	f.TypeName = str("." + msgPkg + "." + msgName)
	return f
}

// optionsFDP reconstructs options.proto as a descriptor under pkg: the
// ComponentKind enum, the three option messages, and the three
// MessageOptions extensions (component=50100, command=50104, event=50105).
func optionsFDP(pkg string) *descriptorpb.FileDescriptorProto {
	str_ := descriptorpb.FieldDescriptorProto_TYPE_STRING
	bool_ := descriptorpb.FieldDescriptorProto_TYPE_BOOL
	return &descriptorpb.FileDescriptorProto{
		Name:       str(optionsPath),
		Package:    str(pkg),
		Syntax:     str("proto3"),
		Dependency: []string{"google/protobuf/descriptor.proto"},
		// protogen requires go_package on every request file, whatever
		// language is being emitted.
		Options: &descriptorpb.FileOptions{GoPackage: str("example.test/angzarrpb;angzarrpb")},
		EnumType: []*descriptorpb.EnumDescriptorProto{{
			Name: str("ComponentKind"),
			Value: []*descriptorpb.EnumValueDescriptorProto{
				enumValue("COMPONENT_KIND_UNSPECIFIED", 0),
				enumValue("COMPONENT_KIND_AGGREGATE", 1),
				enumValue("COMPONENT_KIND_SAGA", 2),
				enumValue("COMPONENT_KIND_PROCESS_MANAGER", 3),
				enumValue("COMPONENT_KIND_PROJECTOR", 4),
			},
		}},
		MessageType: []*descriptorpb.DescriptorProto{
			{
				Name: str("ComponentOptions"),
				Field: []*descriptorpb.FieldDescriptorProto{
					enumField("kind", 1, pkg),
					field("input_domain", 2, str_),
					field("output_domain", 3, str_),
					field("name", 4, str_),
					repeatedField("compensates", 5, str_),
				},
			},
			{
				Name: str("CommandOptions"),
				Field: []*descriptorpb.FieldDescriptorProto{
					field("component", 1, str_),
					repeatedField("emits", 2, str_),
				},
			},
			{
				Name: str("EventConsumer"),
				Field: []*descriptorpb.FieldDescriptorProto{
					field("component", 1, str_),
					field("domain", 2, str_),
					field("applies", 3, bool_),
				},
			},
		},
		Extension: []*descriptorpb.FieldDescriptorProto{
			func() *descriptorpb.FieldDescriptorProto {
				f := messageField("component", 50100, pkg, "ComponentOptions")
				f.Extendee = str(".google.protobuf.MessageOptions")
				return f
			}(),
			func() *descriptorpb.FieldDescriptorProto {
				f := messageField("command", 50104, pkg, "CommandOptions")
				f.Extendee = str(".google.protobuf.MessageOptions")
				return f
			}(),
			func() *descriptorpb.FieldDescriptorProto {
				f := messageField("event", 50105, pkg, "EventConsumer")
				f.Label = descriptorpb.FieldDescriptorProto_LABEL_REPEATED.Enum()
				f.Extendee = str(".google.protobuf.MessageOptions")
				return f
			}(),
		},
	}
}

// optionTypes materializes the extensions and option messages so tests can
// stamp declarations onto messages — through the same dynamic machinery the
// generator itself uses.
type optionTypes struct {
	component, command, event          protoreflect.ExtensionType
	componentMsg, commandMsg, eventMsg protoreflect.MessageType
}

func buildOptionTypes(t *testing.T, pkg string) optionTypes {
	t.Helper()
	files := &protoregistry.Files{}
	if err := files.RegisterFile(descriptorpb.File_google_protobuf_descriptor_proto); err != nil {
		t.Fatalf("register descriptor.proto: %v", err)
	}
	fd, err := protodesc.NewFile(optionsFDP(pkg), files)
	if err != nil {
		t.Fatalf("build options.proto descriptor: %v", err)
	}
	exts := fd.Extensions()
	msgs := fd.Messages()
	findExt := func(name string) protoreflect.ExtensionType {
		for i := 0; i < exts.Len(); i++ {
			if string(exts.Get(i).Name()) == name {
				return dynamicpb.NewExtensionType(exts.Get(i))
			}
		}
		t.Fatalf("extension %q not found", name)
		return nil
	}
	findMsg := func(name string) protoreflect.MessageType {
		d := msgs.ByName(protoreflect.Name(name))
		if d == nil {
			t.Fatalf("message %q not found", name)
		}
		return dynamicpb.NewMessageType(d)
	}
	return optionTypes{
		component:    findExt("component"),
		command:      findExt("command"),
		event:        findExt("event"),
		componentMsg: findMsg("ComponentOptions"),
		commandMsg:   findMsg("CommandOptions"),
		eventMsg:     findMsg("EventConsumer"),
	}
}

// componentDecl stamps an (io.angzarr.v1.component) onto a fresh MessageOptions.
func (o optionTypes) componentDecl(kind int32, inputDomain, outputDomain, name string, compensates ...string) *descriptorpb.MessageOptions {
	sub := o.componentMsg.New()
	f := sub.Descriptor().Fields()
	sub.Set(f.ByName("kind"), protoreflect.ValueOfEnum(protoreflect.EnumNumber(kind)))
	if inputDomain != "" {
		sub.Set(f.ByName("input_domain"), protoreflect.ValueOfString(inputDomain))
	}
	if outputDomain != "" {
		sub.Set(f.ByName("output_domain"), protoreflect.ValueOfString(outputDomain))
	}
	if name != "" {
		sub.Set(f.ByName("name"), protoreflect.ValueOfString(name))
	}
	if len(compensates) > 0 {
		list := sub.Mutable(f.ByName("compensates")).List()
		for _, c := range compensates {
			list.Append(protoreflect.ValueOfString(c))
		}
	}
	opts := &descriptorpb.MessageOptions{}
	opts.ProtoReflect().Set(o.component.TypeDescriptor(), protoreflect.ValueOfMessage(sub))
	return opts
}

// commandDecl stamps an (io.angzarr.v1.command) onto a fresh MessageOptions.
func (o optionTypes) commandDecl(component string, emits ...string) *descriptorpb.MessageOptions {
	sub := o.commandMsg.New()
	f := sub.Descriptor().Fields()
	sub.Set(f.ByName("component"), protoreflect.ValueOfString(component))
	if len(emits) > 0 {
		list := sub.Mutable(f.ByName("emits")).List()
		for _, e := range emits {
			list.Append(protoreflect.ValueOfString(e))
		}
	}
	opts := &descriptorpb.MessageOptions{}
	opts.ProtoReflect().Set(o.command.TypeDescriptor(), protoreflect.ValueOfMessage(sub))
	return opts
}

// eventEntry is one (io.angzarr.v1.event) consumer entry.
type eventEntry struct {
	component string
	domain    string
	applies   bool
}

// eventDecl stamps one or more repeated (io.angzarr.v1.event) entries onto a
// fresh MessageOptions.
func (o optionTypes) eventDecl(entries ...eventEntry) *descriptorpb.MessageOptions {
	opts := &descriptorpb.MessageOptions{}
	list := opts.ProtoReflect().Mutable(o.event.TypeDescriptor()).List()
	for _, e := range entries {
		ev := o.eventMsg.New()
		f := ev.Descriptor().Fields()
		ev.Set(f.ByName("component"), protoreflect.ValueOfString(e.component))
		if e.domain != "" {
			ev.Set(f.ByName("domain"), protoreflect.ValueOfString(e.domain))
		}
		if e.applies {
			ev.Set(f.ByName("applies"), protoreflect.ValueOfBool(true))
		}
		list.Append(protoreflect.ValueOfMessage(ev))
	}
	return opts
}

// declMsg is a test message and its angzarr declaration option.
type declMsg struct {
	name string
	opts *descriptorpb.MessageOptions
}

func fq(name string) string { return testPkg + "." + name }

// buildGen assembles a protogen.Plugin over a descriptor set carrying the
// given declared messages, with options.proto declared under optionsPkg.
func buildGen(t *testing.T, optionsPkg string, msgs ...declMsg) (*protogen.Plugin, error) {
	t.Helper()
	var messageTypes []*descriptorpb.DescriptorProto
	for _, m := range msgs {
		messageTypes = append(messageTypes, &descriptorpb.DescriptorProto{Name: str(m.name), Options: m.opts})
	}
	testFile := &descriptorpb.FileDescriptorProto{
		Name:        str(testPath),
		Package:     str(testPkg),
		Syntax:      str("proto3"),
		Dependency:  []string{optionsPath},
		Options:     &descriptorpb.FileOptions{GoPackage: str("example.test/validation;validationtest")},
		MessageType: messageTypes,
	}
	return protogen.Options{}.New(&pluginpb.CodeGeneratorRequest{
		FileToGenerate: []string{testPath},
		ProtoFile: []*descriptorpb.FileDescriptorProto{
			protodesc.ToFileDescriptorProto(descriptorpb.File_google_protobuf_descriptor_proto),
			optionsFDP(optionsPkg),
			testFile,
		},
	})
}

// generate runs the real generator over a descriptor set carrying the given
// declared messages, with options.proto declared under optionsPkg.
func generate(t *testing.T, lang, optionsPkg string, msgs ...declMsg) (*pluginpb.CodeGeneratorResponse, error) {
	t.Helper()
	gen, err := buildGen(t, optionsPkg, msgs...)
	if err != nil {
		return nil, err
	}
	if err := codegen.Generate(gen, lang, codegen.Options{}); err != nil {
		return nil, err
	}
	return gen.Response(), nil
}

// scaffold runs the scaffold generator, skipping stubs for which exists
// reports true.
func scaffold(t *testing.T, lang, optionsPkg string, exists func(string) bool, msgs ...declMsg) (*pluginpb.CodeGeneratorResponse, error) {
	t.Helper()
	gen, err := buildGen(t, optionsPkg, msgs...)
	if err != nil {
		return nil, err
	}
	if err := codegen.GenerateScaffold(gen, lang, exists, codegen.Options{}); err != nil {
		return nil, err
	}
	return gen.Response(), nil
}

// orderAggregate is the canonical valid aggregate: a CreateOrder command
// (typed-emit of OrderCreated) and an OrderCreated applier, anchored on State.
func orderAggregate(o optionTypes) []declMsg {
	return []declMsg{
		{"State", o.componentDecl(1, "orders", "", "OrderAggregate")},
		{"CreateOrder", o.commandDecl(fq("State"), fq("OrderCreated"))},
		{"OrderCreated", o.eventDecl(eventEntry{component: fq("State")})},
	}
}

func TestGenerate_ValidAggregate_EmitsStrictSeam(t *testing.T) {
	// Both packages must produce the same seam: matching is by extension
	// number, so the declaration package is irrelevant (the KEY FIX).
	for _, pkg := range []string{ioPkg, legacyPkg} {
		t.Run(pkg, func(t *testing.T) {
			o := buildOptionTypes(t, pkg)
			resp, err := generate(t, "go", pkg, orderAggregate(o)...)
			if err != nil {
				t.Fatalf("Generate: %v", err)
			}
			if len(resp.File) != 1 {
				t.Fatalf("generated %d files, want 1", len(resp.File))
			}
			content := resp.File[0].GetContent()
			for _, want := range []string{
				"type OrderAggregateHandler interface",
				"CreateOrder(cmd *",         // command handler, method = command msg name
				"([]*",                      // typed-emit: returns the emitted event slice
				"ApplyOrderCreated(state *", // applier, method = Apply + event msg name
				"func NewOrderAggregateDispatch(",
				"rebuilder.WithSnapshot(", // snapshot loader for stateful kinds
				`OnCommand("validation.test.CreateOrder"`,
				`Apply("validation.test.OrderCreated"`,
				"func RegisterOrderAggregate(",
			} {
				if !strings.Contains(content, want) {
					t.Errorf("generated file missing %q", want)
				}
			}
		})
	}
}

func TestGenerate_RawEventBookEscapeHatch(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	// A command with no declared emits returns the raw EventBook.
	resp, err := generate(t, "go", ioPkg,
		declMsg{"State", o.componentDecl(1, "orders", "", "OrderAggregate")},
		declMsg{"CreateOrder", o.commandDecl(fq("State"))},
	)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	content := resp.File[0].GetContent()
	if !strings.Contains(content, "EventBook, error)") {
		t.Errorf("escape-hatch handler should return a raw EventBook; got:\n%s", content)
	}
	if strings.Contains(content, "Pack(ev)") {
		t.Errorf("escape-hatch handler must not build an EventBook from typed events")
	}
}

func TestGenerate_ValidSaga_EmitsMethodRegister(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "go", ioPkg,
		declMsg{"OrderSaga", o.componentDecl(2, "orders", "fulfillment", "")},
		declMsg{"OrderPlaced", o.eventDecl(eventEntry{component: fq("OrderSaga"), domain: "orders"})},
	)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	content := resp.File[0].GetContent()
	for _, want := range []string{
		"type OrderSagaHandler interface",
		"func NewOrderSagaDispatch(",
		"r.RegisterSaga(NewOrderSagaDispatch(h))", // saga registers via a method
		`OnEvent("validation.test.OrderPlaced"`,
	} {
		if !strings.Contains(content, want) {
			t.Errorf("generated file missing %q", want)
		}
	}
}

func TestGenerate_PMSameEventApplierAndTrigger_NoMethodCollision(t *testing.T) {
	// A process manager folds an event into its OWN state (applies) AND reacts
	// to it (trigger). Both derive from the same event, so the applier must be
	// renamed (Apply<Event>) to avoid a duplicate interface method.
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "go", ioPkg,
		declMsg{"PMState", o.componentDecl(3, "", "fulfillment", "")},
		declMsg{"Trig", o.eventDecl(
			eventEntry{component: fq("PMState"), applies: true},
			eventEntry{component: fq("PMState"), domain: "orders"},
		)},
	)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	content := resp.File[0].GetContent()
	if !strings.Contains(content, "Trig(event *") {
		t.Errorf("missing PM trigger handler method Trig")
	}
	if !strings.Contains(content, "ApplyTrig(state *") {
		t.Errorf("missing PM applier method ApplyTrig (collision not avoided)")
	}
	// Belt-and-suspenders: the generated Go must compile, so the interface must
	// not declare the same method name twice.
	if strings.Count(content, "\tTrig(") > 1 {
		t.Errorf("duplicate Trig method in generated interface")
	}
}

func TestGeneratePython_EmitsProtocolSeam(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "python", ioPkg, orderAggregate(o)...)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	if len(resp.File) != 1 {
		t.Fatalf("generated %d files, want 1", len(resp.File))
	}
	f := resp.File[0]
	if !strings.HasSuffix(f.GetName(), "_angzarr.py") {
		t.Errorf("wiring file name = %q, want *_angzarr.py", f.GetName())
	}
	content := f.GetContent()
	for _, want := range []string{
		"import angzarr_router_ffi as _az",
		"class OrderAggregateHandler(Protocol):",
		"def create_order(self, cmd: _m0.CreateOrder, state: _m0.State, cctx: _az.CommandContext) -> list[_m0.OrderCreated]: ...",
		"def apply_order_created(self, state: _m0.State, event: _m0.OrderCreated) -> None: ...",
		"def new_order_aggregate_dispatch(handler: OrderAggregateHandler) -> _az.AggregateDispatch:",
		`dispatch.on_command("validation.test.CreateOrder"`,
		"book.pages.add().event.CopyFrom(_az.pack(ev))", // typed-emit
		`raise _az.any_decode_error(cmd_any.type_url, exc)`,
		"def register_order_aggregate(router: _az.Router, handler: OrderAggregateHandler) -> None:",
		"router.register_aggregate(new_order_aggregate_dispatch(handler))",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("python wiring missing %q", want)
		}
	}
}

func TestGeneratePython_SagaUsesMethodRegister(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "python", ioPkg,
		declMsg{"OrderSaga", o.componentDecl(2, "orders", "fulfillment", "")},
		declMsg{"OrderPlaced", o.eventDecl(eventEntry{component: fq("OrderSaga"), domain: "orders"})},
	)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	content := resp.File[0].GetContent()
	for _, want := range []string{
		`_az.SagaDispatch("OrderSaga", "orders", targets=["fulfillment"])`,
		`dispatch.on_event("validation.test.OrderPlaced"`,
		"router.register_saga(new_order_saga_dispatch(handler))",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("python saga wiring missing %q", want)
		}
	}
}

func TestGeneratePython_RawEventBookEscapeHatch(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "python", ioPkg,
		declMsg{"State", o.componentDecl(1, "orders", "", "OrderAggregate")},
		declMsg{"CreateOrder", o.commandDecl(fq("State"))},
	)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	content := resp.File[0].GetContent()
	if !strings.Contains(content, "-> Optional[_t.EventBook]") {
		t.Errorf("escape-hatch handler should return Optional[EventBook]")
	}
	if strings.Contains(content, "_az.pack(") {
		t.Errorf("escape-hatch handler must not pack typed events")
	}
}

func TestGeneratePythonScaffold_EmitsOwnedStub(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := scaffold(t, "python", ioPkg, func(string) bool { return false }, orderAggregate(o)...)
	if err != nil {
		t.Fatalf("GenerateScaffold: %v", err)
	}
	if len(resp.File) != 1 || !strings.HasSuffix(resp.File[0].GetName(), "_angzarr_handler.py") {
		t.Fatalf("want one *_angzarr_handler.py file, got %v", resp.File)
	}
	content := resp.File[0].GetContent()
	for _, want := range []string{
		"Regeneration will NOT overwrite this file",
		"class OrderAggregate:",
		"def create_order(self, cmd: _m0.CreateOrder",
		`raise NotImplementedError("TODO: implement OrderAggregate.create_order")`,
	} {
		if !strings.Contains(content, want) {
			t.Errorf("python scaffold missing %q", want)
		}
	}
}

func TestGenerateScaffold_EmitsOwnedStub(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	// exists never fires: a fresh project gets its stub.
	resp, err := scaffold(t, "go", ioPkg, func(string) bool { return false }, orderAggregate(o)...)
	if err != nil {
		t.Fatalf("GenerateScaffold: %v", err)
	}
	if len(resp.File) != 1 {
		t.Fatalf("scaffolded %d files, want 1", len(resp.File))
	}
	f := resp.File[0]
	if !strings.HasSuffix(f.GetName(), "_angzarr_handler.go") {
		t.Errorf("scaffold file name = %q, want *_angzarr_handler.go suffix", f.GetName())
	}
	content := f.GetContent()
	for _, want := range []string{
		"Regeneration will NOT overwrite this file",
		"type OrderAggregate struct{}",
		"var _ OrderAggregateHandler = OrderAggregate{}",
		"func (OrderAggregate) CreateOrder(",
		`panic("TODO: implement OrderAggregate.CreateOrder")`,
		"func (OrderAggregate) ApplyOrderCreated(", // applier stub
	} {
		if !strings.Contains(content, want) {
			t.Errorf("scaffold missing %q", want)
		}
	}
	// An applier returns nothing, so its stub is a TODO comment, not a panic.
	if strings.Contains(content, `panic("TODO: implement OrderAggregate.OrderCreated")`) {
		t.Errorf("no-result applier stub should not panic")
	}
}

func TestGenerateScaffold_SkipsExistingStub(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	// exists always fires: the developer-owned stub is preserved (nothing emitted).
	resp, err := scaffold(t, "go", ioPkg, func(string) bool { return true }, orderAggregate(o)...)
	if err != nil {
		t.Fatalf("GenerateScaffold: %v", err)
	}
	if len(resp.File) != 0 {
		t.Fatalf("scaffolded %d files over an existing stub, want 0", len(resp.File))
	}
}

func TestGenerate_Validations_FailGeneration(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	tests := []struct {
		name string
		msgs []declMsg
	}{
		{"aggregate without input domain", []declMsg{
			{"State", o.componentDecl(1, "", "", "")},
			{"CreateOrder", o.commandDecl(fq("State"))},
		}},
		{"saga without output domain", []declMsg{
			{"OrderSaga", o.componentDecl(2, "orders", "", "")},
		}},
		{"process manager without output domain", []declMsg{
			{"PMState", o.componentDecl(3, "", "", "")},
		}},
		{"process manager trigger without domain", []declMsg{
			{"PMState", o.componentDecl(3, "", "fulfillment", "")},
			{"Trig", o.eventDecl(eventEntry{component: fq("PMState")})},
		}},
		{"projector without domains", []declMsg{
			{"ProjState", o.componentDecl(4, "", "", "")},
		}},
		{"command to unknown component", []declMsg{
			{"CreateOrder", o.commandDecl(fq("Nope"))},
		}},
		{"command emits unresolvable", []declMsg{
			{"State", o.componentDecl(1, "orders", "", "")},
			{"CreateOrder", o.commandDecl(fq("State"), fq("Nope"))},
		}},
		{"event to unknown component", []declMsg{
			{"OrderCreated", o.eventDecl(eventEntry{component: fq("Nope")})},
		}},
		{"compensates unresolvable", []declMsg{
			{"State", o.componentDecl(1, "orders", "", "", fq("Nope"))},
		}},
		{"command handled by non-aggregate", []declMsg{
			{"OrderSaga", o.componentDecl(2, "orders", "fulfillment", "")},
			{"CreateOrder", o.commandDecl(fq("OrderSaga"))},
		}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if _, err := generate(t, "go", ioPkg, tt.msgs...); err == nil {
				t.Fatal("expected generation to fail")
			}
		})
	}
}

func TestGenerateJava_EmitsNestedSeam(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "java", ioPkg, orderAggregate(o)...)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	if len(resp.File) != 1 {
		t.Fatalf("generated %d files, want 1", len(resp.File))
	}
	f := resp.File[0]
	if !strings.HasSuffix(f.GetName(), "/OrderAggregateAngzarr.java") {
		t.Errorf("wiring file name = %q, want */OrderAggregateAngzarr.java", f.GetName())
	}
	content := f.GetContent()
	// The test proto is validation_test.proto, package validation.test, no java
	// options → messages nest in the derived outer class ValidationTest. One file
	// per component: the public class is <Component>Angzarr.
	for _, want := range []string{
		"package validation.test;",
		"public final class OrderAggregateAngzarr {",
		"public interface OrderAggregateHandler {",
		// command handler: method = lowerFirst(message name), typed-emit return
		"java.util.List<validation.test.ValidationTest.OrderCreated> createOrder(",
		"validation.test.ValidationTest.CreateOrder cmd",
		"validation.test.ValidationTest.State.Builder state, io.angzarr.router.CommandContext cctx) throws Exception;",
		"void applyOrderCreated(validation.test.ValidationTest.State.Builder state, validation.test.ValidationTest.OrderCreated event);",
		"public static io.angzarr.router.AggregateDispatch newOrderAggregateDispatch(OrderAggregateHandler h) {",
		"new io.angzarr.router.Rebuilder(validation.test.ValidationTest.State::newBuilder)",
		"rebuilder.withSnapshot(",
		`.onCommand("validation.test.CreateOrder"`,
		`rebuilder.apply("validation.test.OrderCreated"`,
		"io.angzarr.EventPage.newBuilder().setEvent(io.angzarr.router.Pack.pack(ev))",
		"public static void registerOrderAggregate(io.angzarr.router.Router r, OrderAggregateHandler h) {",
		"r.registerAggregate(newOrderAggregateDispatch(h));",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("java wiring missing %q\n---\n%s", want, content)
		}
	}
}

func TestGenerateJava_SagaUsesMethodRegisterAndTargets(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "java", ioPkg,
		declMsg{"OrderSaga", o.componentDecl(2, "orders", "fulfillment", "")},
		declMsg{"OrderPlaced", o.eventDecl(eventEntry{component: fq("OrderSaga"), domain: "orders"})},
	)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	content := resp.File[0].GetContent()
	for _, want := range []string{
		"public interface OrderSagaHandler {",
		`new io.angzarr.router.SagaDispatch("OrderSaga", "orders", java.util.List.of("fulfillment"))`,
		`.onEvent("validation.test.OrderPlaced"`,
		"r.registerSaga(newOrderSagaDispatch(h));",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("java saga wiring missing %q\n---\n%s", want, content)
		}
	}
}

func TestGenerateJava_RawEventBookEscapeHatch(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "java", ioPkg,
		declMsg{"State", o.componentDecl(1, "orders", "", "OrderAggregate")},
		declMsg{"CreateOrder", o.commandDecl(fq("State"))},
	)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	content := resp.File[0].GetContent()
	if !strings.Contains(content, "io.angzarr.EventBook createOrder(") {
		t.Errorf("escape-hatch handler should return a raw EventBook; got:\n%s", content)
	}
	if strings.Contains(content, "Pack.pack(ev)") {
		t.Errorf("escape-hatch handler must not build an EventBook from typed events")
	}
}

func TestGenerateCSharp_EmitsNestedSeam(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "csharp", ioPkg, orderAggregate(o)...)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	if len(resp.File) != 1 {
		t.Fatalf("generated %d files, want 1", len(resp.File))
	}
	f := resp.File[0]
	if !strings.HasSuffix(f.GetName(), "/OrderAggregateAngzarr.cs") {
		t.Errorf("wiring file name = %q, want */OrderAggregateAngzarr.cs", f.GetName())
	}
	content := f.GetContent()
	// validation_test.proto, package validation.test, no csharp_namespace →
	// derived namespace Validation.Test; messages are top-level (no .Types.). One
	// file per component: the public static class is <Component>Angzarr.
	for _, want := range []string{
		"namespace Validation.Test;",
		"public static class OrderAggregateAngzarr",
		"public interface OrderAggregateHandler",
		// command handler: method = message name (PascalCase), typed-emit return
		"System.Collections.Generic.IReadOnlyList<Validation.Test.OrderCreated> CreateOrder(",
		"Validation.Test.CreateOrder cmd, Validation.Test.State state, Angzarr.Router.CommandContext cctx)",
		// applier: state is the mutable message itself (no Builder); ev param
		"void ApplyOrderCreated(Validation.Test.State state, Validation.Test.OrderCreated ev)",
		// dispatch surfaces are generic in the state message → cast-free wiring
		"public static Angzarr.Router.AggregateDispatch<Validation.Test.State> NewOrderAggregateDispatch(OrderAggregateHandler h)",
		"var rebuilder = new Angzarr.Router.Rebuilder<Validation.Test.State>(() => new Validation.Test.State());",
		"rebuilder.WithSnapshot((state, payload) => Google.Protobuf.MessageExtensions.MergeFrom(state, payload.Value));",
		`.OnCommand("validation.test.CreateOrder"`,
		`rebuilder.Apply("validation.test.OrderCreated"`,
		"var events = h.CreateOrder(cmd, state, cctx);",
		"book.Pages.Add(new Angzarr.EventPage { Event = Angzarr.Router.Pack.Wrap(ev) });",
		"public static void RegisterOrderAggregate(Angzarr.Router.Router r, OrderAggregateHandler h)",
		"r.RegisterAggregate(NewOrderAggregateDispatch(h));",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("csharp wiring missing %q\n---\n%s", want, content)
		}
	}
	// The generic dispatch makes state typed, so no (State)state casts leak into
	// the generated wiring.
	if strings.Contains(content, "(Validation.Test.State)state") {
		t.Errorf("generic wiring must be cast-free; found a (State)state cast\n---\n%s", content)
	}
}

func TestGenerateCSharp_SagaUsesMethodRegisterAndTargets(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "csharp", ioPkg,
		declMsg{"OrderSaga", o.componentDecl(2, "orders", "fulfillment", "")},
		declMsg{"OrderPlaced", o.eventDecl(eventEntry{component: fq("OrderSaga"), domain: "orders"})},
	)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	content := resp.File[0].GetContent()
	for _, want := range []string{
		"public interface OrderSagaHandler",
		`new Angzarr.Router.SagaDispatch("OrderSaga", "orders", "fulfillment")`,
		`.OnEvent("validation.test.OrderPlaced"`,
		"r.RegisterSaga(NewOrderSagaDispatch(h));",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("csharp saga wiring missing %q\n---\n%s", want, content)
		}
	}
}

func TestGenerateCSharp_RawEventBookEscapeHatch(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "csharp", ioPkg,
		declMsg{"State", o.componentDecl(1, "orders", "", "OrderAggregate")},
		declMsg{"CreateOrder", o.commandDecl(fq("State"))},
	)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	content := resp.File[0].GetContent()
	if !strings.Contains(content, "Angzarr.EventBook CreateOrder(") {
		t.Errorf("escape-hatch handler should return a raw EventBook; got:\n%s", content)
	}
	if strings.Contains(content, "Pack.Pack(ev)") {
		t.Errorf("escape-hatch handler must not build an EventBook from typed events")
	}
}

func TestGenerateCpp_EmitsNestedSeam(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "cpp", ioPkg, orderAggregate(o)...)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	if len(resp.File) != 1 {
		t.Fatalf("generated %d files, want 1", len(resp.File))
	}
	f := resp.File[0]
	if !strings.HasSuffix(f.GetName(), "_angzarr.h") {
		t.Errorf("wiring file name = %q, want *_angzarr.h", f.GetName())
	}
	content := f.GetContent()
	// validation_test.proto, package validation.test → namespace validation::test.
	for _, want := range []string{
		"#pragma once",
		"namespace validation::test {",
		"class OrderAggregateHandler {",
		"virtual ~OrderAggregateHandler() = default;",
		// command handler: typed-emit return, const-ref command, ref state
		"virtual std::vector<validation::test::OrderCreated> CreateOrder(const validation::test::CreateOrder& cmd, validation::test::State& state, const angzarr::router::CommandContext& cctx) = 0;",
		"virtual void ApplyOrderCreated(validation::test::State& state, const validation::test::OrderCreated& ev) = 0;",
		"inline angzarr::router::AggregateDispatch<validation::test::State> NewOrderAggregateDispatch(OrderAggregateHandler& h) {",
		"angzarr::router::Rebuilder<validation::test::State> rebuilder;",
		"rebuilder.WithSnapshot(",
		`dispatch.OnCommand("validation.test.CreateOrder"`,
		`rebuilder.Apply("validation.test.OrderCreated"`,
		"*book.add_pages()->mutable_event() = angzarr::router::Pack::Wrap(ev);",
		"inline void RegisterOrderAggregate(angzarr::router::Router& r, OrderAggregateHandler& h) {",
		"r.RegisterAggregate(NewOrderAggregateDispatch(h));",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("cpp wiring missing %q\n---\n%s", want, content)
		}
	}
}

func TestGenerateCpp_SagaUsesMethodRegisterAndTargets(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "cpp", ioPkg,
		declMsg{"OrderSaga", o.componentDecl(2, "orders", "fulfillment", "")},
		declMsg{"OrderPlaced", o.eventDecl(eventEntry{component: fq("OrderSaga"), domain: "orders"})},
	)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	content := resp.File[0].GetContent()
	for _, want := range []string{
		"class OrderSagaHandler {",
		`angzarr::router::SagaDispatch dispatch("OrderSaga", "orders", {"fulfillment"});`,
		`dispatch.OnEvent("validation.test.OrderPlaced"`,
		"r.RegisterSaga(NewOrderSagaDispatch(h));",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("cpp saga wiring missing %q\n---\n%s", want, content)
		}
	}
}

func TestGenerateCpp_RawEventBookEscapeHatch(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "cpp", ioPkg,
		declMsg{"State", o.componentDecl(1, "orders", "", "OrderAggregate")},
		declMsg{"CreateOrder", o.commandDecl(fq("State"))},
	)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	content := resp.File[0].GetContent()
	if !strings.Contains(content, "io::angzarr::v1::EventBook CreateOrder(") {
		t.Errorf("escape-hatch handler should return a raw EventBook; got:\n%s", content)
	}
	if strings.Contains(content, "Pack::Wrap(ev)") {
		t.Errorf("escape-hatch handler must not build an EventBook from typed events")
	}
}

func TestGenerateTypeScript_EmitsStrictSeam(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "typescript", ioPkg, orderAggregate(o)...)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	if len(resp.File) != 1 {
		t.Fatalf("generated %d files, want 1", len(resp.File))
	}
	f := resp.File[0]
	if !strings.HasSuffix(f.GetName(), "_angzarr.ts") {
		t.Errorf("wiring file name = %q, want *_angzarr.ts", f.GetName())
	}
	content := f.GetContent()
	for _, want := range []string{
		`import { create } from "@bufbuild/protobuf";`,
		`from "@angzarr/router";`,
		`from "./validation_test_pb";`,
		"export interface OrderAggregateHandler {",
		// command handler: lowerCamel method, typed-emit array return
		"createOrder(cmd: CreateOrder, state: State, cctx: CommandContext): OrderCreated[];",
		"applyOrderCreated(state: State, ev: OrderCreated): void;",
		"export function newOrderAggregateDispatch(h: OrderAggregateHandler): AggregateDispatch<State> {",
		"const rebuilder = new Rebuilder<State>(() => create(StateSchema));",
		"rebuilder.withSnapshot((state, payload) => Pack.merge(StateSchema, state, payload));",
		`dispatch.onCommand("validation.test.CreateOrder"`,
		`rebuilder.apply("validation.test.OrderCreated"`,
		"return Pack.eventBook(events.map((ev) => Pack.wrap(OrderCreatedSchema, ev)));",
		"export function registerOrderAggregate(r: Router, h: OrderAggregateHandler): void {",
		"r.registerAggregate(newOrderAggregateDispatch(h));",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("typescript wiring missing %q\n---\n%s", want, content)
		}
	}
}

func TestGenerateTypeScript_SagaUsesFunctionRegisterAndTargets(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "typescript", ioPkg,
		declMsg{"OrderSaga", o.componentDecl(2, "orders", "fulfillment", "")},
		declMsg{"OrderPlaced", o.eventDecl(eventEntry{component: fq("OrderSaga"), domain: "orders"})},
	)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	content := resp.File[0].GetContent()
	for _, want := range []string{
		"export interface OrderSagaHandler {",
		`const dispatch = new SagaDispatch("OrderSaga", "orders", ["fulfillment"]);`,
		`dispatch.onEvent("validation.test.OrderPlaced"`,
		"r.registerSaga(newOrderSagaDispatch(h));",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("typescript saga wiring missing %q\n---\n%s", want, content)
		}
	}
}

func TestGenerateTypeScript_RawEventBookEscapeHatch(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "typescript", ioPkg,
		declMsg{"State", o.componentDecl(1, "orders", "", "OrderAggregate")},
		declMsg{"CreateOrder", o.commandDecl(fq("State"))},
	)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	content := resp.File[0].GetContent()
	if !strings.Contains(content, "createOrder(cmd: CreateOrder, state: State, cctx: CommandContext): EventBook;") {
		t.Errorf("escape-hatch handler should return a raw EventBook; got:\n%s", content)
	}
	if strings.Contains(content, "Pack.wrap(") {
		t.Errorf("escape-hatch handler must not build an EventBook from typed events")
	}
}

func TestGenerateTypeScriptScaffold_EmitsOwnedStub(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := scaffold(t, "typescript", ioPkg, nil, orderAggregate(o)...)
	if err != nil {
		t.Fatalf("GenerateScaffold: %v", err)
	}
	if len(resp.File) != 1 {
		t.Fatalf("generated %d files, want 1", len(resp.File))
	}
	f := resp.File[0]
	if !strings.HasSuffix(f.GetName(), "_angzarr_handler.ts") {
		t.Errorf("scaffold file name = %q, want *_angzarr_handler.ts", f.GetName())
	}
	content := f.GetContent()
	for _, want := range []string{
		"export class OrderAggregate implements OrderAggregateHandler {",
		`throw new Error("TODO: implement OrderAggregate.createOrder");`,
	} {
		if !strings.Contains(content, want) {
			t.Errorf("typescript scaffold missing %q\n---\n%s", want, content)
		}
	}
}

func TestGenerate_FilePerComponent_OneFileEach(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	// Two components declared in one proto file → two generated wiring files.
	resp, err := generate(t, "go", ioPkg,
		declMsg{"State", o.componentDecl(1, "orders", "", "OrderAggregate")},
		declMsg{"CreateOrder", o.commandDecl(fq("State"))},
		declMsg{"OrderSaga", o.componentDecl(2, "orders", "fulfillment", "")},
		declMsg{"OrderPlaced", o.eventDecl(eventEntry{component: fq("OrderSaga"), domain: "orders"})},
	)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	if len(resp.File) != 2 {
		t.Fatalf("file-per-component: generated %d files, want 2", len(resp.File))
	}
	for _, want := range []string{"/order_aggregate_angzarr.pb.go", "/order_saga_angzarr.pb.go"} {
		found := false
		for _, f := range resp.File {
			if strings.HasSuffix(f.GetName(), want) {
				found = true
			}
		}
		if !found {
			var got []string
			for _, f := range resp.File {
				got = append(got, f.GetName())
			}
			t.Errorf("missing per-component file %q; got %v", want, got)
		}
	}
}

func TestGenerate_UnknownLanguage_Fails(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	_, err := generate(t, "cobol", ioPkg, orderAggregate(o)...)
	if err == nil || !strings.Contains(err.Error(), "no emitter") {
		t.Fatalf("err = %v, want no-emitter failure", err)
	}
}

func TestLanguages_ListsGoAndPython(t *testing.T) {
	langs := codegen.Languages()
	have := map[string]bool{}
	for _, l := range langs {
		have[l] = true
	}
	for _, want := range []string{"cpp", "csharp", "go", "java", "python", "typescript"} {
		if !have[want] {
			t.Fatalf("Languages() = %v, want to include %q", langs, want)
		}
	}
	if !sort.StringsAreSorted(langs) {
		t.Errorf("Languages() not sorted: %v", langs)
	}
}

// ensure proto import is used (the option-message round-trips rely on it via
// the generator; this keeps the import live for any future direct assertion).
var _ = proto.Marshal
