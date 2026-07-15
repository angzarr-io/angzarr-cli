package codegen

// White-box test for the projectorDomains computation itself (package
// codegen, not codegen_test): a pure function over Component.Handlers, so it is
// unit-testable directly without building a descriptor set. Complements the
// black-box per-language TestGenerate*_Projector_MultiDomain tests in
// generate_test.go, which prove the computed value is actually consumed
// identically by every emitter.

import (
	"reflect"
	"testing"
)

// TestProjectorDomains_UnionDedupedSorted proves the model computes the
// authoritative projector domain filter (decision A, L01 remediation) as the
// sorted, deduplicated UNION of handler source domains — not the single
// declared input_domain, and not handler declaration order. This is the
// computation that was previously duplicated (and allowed to diverge) across
// five of six emitters, which restricted to the single input_domain instead
// and silently dropped every secondary-domain event at runtime.
func TestProjectorDomains_UnionDedupedSorted(t *testing.T) {
	tests := []struct {
		name     string
		handlers []Handler
		want     []string
	}{
		{"no handlers yields empty filter (consume-all default)", nil, nil},
		{"single domain", []Handler{{SourceDomain: "table"}}, []string{"table"}},
		{"dedupes repeated domains", []Handler{
			{SourceDomain: "table"}, {SourceDomain: "table"},
		}, []string{"table"}},
		{"unions and sorts out-of-declaration-order domains", []Handler{
			{SourceDomain: "table"}, {SourceDomain: "hand"}, {SourceDomain: "player"},
		}, []string{"hand", "player", "table"}},
		{"handlers with no declared domain don't widen the filter", []Handler{
			{SourceDomain: "table"}, {SourceDomain: ""},
		}, []string{"table"}},
		{"all domain-less handlers yield an empty filter (consume-all default)", []Handler{
			{SourceDomain: ""}, {SourceDomain: ""},
		}, nil},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			s := &Component{Component: &ComponentDecl{Kind: KindProjector}, Handlers: tt.handlers}
			got := projectorDomains(s)
			if !reflect.DeepEqual(got, tt.want) {
				t.Errorf("projectorDomains() = %#v, want %#v", got, tt.want)
			}
		})
	}
}
