package codegen_test

import (
	"strings"
	"testing"

	"google.golang.org/protobuf/types/descriptorpb"

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
			`dispatch.OnFact("validation.test.Shipped", func(factAny *anypb.Any, state *State) (*anypb.Any, error) {`,
			"recorded, err := h.OnShippedFact(fact, state)",
			"OnShippedFact(fact *Shipped, state *State) (*Shipped, error)",
		},
		"python": {
			`dispatch.on_fact("validation.test.Shipped", _fact_on_shipped_fact)`,
			"def on_shipped_fact(self, fact: _validation_test.Shipped, state: _validation_test.State) -> Optional[_validation_test.Shipped]",
		},
		"java": {
			`.onFact("validation.test.Shipped", (factAny, state) -> {`,
			"onShippedFact(validation.test.ValidationTest.Shipped fact, validation.test.ValidationTest.State.Builder state)",
		},
		"csharp": {
			`.OnFact("validation.test.Shipped", (factAny, state) =>`,
			"Validation.Test.Shipped? OnShippedFact(Validation.Test.Shipped fact, Validation.Test.State state)",
		},
		"cpp": {
			`dispatch.OnFact("validation.test.Shipped", [&h](const google::protobuf::Any& factAny, const validation::test::State& state) -> std::optional<google::protobuf::Any> {`,
			"std::optional<validation::test::Shipped> OnShippedFact(const validation::test::Shipped& fact, const validation::test::State& state)",
		},
		"typescript": {
			`dispatch.onFact("validation.test.Shipped", (factAny, state) => {`,
			"onShippedFact(fact: Shipped, state: State): Shipped | undefined",
		},
	}
	for _, lang := range codegen.Languages() {
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
				"go": "OnShippedFact(", "python": "def on_shipped_fact(", "java": "onShippedFact(",
				"csharp": "OnShippedFact(", "cpp": "OnShippedFact(", "typescript": "onShippedFact(",
			}[lang]) {
				t.Errorf("%s scaffold stub lacks the fact handler:\n%s", lang, stub.File[0].GetContent())
			}
		})
	}
}

func TestGeneratePython_ImportsFactModuleFromAnotherPackage(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	w := &codegenWorld{t: t, o: o, msgs: map[string]*descriptorpb.MessageOptions{}, anchors: map[string]string{}}
	w.plain("shipping.ShipmentDispatched")
	w.declare("order.OrderState", o.withList(o.ownedDecl(1, "order", "", ""), "facts", "shipping.ShipmentDispatched"))
	gen, err := w.plugin()
	if err != nil {
		t.Fatal(err)
	}
	if err := codegen.Generate(gen, "python", codegen.Options{}); err != nil {
		t.Fatalf("Generate: %v", err)
	}
	var c string
	for _, f := range gen.Response().File {
		if strings.Contains(f.GetName(), "order_state") {
			c = f.GetContent()
		}
	}
	if !strings.Contains(c, "shipping import decl_pb2 as _decl1") || !strings.Contains(c, "fact: _decl1.ShipmentDispatched") {
		t.Errorf("python wiring must import the fact's module:\n%s", c)
	}
}
