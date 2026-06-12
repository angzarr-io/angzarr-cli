package codegen_test

// These tests build descriptor sets entirely in memory — including the
// angzarr options file itself — proving the generator reads declarations
// dynamically with no compiled options bindings, and that every
// generation-time validation fires language-independently.

import (
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

const (
	optionsPath = "angzarr_client/proto/angzarr/v1/options.proto"
	optionsPkg  = "angzarr_client.proto.angzarr.v1"
	testPath    = "validation_test.proto"
)

func str(s string) *string { return &s }
func i32(i int32) *int32   { return &i }

func enumValue(name string, number int32) *descriptorpb.EnumValueDescriptorProto {
	return &descriptorpb.EnumValueDescriptorProto{Name: str(name), Number: i32(number)}
}

func stringFieldProto(name string, number int32) *descriptorpb.FieldDescriptorProto {
	return &descriptorpb.FieldDescriptorProto{
		Name:     str(name),
		Number:   i32(number),
		Label:    descriptorpb.FieldDescriptorProto_LABEL_OPTIONAL.Enum(),
		Type:     descriptorpb.FieldDescriptorProto_TYPE_STRING.Enum(),
		JsonName: str(name),
	}
}

// optionsFDP reconstructs options.proto as a descriptor: the ComponentKind
// enum, the three option messages, and the four extensions.
func optionsFDP() *descriptorpb.FileDescriptorProto {
	return &descriptorpb.FileDescriptorProto{
		Name:       str(optionsPath),
		Package:    str(optionsPkg),
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
					{
						Name:     str("kind"),
						Number:   i32(1),
						Label:    descriptorpb.FieldDescriptorProto_LABEL_OPTIONAL.Enum(),
						Type:     descriptorpb.FieldDescriptorProto_TYPE_ENUM.Enum(),
						TypeName: str("." + optionsPkg + ".ComponentKind"),
						JsonName: str("kind"),
					},
					stringFieldProto("input_domain", 2),
					stringFieldProto("output_domain", 3),
					stringFieldProto("state", 4),
				},
			},
			{Name: str("RejectedOptions"), Field: []*descriptorpb.FieldDescriptorProto{stringFieldProto("command", 1)}},
			{Name: str("ReactsOptions"), Field: []*descriptorpb.FieldDescriptorProto{stringFieldProto("domain", 1)}},
		},
		Extension: []*descriptorpb.FieldDescriptorProto{
			{
				Name:     str("component"),
				Number:   i32(50100),
				Label:    descriptorpb.FieldDescriptorProto_LABEL_OPTIONAL.Enum(),
				Type:     descriptorpb.FieldDescriptorProto_TYPE_MESSAGE.Enum(),
				TypeName: str("." + optionsPkg + ".ComponentOptions"),
				Extendee: str(".google.protobuf.ServiceOptions"),
				JsonName: str("component"),
			},
			{
				Name:     str("rejected"),
				Number:   i32(50101),
				Label:    descriptorpb.FieldDescriptorProto_LABEL_OPTIONAL.Enum(),
				Type:     descriptorpb.FieldDescriptorProto_TYPE_MESSAGE.Enum(),
				TypeName: str("." + optionsPkg + ".RejectedOptions"),
				Extendee: str(".google.protobuf.MethodOptions"),
				JsonName: str("rejected"),
			},
			{
				Name:     str("applies"),
				Number:   i32(50102),
				Label:    descriptorpb.FieldDescriptorProto_LABEL_OPTIONAL.Enum(),
				Type:     descriptorpb.FieldDescriptorProto_TYPE_BOOL.Enum(),
				Extendee: str(".google.protobuf.MethodOptions"),
				JsonName: str("applies"),
			},
			{
				Name:     str("reacts"),
				Number:   i32(50103),
				Label:    descriptorpb.FieldDescriptorProto_LABEL_OPTIONAL.Enum(),
				Type:     descriptorpb.FieldDescriptorProto_TYPE_MESSAGE.Enum(),
				TypeName: str("." + optionsPkg + ".ReactsOptions"),
				Extendee: str(".google.protobuf.MethodOptions"),
				JsonName: str("reacts"),
			},
		},
	}
}

