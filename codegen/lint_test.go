package codegen_test

// Lint reuses the in-memory descriptor harness from generate_test.go
// (buildGen / buildOptionTypes / optionTypes / declMsg / orderAggregate). These
// tests assert the collect-all behaviour, the stable ANZxxxx codes, and the
// error-vs-warning split that gates code generation.

import (
	"testing"

	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/reflect/protodesc"
	"google.golang.org/protobuf/types/descriptorpb"
	"google.golang.org/protobuf/types/pluginpb"

	"github.com/angzarr-io/angzarr-cli/codegen"
)

// lint builds a descriptor set from the declared messages and runs the linter.
func lint(t *testing.T, msgs ...declMsg) []codegen.Diagnostic {
	t.Helper()
	gen, err := buildGen(t, ioPkg, msgs...)
	if err != nil {
		t.Fatalf("buildGen: %v", err)
	}
	return codegen.Lint(gen)
}

func codesOf(diags []codegen.Diagnostic) []string {
	out := make([]string, len(diags))
	for i, d := range diags {
		out[i] = d.Code
	}
	return out
}

func hasCode(diags []codegen.Diagnostic, code string) bool {
	for _, d := range diags {
		if d.Code == code {
			return true
		}
	}
	return false
}

func severityOf(diags []codegen.Diagnostic, code string) (codegen.Severity, bool) {
	for _, d := range diags {
		if d.Code == code {
			return d.Severity, true
		}
	}
	return 0, false
}

func TestLint_ValidAggregate_NoDiagnostics(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	diags := lint(t, orderAggregate(o)...)
	if len(diags) != 0 {
		t.Fatalf("valid aggregate produced diagnostics: %v", diags)
	}
}

func TestLint_CollectsAllErrors_NotJustTheFirst(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	// Two independent, unrelated errors: a command and an event both pointing at
	// undeclared components. A fail-fast validator would report only one.
	diags := lint(t,
		declMsg{"CreateOrder", o.commandDecl(fq("Nope"))},
		declMsg{"OrderCreated", o.eventDecl(eventEntry{component: fq("AlsoNope")})},
	)
	if !hasCode(diags, "ANZ002") || !hasCode(diags, "ANZ005") {
		t.Fatalf("expected both ANZ002 and ANZ005, got %v", codesOf(diags))
	}
}

func TestLint_TierA_ResolutionErrors(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	tests := []struct {
		name string
		want string
		msgs []declMsg
	}{
		{"command to unknown component", "ANZ002", []declMsg{
			{"CreateOrder", o.commandDecl(fq("Nope"))},
		}},
		{"command handled by non-aggregate", "ANZ003", []declMsg{
			{"OrderSaga", o.componentDecl(2, "orders", "fulfillment", "")},
			{"CreateOrder", o.commandDecl(fq("OrderSaga"))},
		}},
		{"command emits unresolvable", "ANZ004", []declMsg{
			{"State", o.componentDecl(1, "orders", "", "")},
			{"CreateOrder", o.commandDecl(fq("State"), fq("Nope"))},
		}},
		{"event to unknown component", "ANZ005", []declMsg{
			{"OrderCreated", o.eventDecl(eventEntry{component: fq("Nope")})},
		}},
		{"process manager trigger without domain", "ANZ006", []declMsg{
			{"PMState", o.componentDecl(3, "", "fulfillment", "")},
			{"Trig", o.eventDecl(eventEntry{component: fq("PMState")})},
		}},
		{"compensates unresolvable", "ANZ007", []declMsg{
			{"State", o.componentDecl(1, "orders", "", "", fq("Nope"))},
		}},
		{"aggregate without input domain", "ANZ008", []declMsg{
			{"State", o.componentDecl(1, "", "", "")},
		}},
		{"saga without output domain", "ANZ008", []declMsg{
			{"OrderSaga", o.componentDecl(2, "orders", "", "")},
		}},
		{"process manager without output domain", "ANZ008", []declMsg{
			{"PMState", o.componentDecl(3, "", "", "")},
		}},
		{"projector without input domain", "ANZ008", []declMsg{
			{"ProjState", o.componentDecl(4, "", "", "")},
		}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			diags := lint(t, tt.msgs...)
			if !hasCode(diags, tt.want) {
				t.Fatalf("want %s, got %v", tt.want, codesOf(diags))
			}
			if sev, _ := severityOf(diags, tt.want); sev != codegen.SeverityError {
				t.Errorf("%s should be an error, got %v", tt.want, sev)
			}
		})
	}
}

func TestLint_DuplicateGeneratedName(t *testing.T) {
	// Two components that resolve to the same generated base name would emit
	// colliding Go/Python identifiers — uncompilable. ANZ010.
	o := buildOptionTypes(t, ioPkg)
	diags := lint(t,
		declMsg{"StateA", o.componentDecl(1, "orders", "", "Dup")},
		declMsg{"StateB", o.componentDecl(1, "orders", "", "Dup")},
	)
	if !hasCode(diags, "ANZ010") {
		t.Fatalf("want ANZ010 generated-name collision, got %v", codesOf(diags))
	}
}

