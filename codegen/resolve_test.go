package codegen_test

import (
	"strings"
	"testing"

	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/reflect/protodesc"
	"google.golang.org/protobuf/types/descriptorpb"
	"google.golang.org/protobuf/types/pluginpb"

	"github.com/angzarr-io/angzarr-cli/codegen"
)

// requestGen builds a plugin over descriptor.proto plus files, generating
// the named paths.
func requestGen(t *testing.T, generate []string, files ...*descriptorpb.FileDescriptorProto) *protogen.Plugin {
	t.Helper()
	protoFiles := append([]*descriptorpb.FileDescriptorProto{
		protodesc.ToFileDescriptorProto(descriptorpb.File_google_protobuf_descriptor_proto),
	}, files...)
	gen, err := protogen.Options{}.New(&pluginpb.CodeGeneratorRequest{FileToGenerate: generate, ProtoFile: protoFiles})
	if err != nil {
		t.Fatalf("protogen.New: %v", err)
	}
	return gen
}

// declFile is the test proto file carrying msgs, importing deps.
func declFile(deps []string, msgs ...declMsg) *descriptorpb.FileDescriptorProto {
	f := &descriptorpb.FileDescriptorProto{
		Name:       str(testPath),
		Package:    str(testPkg),
		Syntax:     str("proto3"),
		Dependency: deps,
		Options:    &descriptorpb.FileOptions{GoPackage: str("example.test/validation;validationtest")},
	}
	for _, m := range msgs {
		f.MessageType = append(f.MessageType, &descriptorpb.DescriptorProto{Name: str(m.name), Options: m.opts})
	}
	return f
}

func TestGenerate_OptionsFileWithExtraImports_Resolves(t *testing.T) {
	// options.proto importing anything beyond descriptor.proto must still
	// resolve: its declarations generate as usual.
	o := buildOptionTypes(t, ioPkg)
	extra := &descriptorpb.FileDescriptorProto{
		Name:        str("io/angzarr/v1/extra.proto"),
		Package:     str(ioPkg),
		Syntax:      str("proto3"),
		Options:     &descriptorpb.FileOptions{GoPackage: str("example.test/angzarrpb;angzarrpb")},
		MessageType: []*descriptorpb.DescriptorProto{{Name: str("Extra")}},
	}
	opts := optionsFDP(ioPkg)
	opts.Dependency = append(opts.Dependency, "io/angzarr/v1/extra.proto")
	gen := requestGen(t, []string{testPath}, extra, opts, declFile([]string{optionsPath}, orderAggregate(o)...))
	if diags := codegen.Lint(gen); codegen.HasErrors(diags) {
		t.Fatalf("lint errors: %v", diags)
	}
	if err := codegen.Generate(gen, "go", codegen.Options{}); err != nil {
		t.Fatalf("Generate: %v", err)
	}
	if n := len(gen.Response().File); n != 1 {
		t.Fatalf("generated %d files, want 1 (the aggregate's wiring)", n)
	}
}

func TestLint_DeclarationWithoutResolvableOptions_Errors(t *testing.T) {
	// The message carries (component) bytes at 50100, but nothing in the
	// request defines the extension: lint and codegen must fail, not pass
	// with nothing found.
	o := buildOptionTypes(t, ioPkg)
	gen := requestGen(t, []string{testPath}, declFile(nil, orderAggregate(o)...))
	diags := codegen.Lint(gen)
	if !hasCode(diags, "ANZ009") {
		t.Fatalf("want ANZ009 for unresolvable angzarr options, got %v", diags)
	}
	if sev, _ := severityOf(diags, "ANZ009"); sev != codegen.SeverityError {
		t.Errorf("ANZ009 should be an error, got %v", sev)
	}
	want := map[string]string{"State": "50100", "CreateOrder": "50104", "OrderCreated": "50105"}
	for _, d := range diags {
		for msg, num := range want {
			if d.Code == "ANZ009" && strings.Contains(d.Message, fq(msg)+`"`) && strings.Contains(d.Message, num) {
				delete(want, msg)
			}
		}
	}
	if len(want) != 0 {
		t.Errorf("missing ANZ009 (message -> option number) for %v in %v", want, diags)
	}
	gen2 := requestGen(t, []string{testPath}, declFile(nil, orderAggregate(o)...))
	if err := codegen.Generate(gen2, "go", codegen.Options{}); err == nil || !strings.Contains(err.Error(), "ANZ009") {
		t.Fatalf("Generate must fail with ANZ009, got %v", err)
	}
}

func TestLint_RequestWithoutAngzarrDeclarations_IsClean(t *testing.T) {
	gen := requestGen(t, []string{testPath}, declFile(nil, declMsg{"Plain", nil}))
	if diags := codegen.Lint(gen); len(diags) != 0 {
		t.Fatalf("plain protos must lint clean, got %v", diags)
	}
}

