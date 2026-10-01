package codegen_test

// Lint reuses the in-memory descriptor harness from generate_test.go
// (buildGen / buildOptionTypes / optionTypes / declMsg / orderAggregate). These
// tests assert the collect-all behaviour, the stable ANZxxxx codes, and the
// error-vs-warning split that gates code generation.

import (
	"strings"
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
			{"State", o.ownedDecl(1, "orders", "", "")},
			{"CreateOrder", o.commandDecl(fq("State"), fq("Nope"))},
		}},
		{"event to unknown component", "ANZ005", []declMsg{
			{"OrderCreated", o.eventDecl(eventEntry{component: fq("Nope")})},
		}},
		{"process manager trigger without domain", "ANZ006", []declMsg{
			{"PMState", o.ownedDecl(3, "workflow", "fulfillment", "")},
			{"Trig", o.eventDecl(eventEntry{component: fq("PMState")})},
		}},
		{"compensates unresolvable", "ANZ007", []declMsg{
			{"State", o.ownedDecl(1, "orders", "", "", fq("Nope"))},
		}},
		{"aggregate without domain", "ANZ008", []declMsg{
			{"State", o.ownedDecl(1, "", "", "")},
		}},
		{"saga without output domain", "ANZ008", []declMsg{
			{"OrderSaga", o.componentDecl(2, "orders", "", "")},
		}},
		{"process manager without domain", "ANZ008", []declMsg{
			{"PMState", o.componentDecl(3, "", "fulfillment", "")},
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

// TestLint_Projector_InputDomainNotRequired: a projector may declare its
// domains per event, so input_domain is not required (no ANZ008).
func TestLint_Projector_InputDomainNotRequired(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	diags := lint(t,
		declMsg{"Projection", o.componentDecl(4, "", "", "MultiDomainProjector")},
		declMsg{"TableCreated", o.eventDecl(eventEntry{component: fq("Projection"), domain: "table"})},
		declMsg{"HandStarted", o.eventDecl(eventEntry{component: fq("Projection"), domain: "hand"})},
	)
	if hasCode(diags, "ANZ008") {
		t.Errorf("projector with handlers but no input_domain must not require it, got %v", codesOf(diags))
	}
}

func TestLint_DuplicateGeneratedName(t *testing.T) {
	// Two components that resolve to the same generated base name would emit
	// colliding Go/Python identifiers — uncompilable. ANZ010.
	o := buildOptionTypes(t, ioPkg)
	diags := lint(t,
		declMsg{"StateA", o.ownedDecl(1, "orders", "", "Dup")},
		declMsg{"StateB", o.ownedDecl(1, "orders", "", "Dup")},
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

func TestLint_TierB_MethodCollisionCrossCategory(t *testing.T) {
	// A command "ApplyCredit" (handler method "ApplyCredit") and an applier for
	// event "Credit" (applierName = "Apply"+"Credit" = "ApplyCredit") land on
	// the same aggregate and generate the same interface method twice — the
	// escape the per-category check (handler vs. applier) missed. ANZ011.
	o := buildOptionTypes(t, ioPkg)
	diags := lint(t,
		declMsg{"State", o.ownedDecl(1, "orders", "", "")},
		declMsg{"ApplyCredit", o.commandDecl(fq("State"))},
		declMsg{"Credit", o.eventDecl(eventEntry{component: fq("State")})},
	)
	if !hasCode(diags, "ANZ011") {
		t.Fatalf("want ANZ011 cross-category method collision, got %v", codesOf(diags))
	}
	if sev, _ := severityOf(diags, "ANZ011"); sev != codegen.SeverityError {
		t.Errorf("ANZ011 should be an error, got %v", sev)
	}
}

func TestLint_TierB_MethodCollisionRejectionsAcrossPackages(t *testing.T) {
	// compensates: ["shop.a.Reserve", "shop.b.Reserve"] — two different
	// commands from different packages that share a short name — both
	// generate the rejection method "OnReserveRejected". Rejections were
	// excluded from the duplicate check entirely; this is the escape. ANZ011.
	o := buildOptionTypes(t, ioPkg)
	gen := buildGenRejectionCollisionAcrossPackages(t, o)
	diags := codegen.Lint(gen)
	if !hasCode(diags, "ANZ011") {
		t.Fatalf("want ANZ011 duplicate rejection method across packages, got %v", codesOf(diags))
	}
}

func TestLint_TierB_MethodCollisionDuplicateCompensatesEntry(t *testing.T) {
	// A literal duplicate compensates entry on one component: the same
	// fully-qualified command listed twice generates "On<Short>Rejected"
	// twice for the same aggregate. ANZ011.
	o := buildOptionTypes(t, ioPkg)
	diags := lint(t,
		declMsg{"State", o.ownedDecl(1, "orders", "", "", fq("Reserve"), fq("Reserve"))},
		declMsg{"Reserve", nil},
	)
	if !hasCode(diags, "ANZ011") {
		t.Fatalf("want ANZ011 duplicate compensates entry, got %v", codesOf(diags))
	}
}

func TestLint_TierB_ProjectorHandlerCollidesWithFinish(t *testing.T) {
	// Every projector interface carries a fixed Finish method; a consumed
	// event named "Finish" generates a second one. ANZ011.
	o := buildOptionTypes(t, ioPkg)
	diags := lint(t,
		declMsg{"Projection", o.componentDecl(4, "orders", "", "")},
		declMsg{"Finish", o.eventDecl(eventEntry{component: fq("Projection")})},
	)
	if !hasCode(diags, "ANZ011") {
		t.Fatalf("want ANZ011 for a projector handler named Finish, got %v", codesOf(diags))
	}
}

func TestLint_TierB_FinishIsOnlyReservedOnProjectors(t *testing.T) {
	// An aggregate has no Finish method, so a command named Finish is fine.
	o := buildOptionTypes(t, ioPkg)
	diags := lint(t,
		declMsg{"State", o.ownedDecl(1, "orders", "", "")},
		declMsg{"Finish", o.commandDecl(fq("State"))},
	)
	if hasCode(diags, "ANZ011") {
		t.Fatalf("aggregate command named Finish must not collide, got %v", codesOf(diags))
	}
}

func TestLint_TierB_MethodCollisionAfterLanguageCasing(t *testing.T) {
	// "HTTPGet" and "HttpGet" are distinct Go/Java/C#/C++/TS methods but both
	// render as Python's http_get. ANZ011 names the colliding language.
	o := buildOptionTypes(t, ioPkg)
	diags := lint(t,
		declMsg{"State", o.ownedDecl(1, "orders", "", "")},
		declMsg{"HTTPGet", o.commandDecl(fq("State"))},
		declMsg{"HttpGet", o.commandDecl(fq("State"))},
	)
	var msg string
	for _, d := range diags {
		if d.Code == "ANZ011" {
			msg = d.Message
		}
	}
	if msg == "" {
		t.Fatalf("want ANZ011 for http_get python collision, got %v", codesOf(diags))
	}
	if !strings.Contains(msg, "http_get") || !strings.Contains(msg, "python") {
		t.Errorf("ANZ011 message should name the python method http_get, got %q", msg)
	}
}

func TestLint_TierB_DistinctNamesDoNotCollide(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	diags := lint(t,
		declMsg{"State", o.ownedDecl(1, "orders", "", "")},
		declMsg{"HttpGet", o.commandDecl(fq("State"))},
		declMsg{"HttpPut", o.commandDecl(fq("State"))},
	)
	if hasCode(diags, "ANZ011") {
		t.Fatalf("distinct methods flagged as colliding: %v", diags)
	}
}

// buildGenRejectionCollisionAcrossPackages builds a two-file request: file A
// (testPkg "shop.a") holds the compensating component and a "Reserve"
// message; file B ("shop.b") holds another "Reserve" message. The component's
// compensates references both fully-qualified names, so it generates the
// rejection method "OnReserveRejected" twice.
func buildGenRejectionCollisionAcrossPackages(t *testing.T, o optionTypes) *protogen.Plugin {
	t.Helper()
	const pkgA = "shop.a"
	const pkgB = "shop.b"
	const pathA = "shop_a_test.proto"
	const pathB = "shop_b_test.proto"

	fileA := &descriptorpb.FileDescriptorProto{
		Name:       str(pathA),
		Package:    str(pkgA),
		Syntax:     str("proto3"),
		Dependency: []string{optionsPath},
		Options:    &descriptorpb.FileOptions{GoPackage: str("example.test/a;a")},
		MessageType: []*descriptorpb.DescriptorProto{
			{Name: str("State"), Options: o.ownedDecl(1, "orders", "", "", pkgA+".Reserve", pkgB+".Reserve")},
			{Name: str("Reserve")},
		},
	}
	fileB := &descriptorpb.FileDescriptorProto{
		Name:       str(pathB),
		Package:    str(pkgB),
		Syntax:     str("proto3"),
		Dependency: []string{optionsPath},
		Options:    &descriptorpb.FileOptions{GoPackage: str("example.test/b;b")},
		MessageType: []*descriptorpb.DescriptorProto{
			{Name: str("Reserve")},
		},
	}
	gen, err := protogen.Options{}.New(&pluginpb.CodeGeneratorRequest{
		FileToGenerate: []string{pathA, pathB},
		ProtoFile: []*descriptorpb.FileDescriptorProto{
			protodesc.ToFileDescriptorProto(descriptorpb.File_google_protobuf_descriptor_proto),
			optionsFDP(ioPkg),
			fileA,
			fileB,
		},
	})
	if err != nil {
		t.Fatalf("buildGenRejectionCollisionAcrossPackages: %v", err)
	}
	return gen
}

func TestLint_TierC_EmitWithoutApplier_Warns(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	// Aggregate emits OrderCreated but declares no applier for it.
	diags := lint(t,
		declMsg{"State", o.ownedDecl(1, "orders", "", "OrderAggregate")},
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

func TestLint_TierB_GeneratedTypeCollidesWithProtoType(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	cases := []struct {
		name string
		msgs []declMsg
	}{
		{"explicit name equals the anchor", []declMsg{
			{"OrderState", o.ownedDecl(1, "orders", "", "OrderState")},
		}},
		{"explicit name equals another message", []declMsg{
			{"OrderState", o.ownedDecl(1, "orders", "", "Order")},
			{"Order", nil},
		}},
		{"derived handler interface equals a message", []declMsg{
			{"OrderState", o.ownedDecl(1, "orders", "", "Order")},
			{"OrderHandler", nil},
		}},
		{"unnamed stub type equals a message", []declMsg{
			{"OrderSaga", o.componentDecl(2, "orders", "fulfillment", "")},
			{"OrderSagaImpl", nil},
		}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			diags := lint(t, tc.msgs...)
			if !hasCode(diags, "ANZ012") {
				t.Fatalf("want ANZ012, got %v", codesOf(diags))
			}
			if sev, _ := severityOf(diags, "ANZ012"); sev != codegen.SeverityError {
				t.Errorf("ANZ012 should be an error, got %v", sev)
			}
		})
	}
}

func TestLint_TierB_UnnamedAndDistinctNamesDoNotCollideWithProtoTypes(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	diags := lint(t,
		declMsg{"OrderSaga", o.componentDecl(2, "orders", "fulfillment", "")},
		declMsg{"OrderState", o.ownedDecl(1, "orders", "", "OrderAggregate")},
	)
	if hasCode(diags, "ANZ012") {
		t.Fatalf("no generated identifier shadows a proto type here, got %v", diags)
	}
}

func TestDuplicateAnchorFullNames_RejectedBeforeAnalysis(t *testing.T) {
	// Components are keyed by anchor full name; protogen refuses a request
	// declaring one full name twice, so analysis never sees duplicates.
	o := buildOptionTypes(t, ioPkg)
	_, err := buildGen(t, ioPkg,
		declMsg{"State", o.ownedDecl(1, "orders", "", "A")},
		declMsg{"State", o.ownedDecl(1, "orders", "", "B")},
	)
	if err == nil {
		t.Fatal("protogen accepted two messages with the same full name")
	}
}

func TestLint_DomainRolesPerKind(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	cases := []struct {
		name string
		decl *descriptorpb.MessageOptions
		want string // "" = no ANZ008/ANZ014
	}{
		{"aggregate owns its domain", o.ownedDecl(1, "orders", "", ""), ""},
		{"aggregate without domain", o.componentDecl(1, "", "", ""), "ANZ008"},
		{"aggregate subscribing via input_domain", o.withField(o.ownedDecl(1, "orders", "", ""), "input_domain", "orders"), "ANZ014"},
		{"aggregate with a command target", o.ownedDecl(1, "orders", "billing", ""), "ANZ014"},
		{"aggregate with output_domains", o.withOutputDomains(o.ownedDecl(1, "orders", "", ""), "billing"), "ANZ014"},
		{"process manager owns its workflow domain", o.ownedDecl(3, "workflow", "billing", ""), ""},
		{"process manager without targets", o.ownedDecl(3, "workflow", "", ""), ""},
		{"process manager without domain", o.componentDecl(3, "", "billing", ""), "ANZ008"},
		{"process manager with input_domain", o.withField(o.ownedDecl(3, "workflow", "billing", ""), "input_domain", "orders"), "ANZ014"},
		{"saga", o.componentDecl(2, "orders", "billing", ""), ""},
		{"saga owning a domain", o.withField(o.componentDecl(2, "orders", "billing", ""), "domain", "orders"), "ANZ014"},
		{"projector", o.componentDecl(4, "orders", "", ""), ""},
		{"projector owning a domain", o.withField(o.componentDecl(4, "orders", "", ""), "domain", "orders"), "ANZ014"},
		{"projector with a command target", o.componentDecl(4, "orders", "billing", ""), "ANZ014"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			diags := lint(t, declMsg{"Anchor", tc.decl})
			for _, code := range []string{"ANZ008", "ANZ014"} {
				if got := hasCode(diags, code); got != (code == tc.want) {
					t.Errorf("%s present = %v, want %v (diags %v)", code, got, code == tc.want, diags)
				}
			}
			if tc.want != "" {
				if sev, _ := severityOf(diags, tc.want); sev != codegen.SeverityError {
					t.Errorf("%s should be an error", tc.want)
				}
			}
		})
	}
}

func TestLint_CoherenceUsesAggregateOwnDomain(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	diags := lint(t,
		declMsg{"State", o.ownedDecl(1, "billing", "", "BillingAggregate")},
		declMsg{"Charge", o.commandDecl(fq("State"))},
		declMsg{"OrderSaga", o.componentDecl(2, "billing", "billing", "")},
		declMsg{"Charged", o.eventDecl(eventEntry{component: fq("OrderSaga"), domain: "billing"})},
	)
	if hasCode(diags, "ANZ101") || hasCode(diags, "ANZ102") {
		t.Fatalf("billing is an aggregate's own domain; no dangling-domain warning expected, got %v", diags)
	}
}

func TestLint_Undoes(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	agg := func(undoes ...string) []declMsg {
		return []declMsg{
			{"State", o.withUndoes(o.ownedDecl(1, "orders", "", "OrderAggregate"), undoes...)},
			{"ReserveStock", o.commandDecl(fq("State"))},
			{"Other", nil},
		}
	}
	cases := []struct {
		name string
		msgs []declMsg
		want string // "" = none of ANZ011/ANZ014/ANZ015
	}{
		{"aggregate undoing a command it handles", agg(fq("ReserveStock")), ""},
		{"undo of a message that is not its command", agg(fq("Other")), "ANZ015"},
		{"undo of an unresolvable command", agg(fq("Nope")), "ANZ015"},
		{"undo by short name", agg("ReserveStock"), "ANZ015"},
		{"duplicate undo entry", agg(fq("ReserveStock"), fq("ReserveStock")), "ANZ011"},
		{"process manager undoes", []declMsg{
			{"PMState", o.withUndoes(o.ownedDecl(3, "workflow", "orders", ""), fq("ReserveStock"))},
			{"ReserveStock", nil},
		}, "ANZ014"},
		{"saga undoes", []declMsg{
			{"OrderSaga", o.withUndoes(o.componentDecl(2, "orders", "billing", ""), fq("ReserveStock"))},
			{"ReserveStock", nil},
		}, "ANZ014"},
		{"projector undoes", []declMsg{
			{"Projection", o.withUndoes(o.componentDecl(4, "orders", "", ""), fq("ReserveStock"))},
			{"ReserveStock", nil},
		}, "ANZ014"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			diags := lint(t, tc.msgs...)
			for _, code := range []string{"ANZ011", "ANZ014", "ANZ015"} {
				if got := hasCode(diags, code); got != (code == tc.want) {
					t.Errorf("%s present = %v, want %v (diags %v)", code, got, code == tc.want, diags)
				}
			}
			if tc.want != "" {
				if sev, _ := severityOf(diags, tc.want); sev != codegen.SeverityError {
					t.Errorf("%s should be an error", tc.want)
				}
			}
		})
	}
}

func TestLint_UndoMethodSharesTheNamespace(t *testing.T) {
	// undoes ReserveStock generates OnReserveStockUndo; a command of that
	// name collides with it.
	o := buildOptionTypes(t, ioPkg)
	diags := lint(t,
		declMsg{"State", o.withUndoes(o.ownedDecl(1, "orders", "", "OrderAggregate"), fq("ReserveStock"))},
		declMsg{"ReserveStock", o.commandDecl(fq("State"))},
		declMsg{"OnReserveStockUndo", o.commandDecl(fq("State"))},
	)
	if !hasCode(diags, "ANZ011") {
		t.Fatalf("want ANZ011 for OnReserveStockUndo, got %v", diags)
	}
}

func TestLint_CompensatesEntries(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	agg := func(entries ...string) []declMsg {
		return []declMsg{
			{"State", o.ownedDecl(1, "orders", "", "OrderAggregate", entries...)},
			{"ReserveStock", nil},
			{"ChargeCard", nil},
		}
	}
	rs, cc := fq("ReserveStock"), fq("ChargeCard")
	cases := []struct {
		name string
		msgs []declMsg
		want string // "" = none of ANZ007/ANZ014/ANZ016
	}{
		{"unqualified", agg(rs), ""},
		{"qualified", agg("inventory:" + rs), ""},
		{"one type qualified by two domains", agg("inventory:"+rs, "warehouse:"+rs), ""},
		{"two types, mixed forms", agg(rs, "billing:"+cc), ""},
		{"type both unqualified and qualified", agg(rs, "inventory:"+rs), "ANZ016"},
		{"type unqualified twice", agg(rs, rs), "ANZ016"},
		{"type qualified by one domain twice", agg("inventory:"+rs, "inventory:"+rs), "ANZ016"},
		{"empty domain", agg(":" + rs), "ANZ007"},
		{"domain with a colon", agg("a:b:" + rs), "ANZ007"},
		{"qualified unresolvable type", agg("inventory:" + fq("Nope")), "ANZ007"},
		{"qualified short name", agg("inventory:ReserveStock"), "ANZ007"},
		{"process manager compensates", []declMsg{
			{"PMState", o.ownedDecl(3, "workflow", "inventory", "", "inventory:"+rs)},
			{"ReserveStock", nil},
		}, ""},
		{"saga compensates", []declMsg{
			{"OrderSaga", o.componentDecl(2, "orders", "inventory", "", rs)},
			{"ReserveStock", nil},
		}, "ANZ014"},
		{"projector compensates", []declMsg{
			{"Projection", o.componentDecl(4, "orders", "", "", rs)},
			{"ReserveStock", nil},
		}, "ANZ014"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			diags := lint(t, tc.msgs...)
			for _, code := range []string{"ANZ007", "ANZ014", "ANZ016"} {
				if got := hasCode(diags, code); got != (code == tc.want) {
					t.Errorf("%s present = %v, want %v (diags %v)", code, got, code == tc.want, diags)
				}
			}
			if tc.want == "" && hasCode(diags, "ANZ011") {
				t.Errorf("valid entries must not collide as methods: %v", diags)
			}
		})
	}
}

func TestGenerate_QualifiedCompensatesRegistersTheEntry(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	msgs := []declMsg{
		{"State", o.ownedDecl(1, "orders", "", "OrderAggregate", "inventory:"+fq("ReserveStock"), "warehouse:"+fq("ReserveStock"), fq("ChargeCard"))},
		{"ReserveStock", nil},
		{"ChargeCard", nil},
		{"CreateOrder", o.commandDecl(fq("State"))},
	}
	resp, err := generate(t, "go", ioPkg, msgs...)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	c := resp.File[0].GetContent()
	for _, want := range []string{
		`dispatch.OnRejected("inventory:validation.test.ReserveStock", h.OnReserveStockFromInventoryRejected)`,
		`dispatch.OnRejected("warehouse:validation.test.ReserveStock", h.OnReserveStockFromWarehouseRejected)`,
		`dispatch.OnRejected("validation.test.ChargeCard", h.OnChargeCardRejected)`,
	} {
		if !strings.Contains(c, want) {
			t.Errorf("missing %s:\n%s", want, c)
		}
	}
}

func TestGenerate_QualifiedCompensatesKeyInEveryLanguage(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	msgs := []declMsg{
		{"State", o.ownedDecl(1, "orders", "", "OrderAggregate", "inventory:"+fq("ReserveStock"))},
		{"ReserveStock", nil},
		{"CreateOrder", o.commandDecl(fq("State"))},
	}
	for _, lang := range codegen.BuiltinLanguages() {
		t.Run(lang, func(t *testing.T) {
			resp, err := generate(t, lang, ioPkg, msgs...)
			if err != nil {
				t.Fatalf("Generate: %v", err)
			}
			c := resp.File[0].GetContent()
			if !strings.Contains(c, `"inventory:validation.test.ReserveStock"`) {
				t.Errorf("%s wiring does not register the qualified entry:\n%s", lang, c)
			}
			if !strings.Contains(strings.ToLower(c), "onreservestockfrominventoryrejected") && !strings.Contains(c, "on_reserve_stock_from_inventory_rejected") {
				t.Errorf("%s wiring lacks the qualified handler method:\n%s", lang, c)
			}
		})
	}
}