// optionTypes materializes the extensions and option messages so tests can
// stamp declarations onto services — through the same dynamic machinery
// the generator itself uses.
type optionTypes struct {
	component, rejected, applies, reacts protoreflect.ExtensionType
	componentMsg, rejectedMsg, reactsMsg protoreflect.MessageType
}

func buildOptionTypes(t *testing.T) optionTypes {
	t.Helper()
	files := &protoregistry.Files{}
	if err := files.RegisterFile(descriptorpb.File_google_protobuf_descriptor_proto); err != nil {
		t.Fatalf("register descriptor.proto: %v", err)
	}
	fd, err := protodesc.NewFile(optionsFDP(), files)
	if err != nil {
		t.Fatalf("build options.proto descriptor: %v", err)
	}
	exts := fd.Extensions()
	msgs := fd.Messages()
	find := func(name string) protoreflect.ExtensionType {
		for i := 0; i < exts.Len(); i++ {
			if string(exts.Get(i).Name()) == name {
				return dynamicpb.NewExtensionType(exts.Get(i))
			}
		}
		t.Fatalf("extension %q not found", name)
		return nil
	}
	msg := func(name string) protoreflect.MessageType {
		d := msgs.ByName(protoreflect.Name(name))
		if d == nil {
			t.Fatalf("message %q not found", name)
		}
		return dynamicpb.NewMessageType(d)
	}
	return optionTypes{
		component:    find("component"),
		rejected:     find("rejected"),
		applies:      find("applies"),
		reacts:       find("reacts"),
		componentMsg: msg("ComponentOptions"),
		rejectedMsg:  msg("RejectedOptions"),
		reactsMsg:    msg("ReactsOptions"),
	}
}

func (o optionTypes) componentDecl(t *testing.T, kind int32, inputDomain, outputDomain, state string) *descriptorpb.ServiceOptions {
	t.Helper()
	decl := o.componentMsg.New()
	fields := decl.Descriptor().Fields()
	decl.Set(fields.ByName("kind"), protoreflect.ValueOfEnum(protoreflect.EnumNumber(kind)))
	if inputDomain != "" {
		decl.Set(fields.ByName("input_domain"), protoreflect.ValueOfString(inputDomain))
	}
	if outputDomain != "" {
		decl.Set(fields.ByName("output_domain"), protoreflect.ValueOfString(outputDomain))
	}
	if state != "" {
		decl.Set(fields.ByName("state"), protoreflect.ValueOfString(state))
	}
	opts := &descriptorpb.ServiceOptions{}
	proto.SetExtension(opts, o.component, decl.Interface())
	return opts
}

func (o optionTypes) reactsRPC(t *testing.T, name, domain string) *descriptorpb.MethodDescriptorProto {
	t.Helper()
	m := handlerRPC(name)
	decl := o.reactsMsg.New()
	decl.Set(decl.Descriptor().Fields().ByName("domain"), protoreflect.ValueOfString(domain))
	m.Options = &descriptorpb.MethodOptions{}
	proto.SetExtension(m.Options, o.reacts, decl.Interface())
	return m
}

func (o optionTypes) applierRPC(name string) *descriptorpb.MethodDescriptorProto {
	m := handlerRPC(name)
	m.Options = &descriptorpb.MethodOptions{}
	proto.SetExtension(m.Options, o.applies, true)
	return m
}

func handlerRPC(name string) *descriptorpb.MethodDescriptorProto {
	return &descriptorpb.MethodDescriptorProto{
		Name:       str(name),
		InputType:  str(".validation.test.Cmd"),
		OutputType: str(".validation.test.Evt"),
	}
}

