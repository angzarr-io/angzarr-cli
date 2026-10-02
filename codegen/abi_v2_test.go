package codegen_test

import (
	"strings"
	"testing"

	"github.com/angzarr-io/angzarr-cli/codegen"
)

// abiV2Surface declares one aggregate with an undo handler, one process
// manager with command targets, a trigger and a compensator, and one
// projector — every handler shape the router's ABI v2 changed.
func abiV2Surface(o optionTypes) []declMsg {
	return []declMsg{
		{"State", o.withUndoes(o.ownedDecl(1, "orders", "", "OrderAggregate"), fq("ReserveStock"))},
		{"ReserveStock", o.commandDecl(fq("State"))},
		{"FlowState", o.withOutputDomains(o.ownedDecl(3, "flow", "inventory", "Flow", fq("ReserveStock")), "billing")},
		{"Ledger", o.componentDecl(4, "orders", "", "LedgerProjector")},
		{"OrderPlaced", o.eventDecl(
			eventEntry{component: fq("FlowState"), domain: "orders"},
			eventEntry{component: fq("Ledger"), domain: "orders"},
		)},
	}
}

// abiV2Want is each language's expected text per component, as substrings
// of that component's wiring file.
var abiV2Want = map[string]map[string][]string{
	"go": {
		"OrderAggregate": {
			`dispatch.OnUndo("validation.test.ReserveStock", h.OnReserveStockUndo)`,
			"OnReserveStockUndo(n *", "compensate *", "Compensate, state *State, cctx ",
		},
		"Flow": {
			`NewProcessManagerDispatch("Flow", "flow", rebuilder, "inventory", "billing")`,
			`dispatch.OnEventWithCover("orders", "validation.test.OrderPlaced", func(eventAny *anypb.Any, state *FlowState, dests *_go.Destinations, triggerCover *v1.Cover)`,
			"OrderPlaced(event *OrderPlaced, state *FlowState, dests *_go.Destinations, triggerCover *v1.Cover) (*v1.ProcessManagerHandleResponse, error)",
			`dispatch.OnRejectedResponse("validation.test.ReserveStock", h.OnReserveStockRejected)`,
			"OnReserveStockRejected(n *v1.Notification, rejection *v1.RejectionNotification, state *FlowState) (*v1.ProcessManagerHandleResponse, error)",
		},
		"LedgerProjector": {
			`dispatch.OnEventWithContext("validation.test.OrderPlaced", func(projection *Ledger, eventAny *anypb.Any, ctx _go.PageContext) error {`,
			"OrderPlaced(projection *Ledger, event *OrderPlaced, ctx _go.PageContext) error",
		},
	},
	"python": {
		"OrderAggregate": {
			`dispatch.on_undo("validation.test.ReserveStock", handler.on_reserve_stock_undo)`,
			"def on_reserve_stock_undo(self, n: _t.Notification, compensate: _t.Compensate, state: _validation_test.State, cctx: _az.CommandContext) -> Optional[_ch.BusinessResponse]",
		},
		"Flow": {
			`_az.ProcessManagerDispatch("Flow", "flow", rebuilder, targets=["inventory", "billing"])`,
			`dispatch.on_event_with_cover("orders", "validation.test.OrderPlaced", _on_order_placed)`,
			"def order_placed(self, event: _validation_test.OrderPlaced, state: _validation_test.FlowState, dests: _az.Destinations, trigger_cover: Optional[_t.Cover]) -> _pm.ProcessManagerHandleResponse",
			"def on_reserve_stock_rejected(self, n: _t.Notification, rejection: _t.RejectionNotification, state: _validation_test.FlowState) -> Optional[_pm.ProcessManagerHandleResponse]",
		},
		"LedgerProjector": {
			`dispatch.on_event_with_context("validation.test.OrderPlaced", _on_order_placed)`,
			"def order_placed(self, projection: _validation_test.Ledger, event: _validation_test.OrderPlaced, ctx: _az.PageContext) -> None",
		},
	},
	"java": {
		"OrderAggregate": {
			`.onUndo("validation.test.ReserveStock", (n, compensate, state, cctx) ->`,
			"io.angzarr.BusinessResponse onReserveStockUndo(io.angzarr.Notification n, io.angzarr.Compensate compensate,",
		},
		"Flow": {
			`new io.angzarr.router.ProcessManagerDispatch("Flow", "flow", java.util.List.of("inventory", "billing"), rebuilder)`,
			`(eventAny, state, dests, triggerCover) ->`,
			"io.angzarr.Cover triggerCover)",
			`.onRejectedResponse("validation.test.ReserveStock",`,
			"io.angzarr.ProcessManagerHandleResponse onReserveStockRejected(",
		},
		"LedgerProjector": {
			`(projection, eventAny, ctx) ->`,
			"io.angzarr.router.PageContext ctx)",
		},
	},
	"csharp": {
		"OrderAggregate": {
			`.OnUndo("validation.test.ReserveStock", (n, compensate, state, cctx) =>`,
			"Angzarr.BusinessResponse OnReserveStockUndo(Angzarr.Notification n, Angzarr.Compensate compensate,",
		},
		"Flow": {
			`("Flow", "flow", new[] { "inventory", "billing" }, rebuilder)`,
			`(eventAny, state, dests, triggerCover) =>`,
			"Angzarr.Cover? triggerCover)",
			`.OnRejectedResponse("validation.test.ReserveStock",`,
			"Angzarr.ProcessManagerHandleResponse OnReserveStockRejected(",
		},
		"LedgerProjector": {
			`(projection, eventAny, page) =>`,
			"Angzarr.Router.PageContext page)",
		},
	},
	"cpp": {
		"OrderAggregate": {
			`dispatch.OnUndo("validation.test.ReserveStock", [&h](`,
			"const io::angzarr::v1::Compensate& compensate",
		},
		"Flow": {
			`dispatch("Flow", "flow", {"inventory", "billing"}, std::move(rebuilder));`,
			`dispatch.OnEventWithCover("orders", "validation.test.OrderPlaced", [&h](`,
			"const io::angzarr::v1::Cover& triggerCover",
			`dispatch.OnRejectedWithResponse("validation.test.ReserveStock", [&h](`,
			"io::angzarr::v1::ProcessManagerHandleResponse OnReserveStockRejected(",
		},
		"LedgerProjector": {
			`dispatch.OnEventWithContext("validation.test.OrderPlaced", [&h](`,
			"const angzarr::router::PageContext& ctx",
		},
	},
	"typescript": {
		"OrderAggregate": {
			`dispatch.onUndo("validation.test.ReserveStock", (n, compensate, state, cctx) =>`,
			"onReserveStockUndo(n: Notification, compensate: Compensate,",
		},
		"Flow": {
			`("Flow", "flow", rebuilder, ["inventory", "billing"]);`,
			`(eventAny, state, dests, triggerCover) =>`,
			"triggerCover?: Cover",
			"onReserveStockRejected(n: Notification, rejection: RejectionNotification, state: FlowState): ProcessManagerHandleResponse",
		},
		"LedgerProjector": {
			`(projection, eventAny, ctx) =>`,
			"ctx: PageContext",
		},
	},
}

