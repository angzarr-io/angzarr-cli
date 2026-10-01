package cmd

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"google.golang.org/protobuf/encoding/protowire"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/reflect/protodesc"
	"google.golang.org/protobuf/types/descriptorpb"
	"google.golang.org/protobuf/types/pluginpb"
)

// optionsFile is a minimal io/angzarr/v1/options.proto: the ComponentKind enum,
// ComponentOptions and the (component) extension at 50100.
func optionsFile() *descriptorpb.FileDescriptorProto {
	opt := descriptorpb.FieldDescriptorProto_LABEL_OPTIONAL.Enum()
	str := descriptorpb.FieldDescriptorProto_TYPE_STRING.Enum()
	return &descriptorpb.FileDescriptorProto{
		Name:       strp("io/angzarr/v1/options.proto"),
		Package:    strp("io.angzarr.v1"),
		Syntax:     strp("proto3"),
		Dependency: []string{"google/protobuf/descriptor.proto"},
		Options:    &descriptorpb.FileOptions{GoPackage: strp("example.test/angzarrpb;angzarrpb")},
		EnumType: []*descriptorpb.EnumDescriptorProto{{
			Name: strp("ComponentKind"),
			Value: []*descriptorpb.EnumValueDescriptorProto{
				{Name: strp("COMPONENT_KIND_UNSPECIFIED"), Number: proto.Int32(0)},
				{Name: strp("COMPONENT_KIND_AGGREGATE"), Number: proto.Int32(1)},
			},
		}},
		MessageType: []*descriptorpb.DescriptorProto{{
			Name: strp("ComponentOptions"),
			Field: []*descriptorpb.FieldDescriptorProto{
				{Name: strp("kind"), Number: proto.Int32(1), Label: opt, Type: descriptorpb.FieldDescriptorProto_TYPE_ENUM.Enum(), TypeName: strp(".io.angzarr.v1.ComponentKind"), JsonName: strp("kind")},
				{Name: strp("input_domain"), Number: proto.Int32(2), Label: opt, Type: str, JsonName: strp("inputDomain")},
				{Name: strp("name"), Number: proto.Int32(4), Label: opt, Type: str, JsonName: strp("name")},
				{Name: strp("domain"), Number: proto.Int32(7), Label: opt, Type: str, JsonName: strp("domain")},
			},
		}},
		Extension: []*descriptorpb.FieldDescriptorProto{{
			Name: strp("component"), Number: proto.Int32(50100), Label: opt,
			Type: descriptorpb.FieldDescriptorProto_TYPE_MESSAGE.Enum(), TypeName: strp(".io.angzarr.v1.ComponentOptions"),
			Extendee: strp(".google.protobuf.MessageOptions"), JsonName: strp("component"),
		}},
	}
}

// aggregateOptions encodes (io.angzarr.v1.component) = {kind: AGGREGATE,
// domain: "orders", name: "OrderAggregate"} as raw MessageOptions bytes,
// the way protoc delivers an extension the plugin binary has no bindings for.
func aggregateOptions(t *testing.T) *descriptorpb.MessageOptions {
	t.Helper()
	var sub []byte
	sub = protowire.AppendTag(sub, 1, protowire.VarintType)
	sub = protowire.AppendVarint(sub, 1)
	sub = protowire.AppendTag(sub, 7, protowire.BytesType)
	sub = protowire.AppendString(sub, "orders")
	sub = protowire.AppendTag(sub, 4, protowire.BytesType)
	sub = protowire.AppendString(sub, "OrderAggregate")
	var raw []byte
	raw = protowire.AppendTag(raw, 50100, protowire.BytesType)
	raw = protowire.AppendBytes(raw, sub)
	opts := &descriptorpb.MessageOptions{}
	if err := proto.Unmarshal(raw, opts); err != nil {
		t.Fatalf("decode options: %v", err)
	}
	return opts
}

// aggregateRequest is a CodeGeneratorRequest for orders.proto declaring one
// aggregate (OrderAggregate, anchored on OrderState), with the given plugin
// parameter string.
func aggregateRequest(t *testing.T, parameter string) []byte {
	t.Helper()
	orders := &descriptorpb.FileDescriptorProto{
		Name:        strp("orders.proto"),
		Package:     strp("shop.orders"),
		Syntax:      strp("proto3"),
		Dependency:  []string{"io/angzarr/v1/options.proto"},
		Options:     &descriptorpb.FileOptions{GoPackage: strp("example.test/orders;orders")},
		MessageType: []*descriptorpb.DescriptorProto{{Name: strp("OrderState"), Options: aggregateOptions(t)}},
	}
	req := &pluginpb.CodeGeneratorRequest{
		FileToGenerate: []string{"orders.proto"},
		Parameter:      strp(parameter),
		ProtoFile: []*descriptorpb.FileDescriptorProto{
			protodesc.ToFileDescriptorProto(descriptorpb.File_google_protobuf_descriptor_proto),
			optionsFile(),
			orders,
		},
	}
	raw, err := proto.Marshal(req)
	if err != nil {
		t.Fatalf("marshal request: %v", err)
	}
	return raw
}

const orderStub = "order_aggregate_angzarr_handler.go"