// generate runs the real generator over a one-service descriptor set.
func generate(t *testing.T, lang, serviceName string, svcOpts *descriptorpb.ServiceOptions, methods ...*descriptorpb.MethodDescriptorProto) (*pluginpb.CodeGeneratorResponse, error) {
	t.Helper()
	testFile := &descriptorpb.FileDescriptorProto{
		Name:       str(testPath),
		Package:    str("validation.test"),
		Syntax:     str("proto3"),
		Dependency: []string{optionsPath},
		Options:    &descriptorpb.FileOptions{GoPackage: str("example.test/validation;validationtest")},
		MessageType: []*descriptorpb.DescriptorProto{
			{Name: str("State")},
			{Name: str("Cmd")},
			{Name: str("Evt")},
		},
		Service: []*descriptorpb.ServiceDescriptorProto{{
			Name:    str(serviceName),
			Options: svcOpts,
			Method:  methods,
		}},
	}
	gen, err := protogen.Options{}.New(&pluginpb.CodeGeneratorRequest{
		FileToGenerate: []string{testPath},
		ProtoFile: []*descriptorpb.FileDescriptorProto{
			protodesc.ToFileDescriptorProto(descriptorpb.File_google_protobuf_descriptor_proto),
			optionsFDP(),
			testFile,
		},
	})
	if err != nil {
		return nil, err
	}
	if err := codegen.Generate(gen, lang); err != nil {
		return nil, err
	}
	return gen.Response(), nil
}

func TestGenerate_ValidAggregate_EmitsStrictSeam(t *testing.T) {
	o := buildOptionTypes(t)
	resp, err := generate(t, "go", "OrderAggregate",
		o.componentDecl(t, 1, "orders", "", "validation.test.State"),
		handlerRPC("HandleCreate"), o.applierRPC("ApplyCreated"))
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	if len(resp.File) != 1 {
		t.Fatalf("generated %d files, want 1", len(resp.File))
	}
	content := resp.File[0].GetContent()
	for _, want := range []string{
		"type OrderAggregateHandler interface",
		"HandleCreate(cmd *",
		"ApplyCreated(state *",
		"func NewOrderAggregateDispatch(",
		`OnCommand("validation.test.Cmd"`,
	} {
		if !strings.Contains(content, want) {
			t.Errorf("generated file missing %q", want)
		}
	}
}

func TestGenerate_Validations_FailGeneration(t *testing.T) {
	o := buildOptionTypes(t)
	tests := []struct {
		name    string
		opts    *descriptorpb.ServiceOptions
		methods []*descriptorpb.MethodDescriptorProto
	}{
		{"aggregate without state", o.componentDecl(t, 1, "orders", "", ""),
			[]*descriptorpb.MethodDescriptorProto{handlerRPC("HandleCreate")}},
		{"saga without target", o.componentDecl(t, 2, "orders", "", ""),
			[]*descriptorpb.MethodDescriptorProto{handlerRPC("HandleEvt")}},
		{"process manager without output domain", o.componentDecl(t, 3, "", "", "validation.test.State"),
			[]*descriptorpb.MethodDescriptorProto{o.reactsRPC(t, "HandleEvt", "orders")}},
		{"process manager handler without reacts", o.componentDecl(t, 3, "", "fulfillment", "validation.test.State"),
			[]*descriptorpb.MethodDescriptorProto{handlerRPC("HandleEvt")}},
		{"projector without domains", o.componentDecl(t, 4, "", "", "validation.test.State"),
			[]*descriptorpb.MethodDescriptorProto{handlerRPC("HandleEvt")}},
		{"unresolvable state", o.componentDecl(t, 1, "orders", "", "validation.test.Nope"),
			[]*descriptorpb.MethodDescriptorProto{handlerRPC("HandleCreate")}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if _, err := generate(t, "go", "X", tt.opts, tt.methods...); err == nil {
				t.Fatal("expected generation to fail")
			}
		})
	}
}

func TestGenerate_UnknownLanguage_Fails(t *testing.T) {
	o := buildOptionTypes(t)
	_, err := generate(t, "cobol", "OrderAggregate",
		o.componentDecl(t, 1, "orders", "", "validation.test.State"), handlerRPC("HandleCreate"))
	if err == nil || !strings.Contains(err.Error(), "no emitter") {
		t.Fatalf("err = %v, want no-emitter failure", err)
	}
}

func TestLanguages_ListsGo(t *testing.T) {
	langs := codegen.Languages()
	if len(langs) == 0 || langs[0] != "go" {
		t.Fatalf("Languages() = %v, want [go ...]", langs)
	}
}