func componentFileFor(t *testing.T, lang, base string, files map[string]string) string {
	t.Helper()
	for name, content := range files {
		low := strings.ToLower(strings.ReplaceAll(name, "_", ""))
		if strings.Contains(low, strings.ToLower(base)+"angzarr") {
			return content
		}
	}
	t.Fatalf("%s: no wiring file for %s among %v", lang, base, keys(files))
	return ""
}

func keys(m map[string]string) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	return out
}

func TestGenerate_ABIv2Surface(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	for _, lang := range codegen.Languages() {
		t.Run(lang, func(t *testing.T) {
			resp, err := generate(t, lang, ioPkg, abiV2Surface(o)...)
			if err != nil {
				t.Fatalf("Generate: %v", err)
			}
			files := map[string]string{}
			for _, f := range resp.File {
				files[f.GetName()] = f.GetContent()
			}
			for base, wants := range abiV2Want[lang] {
				content := componentFileFor(t, lang, base, files)
				for _, w := range wants {
					if !strings.Contains(content, w) {
						t.Errorf("%s %s wiring missing %q:\n%s", lang, base, w, content)
					}
				}
			}
		})
	}
}

func TestGenerateScaffold_ABIv2StubsImplementTheNewMethods(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	want := map[string]string{
		"go": "OnReserveStockUndo(", "python": "def on_reserve_stock_undo(", "java": "onReserveStockUndo(",
		"csharp": "OnReserveStockUndo(", "cpp": "OnReserveStockUndo(", "typescript": "onReserveStockUndo(",
	}
	for _, lang := range codegen.Languages() {
		t.Run(lang, func(t *testing.T) {
			resp, err := scaffold(t, lang, ioPkg, func(string) bool { return false }, abiV2Surface(o)...)
			if err != nil {
				t.Fatalf("GenerateScaffold: %v", err)
			}
			found := false
			for _, f := range resp.File {
				if strings.Contains(f.GetContent(), want[lang]) {
					found = true
				}
			}
			if !found {
				t.Errorf("%s: no scaffold stub declares %s", lang, want[lang])
			}
		})
	}
}