func scaffoldResponse(t *testing.T, parameter string) *pluginpb.CodeGeneratorResponse {
	t.Helper()
	var out bytes.Buffer
	if err := runScaffold(bytes.NewReader(aggregateRequest(t, parameter)), &out, "go"); err != nil {
		t.Fatalf("runScaffold: %v", err)
	}
	resp := &pluginpb.CodeGeneratorResponse{}
	if err := proto.Unmarshal(out.Bytes(), resp); err != nil {
		t.Fatalf("parse response: %v", err)
	}
	return resp
}

func TestRunScaffold_ExistingStubUnderOutDir_IsPreserved(t *testing.T) {
	outDir := t.TempDir()
	if err := os.WriteFile(filepath.Join(outDir, orderStub), []byte("// MINE\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	resp := scaffoldResponse(t, "paths=source_relative,out_dir="+outDir)
	if resp.GetError() != "" {
		t.Fatalf("response error: %s", resp.GetError())
	}
	if len(resp.File) != 0 {
		t.Fatalf("scaffold re-emitted %d file(s) over an existing stub in out_dir: %v", len(resp.File), resp.File[0].GetName())
	}
}

func TestRunScaffold_StubAbsentUnderOutDir_IsEmitted(t *testing.T) {
	resp := scaffoldResponse(t, "paths=source_relative,out_dir="+t.TempDir())
	if resp.GetError() != "" {
		t.Fatalf("response error: %s", resp.GetError())
	}
	if len(resp.File) != 1 || resp.File[0].GetName() != orderStub {
		t.Fatalf("want one stub %s, got %v", orderStub, resp.File)
	}
	if !strings.Contains(resp.File[0].GetContent(), "type OrderAggregate struct{}") {
		t.Errorf("stub content missing the handler type:\n%s", resp.File[0].GetContent())
	}
}

func TestRunScaffold_WithoutOutDir_Refuses(t *testing.T) {
	resp := scaffoldResponse(t, "paths=source_relative")
	if !strings.Contains(resp.GetError(), "out_dir") {
		t.Fatalf("want an error naming out_dir, got %q", resp.GetError())
	}
	if len(resp.File) != 0 {
		t.Fatalf("scaffold without out_dir emitted %d file(s)", len(resp.File))
	}
}

func TestRunPlugin_RejectsUnknownParameter(t *testing.T) {
	var out bytes.Buffer
	err := runPlugin(bytes.NewReader(aggregateRequest(t, "bogus=1")), &out, "go")
	if err == nil || !strings.Contains(err.Error(), "bogus") {
		t.Fatalf("want unknown-parameter error naming bogus, got %v", err)
	}
}

func TestRunPlugin_EmitsWiring(t *testing.T) {
	var out bytes.Buffer
	if err := runPlugin(bytes.NewReader(aggregateRequest(t, "paths=source_relative")), &out, "go"); err != nil {
		t.Fatalf("runPlugin: %v", err)
	}
	resp := &pluginpb.CodeGeneratorResponse{}
	if err := proto.Unmarshal(out.Bytes(), resp); err != nil {
		t.Fatal(err)
	}
	if len(resp.File) != 1 || resp.File[0].GetName() != "order_aggregate_angzarr.pb.go" {
		t.Fatalf("want order_aggregate_angzarr.pb.go, got %v (error %q)", resp.File, resp.GetError())
	}
}

func TestRunLint_RequestCarryingPluginParameters_Lints(t *testing.T) {
	var out, errOut bytes.Buffer
	raw := aggregateRequest(t, "paths=source_relative,templates=github.com/org/repo@v1,param.a=b")
	if err := runLint(bytes.NewReader(raw), &out, &errOut, true); err != nil {
		t.Fatalf("runLint --request: %v (stderr %s)", err, errOut.String())
	}
	// The aggregate declares nothing to handle: the lint run reached analysis.
	if !strings.Contains(errOut.String(), "ANZ103") {
		t.Errorf("want ANZ103 from analysing the request, got stderr %q", errOut.String())
	}
}

func TestRunPlugin_RejectsOutDir(t *testing.T) {
	var out bytes.Buffer
	err := runPlugin(bytes.NewReader(aggregateRequest(t, "out_dir=.")), &out, "go")
	if err == nil || !strings.Contains(err.Error(), "out_dir") {
		t.Fatalf("codegen must reject out_dir (scaffold-only), got %v", err)
	}
}

func TestRunPluginAndScaffold_GenerationFailureTravelsInResponse(t *testing.T) {
	for name, run := range map[string]func(*bytes.Buffer) error{
		"codegen": func(out *bytes.Buffer) error {
			return runPlugin(bytes.NewReader(aggregateRequest(t, "")), out, "cobol")
		},
		"scaffold": func(out *bytes.Buffer) error {
			return runScaffold(bytes.NewReader(aggregateRequest(t, "out_dir="+t.TempDir())), out, "cobol")
		},
	} {
		t.Run(name, func(t *testing.T) {
			var out bytes.Buffer
			if err := run(&out); err != nil {
				t.Fatalf("protocol-level error: %v", err)
			}
			resp := &pluginpb.CodeGeneratorResponse{}
			if err := proto.Unmarshal(out.Bytes(), resp); err != nil {
				t.Fatal(err)
			}
			if !strings.Contains(resp.GetError(), "cobol") {
				t.Fatalf("want response error naming the unknown language, got %q", resp.GetError())
			}
		})
	}
}
