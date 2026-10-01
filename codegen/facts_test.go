package codegen_test

import (
	"strings"
	"testing"

	"github.com/angzarr-io/angzarr-cli/codegen"
)

func TestLint_FactsDeclarations(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	agg := func(facts ...string) declMsg {
		return declMsg{"State", o.withList(o.ownedDecl(1, "orders", "", "OrderAggregate"), "facts", facts...)}
	}
	shipped, delayed := fq("Shipped"), fq("Delayed")
	plain := []declMsg{{"Shipped", nil}, {"Delayed", nil}}
	cases := []struct {
		name string
		msgs []declMsg
		want string // "" = none of ANZ011/ANZ014/ANZ017/ANZ018
	}{
		{"aggregate facts", []declMsg{agg(shipped)}, ""},
		{"unresolvable fact", []declMsg{agg(fq("Nope"))}, "ANZ017"},
		{"short fact name", []declMsg{agg("Shipped")}, "ANZ017"},
		{"duplicate fact", []declMsg{agg(shipped, shipped)}, "ANZ011"},
		{"emits_facts on an aggregate", []declMsg{{"State", o.withList(o.ownedDecl(1, "orders", "", "OrderAggregate"), "emits_facts", shipped)}}, "ANZ014"},
		{"emits_facts on a projector", []declMsg{{"Proj", o.withList(o.componentDecl(4, "orders", "", ""), "emits_facts", shipped)}}, "ANZ014"},
		{"saga emits a declared fact", []declMsg{agg(shipped), {"ShipSaga", o.withList(o.componentDecl(2, "shipping", "orders", ""), "emits_facts", shipped)}}, ""},
		{"saga emits an undeclared fact", []declMsg{agg(shipped), {"ShipSaga", o.withList(o.componentDecl(2, "shipping", "orders", ""), "emits_facts", delayed)}}, "ANZ018"},
		{"saga emits into a domain no aggregate owns", []declMsg{agg(shipped), {"ShipSaga", o.withList(o.componentDecl(2, "shipping", "billing", ""), "emits_facts", shipped)}}, "ANZ018"},
		{"pm emits a declared fact", []declMsg{agg(shipped), {"FlowState", o.withList(o.ownedDecl(3, "flow", "orders", "Flow"), "emits_facts", shipped)}}, ""},
		{"pm emits an undeclared fact", []declMsg{agg(shipped), {"FlowState", o.withList(o.ownedDecl(3, "flow", "orders", "Flow"), "emits_facts", delayed)}}, "ANZ018"},
		{"unresolvable emitted fact", []declMsg{agg(shipped), {"ShipSaga", o.withList(o.componentDecl(2, "shipping", "orders", ""), "emits_facts", fq("Nope"))}}, "ANZ017"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			diags := lint(t, append(append([]declMsg{}, plain...), tc.msgs...)...)
			for _, code := range []string{"ANZ011", "ANZ014", "ANZ017", "ANZ018"} {
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

func TestGenerate_FactHandlersInEveryLanguage(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	msgs := []declMsg{
		{"State", o.withList(o.ownedDecl(1, "orders", "", "OrderAggregate"), "facts", fq("Shipped"))},
		{"PlaceOrder", o.commandDecl(fq("State"))},
		{"Shipped", nil},
	}
	want := map[string][]string{
		"go": {
			`dispatch.OnFact("validation.test.Shipped", func(factAny *anypb.Any, state *State) (_go.FactRecord, error) {`,
			"rec, err := h.OnShippedFact(fact, state)",
			"rec.Fact = factAny",
			"OnShippedFact(fact *Shipped, state *State) (_go.FactRecord, error)",
		},
		"java": {
			`.onFact("validation.test.Shipped", (factAny, state) -> {`,
			"return rec == null ? io.angzarr.router.FactRecord.of(factAny) : rec;",
			"io.angzarr.router.FactRecord onShippedFact(validation.test.ValidationTest.Shipped fact, validation.test.ValidationTest.State.Builder state)",
		},
		"csharp": {
			`.OnFact("validation.test.Shipped", (factAny, state) =>`,
			"?? Angzarr.Router.FactRecord.AsReceived(factAny);",
			"Angzarr.Router.FactRecord? OnShippedFact(Validation.Test.Shipped fact, Validation.Test.State state)",
		},
		"cpp": {
			`dispatch.OnFact("validation.test.Shipped", [&h](const google::protobuf::Any& factAny, const validation::test::State& state) -> angzarr::router::FactRecord {`,
			"if (!rec) return factAny;",
			"std::optional<angzarr::router::FactRecord> OnShippedFact(const validation::test::Shipped& fact, const validation::test::State& state)",
		},
		"typescript": {
			`dispatch.onFact("validation.test.Shipped", (factAny, state) =>`,
			"?? FactRecord.asReceived(factAny)",
			"onShippedFact(fact: Shipped, state: State): FactRecord | undefined",
		},
	}
	for _, lang := range codegen.BuiltinLanguages() {
		t.Run(lang, func(t *testing.T) {
			resp, err := generate(t, lang, ioPkg, msgs...)
			if err != nil {
				t.Fatalf("Generate: %v", err)
			}
			c := resp.File[0].GetContent()
			for _, w := range want[lang] {
				if !strings.Contains(c, w) {
					t.Errorf("%s wiring missing %q:\n%s", lang, w, c)
				}
			}
			stub, err := scaffold(t, lang, ioPkg, func(string) bool { return false }, msgs...)
			if err != nil {
				t.Fatalf("GenerateScaffold: %v", err)
			}
			if !strings.Contains(strings.ToLower(stub.File[0].GetContent()), "shipped") || !strings.Contains(stub.File[0].GetContent(), map[string]string{
				"go": "OnShippedFact(", "java": "onShippedFact(",
				"csharp": "OnShippedFact(", "cpp": "OnShippedFact(", "typescript": "onShippedFact(",
			}[lang]) {
				t.Errorf("%s scaffold stub lacks the fact handler:\n%s", lang, stub.File[0].GetContent())
			}
		})
	}
}