func TestGenerateCSharp_ProcessManagerWithoutTargets(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	resp, err := generate(t, "csharp", ioPkg,
		declMsg{"FlowState", o.ownedDecl(3, "flow", "", "Flow")},
		declMsg{"OrderPlaced", o.eventDecl(eventEntry{component: fq("FlowState"), domain: "orders"})},
	)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	if c := resp.File[0].GetContent(); !strings.Contains(c, `("Flow", "flow", System.Array.Empty<string>(), rebuilder)`) {
		t.Errorf("an untargeted PM needs a typed empty array:\n%s", c)
	}
}

func TestGenerate_ProjectorWithoutDomainsHasNoFilter(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	msgs := []declMsg{
		{"Ledger", o.componentDecl(4, "", "", "LedgerProjector")},
		{"OrderPlaced", o.eventDecl(eventEntry{component: fq("Ledger")})},
	}
	for _, lang := range codegen.Languages() {
		t.Run(lang, func(t *testing.T) {
			resp, err := generate(t, lang, ioPkg, msgs...)
			if err != nil {
				t.Fatalf("Generate: %v", err)
			}
			if c := strings.ToLower(resp.File[0].GetContent()); strings.Contains(c, "fordomains") || strings.Contains(c, "for_domains") {
				t.Errorf("%s projector with no declared domain must not filter:\n%s", lang, c)
			}
		})
	}
}

func TestLint_ProcessManagerTargetWithoutAggregateWarns(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	diags := lint(t,
		declMsg{"FlowState", o.ownedDecl(3, "flow", "billing", "Flow")},
		declMsg{"OrderPlaced", o.eventDecl(eventEntry{component: fq("FlowState"), domain: "orders"})},
	)
	for _, d := range diags {
		if d.Code == "ANZ101" && strings.Contains(d.Message, `"billing"`) {
			return
		}
	}
	t.Fatalf("want ANZ101 for the PM's unowned target billing, got %v", diags)
}

// pageContextSurface declares an aggregate applier, a process-manager own-
// state applier and a saga trigger: every handler the router now hands a
// PageContext.
func pageContextSurface(o optionTypes) []declMsg {
	return []declMsg{
		{"State", o.ownedDecl(1, "orders", "", "OrderAggregate")},
		{"PlaceOrder", o.commandDecl(fq("State"), fq("OrderPlaced"))},
		{"FlowState", o.ownedDecl(3, "flow", "inventory", "Flow")},
		{"FlowAdvanced", o.eventDecl(eventEntry{component: fq("FlowState"), applies: true})},
		{"OrderSaga", o.componentDecl(2, "orders", "inventory", "")},
		{"OrderPlaced", o.eventDecl(
			eventEntry{component: fq("State")},
			eventEntry{component: fq("FlowState"), domain: "orders"},
			eventEntry{component: fq("OrderSaga"), domain: "orders"},
		)},
	}
}