// splitAggregate puts the aggregate's anchor in anchor.proto and its command
// and event in ops.proto (which imports anchor.proto), as two directories
// would under buf's per-directory plugin strategy.
func splitAggregate(t *testing.T, o optionTypes, generate ...string) *protogen.Plugin {
	t.Helper()
	anchor := &descriptorpb.FileDescriptorProto{
		Name:        str("orders/state/anchor.proto"),
		Package:     str(testPkg),
		Syntax:      str("proto3"),
		Dependency:  []string{optionsPath},
		Options:     &descriptorpb.FileOptions{GoPackage: str("example.test/state;state")},
		MessageType: []*descriptorpb.DescriptorProto{{Name: str("State"), Options: o.ownedDecl(1, "orders", "", "OrderAggregate")}},
	}
	ops := &descriptorpb.FileDescriptorProto{
		Name:       str("orders/ops/ops.proto"),
		Package:    str(testPkg),
		Syntax:     str("proto3"),
		Dependency: []string{optionsPath, "orders/state/anchor.proto"},
		Options:    &descriptorpb.FileOptions{GoPackage: str("example.test/ops;ops")},
		MessageType: []*descriptorpb.DescriptorProto{
			{Name: str("CreateOrder"), Options: o.commandDecl(fq("State"), fq("OrderCreated"))},
			{Name: str("OrderCreated"), Options: o.eventDecl(eventEntry{component: fq("State")})},
		},
	}
	return requestGen(t, generate, optionsFDP(ioPkg), anchor, ops)
}

func TestLint_DeclarationsSplitFromAnchorAcrossRuns_Errors(t *testing.T) {
	// ops.proto is generated, its anchor is not: this run cannot wire the
	// handlers and the anchor's own run cannot see them.
	o := buildOptionTypes(t, ioPkg)
	diags := codegen.Lint(splitAggregate(t, o, "orders/ops/ops.proto"))
	n := 0
	for _, d := range diags {
		if d.Code == "ANZ013" {
			n++
			if d.Severity != codegen.SeverityError || !strings.Contains(d.Message, "strategy: all") {
				t.Errorf("ANZ013 should be an error pointing at strategy: all, got %v", d)
			}
		}
	}
	if n != 2 {
		t.Fatalf("want ANZ013 for the command and the event, got %v", diags)
	}
	if err := codegen.Generate(splitAggregate(t, o, "orders/ops/ops.proto"), "go", codegen.Options{}); err == nil {
		t.Fatal("Generate must refuse a request that splits a component's declarations")
	}
}

func TestLint_AllDeclarationFilesGeneratedTogether_IsClean(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	gen := splitAggregate(t, o, "orders/state/anchor.proto", "orders/ops/ops.proto")
	if diags := codegen.Lint(gen); hasCode(diags, "ANZ013") || codegen.HasErrors(diags) {
		t.Fatalf("one run over every file must lint clean, got %v", diags)
	}
}

func TestLint_AnchorGeneratedWithImportedDeclarations_IsClean(t *testing.T) {
	// The anchor's run sees the declarations through an import: it wires them.
	o := buildOptionTypes(t, ioPkg)
	gen := splitAggregate(t, o, "orders/state/anchor.proto")
	if diags := codegen.Lint(gen); hasCode(diags, "ANZ013") {
		t.Fatalf("anchor-side run must not report ANZ013, got %v", diags)
	}
}

func TestLint_UnknownComponentHintsAtStrategy(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	diags := lint(t, declMsg{"CreateOrder", o.commandDecl(fq("Nope"))})
	for _, d := range diags {
		if d.Code == "ANZ002" && strings.Contains(d.Message, "strategy: all") {
			return
		}
	}
	t.Fatalf("ANZ002 should mention strategy: all for anchors outside the request, got %v", diags)
}

func TestLint_UnresolvableOptionAfterOtherOptions_Errors(t *testing.T) {
	// The angzarr bytes follow a standard option on the wire; the scan must
	// step past the earlier field to find them.
	o := buildOptionTypes(t, ioPkg)
	raw, err := proto.Marshal(o.ownedDecl(1, "orders", "", "OrderAggregate"))
	if err != nil {
		t.Fatal(err)
	}
	opts := &descriptorpb.MessageOptions{Deprecated: proto.Bool(true)}
	opts.ProtoReflect().SetUnknown(raw) // unknown fields marshal after known ones
	gen := requestGen(t, []string{testPath}, declFile(nil, declMsg{"State", opts}))
	if diags := codegen.Lint(gen); !hasCode(diags, "ANZ009") {
		t.Fatalf("want ANZ009 behind a deprecated option, got %v", diags)
	}
}