func TestLint_TierB_MethodCollisionAcrossPackages(t *testing.T) {
	// A saga consuming two events that share a short name from different packages
	// generates the same handler method twice — uncompilable. ANZ011.
	o := buildOptionTypes(t, ioPkg)
	gen := buildGenMultiPkg(t, o)
	diags := codegen.Lint(gen)
	if !hasCode(diags, "ANZ011") {
		t.Fatalf("want ANZ011 duplicate method, got %v", codesOf(diags))
	}
}

func TestLint_TierC_EmitWithoutApplier_Warns(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	// Aggregate emits OrderCreated but declares no applier for it.
	diags := lint(t,
		declMsg{"State", o.componentDecl(1, "orders", "", "OrderAggregate")},
		declMsg{"CreateOrder", o.commandDecl(fq("State"), fq("OrderCreated"))},
		declMsg{"OrderCreated", nil}, // a plain message: resolvable, but no (event) applier
	)
	sev, ok := severityOf(diags, "ANZ100")
	if !ok {
		t.Fatalf("want ANZ100 emit-without-applier, got %v", codesOf(diags))
	}
	if sev != codegen.SeverityWarning {
		t.Errorf("ANZ100 should be a warning, got %v", sev)
	}
	if codegen.HasErrors(diags) {
		t.Errorf("emit-without-applier must not block generation: %v", diags)
	}
}

func TestLint_TierC_OrphanComponent_Warns(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	diags := lint(t, declMsg{"ProjState", o.componentDecl(4, "counter", "", "Proj")})
	if !hasCode(diags, "ANZ103") {
		t.Fatalf("want ANZ103 orphan component, got %v", codesOf(diags))
	}
	if codegen.HasErrors(diags) {
		t.Errorf("orphan component must not block generation: %v", diags)
	}
}

func TestLint_TierC_DanglingDomains_Warn(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	// A saga whose output_domain has no consuming aggregate (ANZ101) and whose
	// trigger source domain has no producing aggregate (ANZ102).
	diags := lint(t,
		declMsg{"OrderSaga", o.componentDecl(2, "orders", "fulfillment", "")},
		declMsg{"OrderPlaced", o.eventDecl(eventEntry{component: fq("OrderSaga"), domain: "orders"})},
	)
	if !hasCode(diags, "ANZ101") {
		t.Errorf("want ANZ101 dangling output_domain, got %v", codesOf(diags))
	}
	if !hasCode(diags, "ANZ102") {
		t.Errorf("want ANZ102 dangling trigger domain, got %v", codesOf(diags))
	}
	if codegen.HasErrors(diags) {
		t.Errorf("dangling domains must not block generation: %v", diags)
	}
}

func TestLint_DiagnosticString_Format(t *testing.T) {
	d := codegen.Diagnostic{
		Severity: codegen.SeverityError,
		Code:     "ANZ002",
		Message:  "boom",
		Pos:      codegen.Position{File: "x.proto", Line: 3, Col: 5},
	}
	if got := d.String(); got != "x.proto:3:5: error[ANZ002]: boom" {
		t.Errorf("String() = %q", got)
	}
	d.Pos = codegen.Position{File: "x.proto"} // no line info
	if got := d.String(); got != "x.proto: error[ANZ002]: boom" {
		t.Errorf("String() without line = %q", got)
	}
}

// buildGenMultiPkg builds a two-file request: file A (testPkg) holds the saga
// anchor and a "Ping" event; file B (a second package) holds another "Ping"
// event. Both events name the saga as their consuming component, so the saga
// generates the handler method "Ping" twice.
func buildGenMultiPkg(t *testing.T, o optionTypes) *protogen.Plugin {
	t.Helper()
	const pkgB = "validation.other"
	const pathB = "other_test.proto"

	fileA := &descriptorpb.FileDescriptorProto{
		Name:       str(testPath),
		Package:    str(testPkg),
		Syntax:     str("proto3"),
		Dependency: []string{optionsPath},
		Options:    &descriptorpb.FileOptions{GoPackage: str("example.test/a;a")},
		MessageType: []*descriptorpb.DescriptorProto{
			{Name: str("OrderSaga"), Options: o.componentDecl(2, "orders", "fulfillment", "")},
			{Name: str("Ping"), Options: o.eventDecl(eventEntry{component: fq("OrderSaga"), domain: "orders"})},
		},
	}
	fileB := &descriptorpb.FileDescriptorProto{
		Name:       str(pathB),
		Package:    str(pkgB),
		Syntax:     str("proto3"),
		Dependency: []string{optionsPath},
		Options:    &descriptorpb.FileOptions{GoPackage: str("example.test/b;b")},
		MessageType: []*descriptorpb.DescriptorProto{
			{Name: str("Ping"), Options: o.eventDecl(eventEntry{component: fq("OrderSaga"), domain: "orders"})},
		},
	}
	gen, err := protogen.Options{}.New(&pluginpb.CodeGeneratorRequest{
		FileToGenerate: []string{testPath, pathB},
		ProtoFile: []*descriptorpb.FileDescriptorProto{
			protodesc.ToFileDescriptorProto(descriptorpb.File_google_protobuf_descriptor_proto),
			optionsFDP(ioPkg),
			fileA,
			fileB,
		},
	})
	if err != nil {
		t.Fatalf("buildGenMultiPkg: %v", err)
	}
	return gen
}