var pageContextWant = map[string]map[string][]string{
	"go": {
		"OrderAggregate": {
			`rebuilder.ApplyWithContext("validation.test.OrderPlaced", func(state *State, payload *anypb.Any, ctx _go.PageContext) error {`,
			"ApplyOrderPlaced(state *State, event *OrderPlaced, ctx _go.PageContext)",
		},
		"Flow": {
			`rebuilder.ApplyWithContext("validation.test.FlowAdvanced", func(state *FlowState, payload *anypb.Any, ctx _go.PageContext) error {`,
			"ApplyFlowAdvanced(state *FlowState, event *FlowAdvanced, ctx _go.PageContext)",
		},
		"OrderSaga": {
			`dispatch.OnEventWithContext("validation.test.OrderPlaced", func(eventAny *anypb.Any, dests *_go.Destinations, source _go.PageContext)`,
			"OrderPlaced(event *OrderPlaced, dests *_go.Destinations, source _go.PageContext) (",
		},
	},
	"python": {
		"OrderAggregate": {
			`rebuilder.apply_with_context("validation.test.OrderPlaced", _apply_apply_order_placed)`,
			"def apply_order_placed(self, state: _validation_test.State, event: _validation_test.OrderPlaced, ctx: _az.PageContext) -> None",
		},
		"Flow": {
			`rebuilder.apply_with_context("validation.test.FlowAdvanced", `,
		},
		"OrderSaga": {
			`dispatch.on_event_with_context("validation.test.OrderPlaced", _on_order_placed)`,
			"def order_placed(self, event: _validation_test.OrderPlaced, dests: _az.Destinations, source: _az.PageContext)",
		},
	},
	"java": {
		"OrderAggregate": {
			`rebuilder.applyWithContext("validation.test.OrderPlaced", (state, payload, ctx) ->`,
			"OrderPlaced event, io.angzarr.router.PageContext ctx);",
		},
		"Flow": {`rebuilder.applyWithContext("validation.test.FlowAdvanced", (state, payload, ctx) ->`},
		"OrderSaga": {
			`.onEventWithContext("validation.test.OrderPlaced", (eventAny, dests, source) -> {`,
			"io.angzarr.router.PageContext source)",
		},
	},
	"csharp": {
		"OrderAggregate": {
			`rebuilder.ApplyWithContext("validation.test.OrderPlaced", (state, payload, page) =>`,
			"void ApplyOrderPlaced(Validation.Test.State state, Validation.Test.OrderPlaced ev, Angzarr.Router.PageContext page)",
		},
		"Flow": {`rebuilder.ApplyWithContext("validation.test.FlowAdvanced", (state, payload, page) =>`},
		"OrderSaga": {
			`.OnEventWithContext("validation.test.OrderPlaced", (eventAny, dests, source) =>`,
			"Angzarr.Router.PageContext source)",
		},
	},
	"cpp": {
		"OrderAggregate": {
			`rebuilder.ApplyWithContext("validation.test.OrderPlaced", [&h](validation::test::State& state, const google::protobuf::Any& payload, const angzarr::router::PageContext& ctx) {`,
			"void ApplyOrderPlaced(validation::test::State& state, const validation::test::OrderPlaced& ev, const angzarr::router::PageContext& ctx)",
		},
		"Flow": {`rebuilder.ApplyWithContext("validation.test.FlowAdvanced", [&h](`},
		"OrderSaga": {
			`dispatch.OnEventWithContext("validation.test.OrderPlaced", [&h](const google::protobuf::Any& eventAny, const angzarr::router::Destinations& dests, const angzarr::router::PageContext& source) {`,
			"const angzarr::router::PageContext& source)",
		},
	},
	"typescript": {
		"OrderAggregate": {
			`rebuilder.applyWithContext("validation.test.OrderPlaced", (state, payload, ctx) => {`,
			"applyOrderPlaced(state: State, ev: OrderPlaced, ctx: PageContext): void",
			`new Rebuilder<State>(() => create(StateSchema), StateSchema);`,
		},
		"Flow": {
			`rebuilder.applyWithContext("validation.test.FlowAdvanced", (state, payload, ctx) => {`,
			`new Rebuilder<FlowState>(() => create(FlowStateSchema), FlowStateSchema);`,
		},
		"OrderSaga": {
			`dispatch.onEventWithContext("validation.test.OrderPlaced", (eventAny, dests, source) => {`,
			"source: PageContext",
		},
	},
}

func TestGenerate_PageContextSurface(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	for _, lang := range codegen.Languages() {
		t.Run(lang, func(t *testing.T) {
			resp, err := generate(t, lang, ioPkg, pageContextSurface(o)...)
			if err != nil {
				t.Fatalf("Generate: %v", err)
			}
			files := map[string]string{}
			for _, f := range resp.File {
				files[f.GetName()] = f.GetContent()
			}
			for base, wants := range pageContextWant[lang] {
				content := componentFileFor(t, lang, base, files)
				for _, w := range wants {
					if !strings.Contains(content, w) {
						t.Errorf("%s %s wiring missing %q:\n%s", lang, base, w, content)
					}
				}
			}
		})
	}
}
