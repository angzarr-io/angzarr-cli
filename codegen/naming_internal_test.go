package codegen

import (
	"testing"

	"github.com/angzarr-io/angzarr-cli/codegen/internal/codegentest"
)

func TestSnakeToPascal_MatchesProtocOuterClassRule(t *testing.T) {
	// protoc's UnderscoresToCamelCase: capitalise the first letter and any
	// letter following an underscore or a digit; drop non-alphanumerics.
	for in, want := range map[string]string{
		"orders":      "Orders",
		"order_saga":  "OrderSaga",
		"foo2bar":     "Foo2Bar",
		"v1_2beta":    "V12Beta",
		"FOO_bar":     "FOOBar",
		"a-b.c":       "ABC",
		"__x":         "X",
		"table_hand9": "TableHand9",
	} {
		if got := SnakeToPascal(in); got != want {
			t.Errorf("snakeToPascal(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestQuoteLiteral_EscapesForCFamilyAndScriptLanguages(t *testing.T) {
	for in, want := range map[string]string{
		`orders`:  `"orders"`,
		`or"ders`: `"or\"ders"`,
		`a\b`:     `"a\\b"`,
		"l1\nl2":  `"l1\nl2"`,
		"t\tr\r":  `"t\tr\r"`,
	} {
		if got := QuoteLiteral(in); got != want {
			t.Errorf("QuoteLiteral(%q) = %s, want %s", in, got, want)
		}
	}
}

func TestNestedNames_OutermostFirst(t *testing.T) {
	inner := codegentest.NestedFixture(t)
	got := NestedNames(inner)
	want := []string{"Outer", "Mid", "Inner"}
	if len(got) != len(want) {
		t.Fatalf("NestedNames = %v, want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("NestedNames = %v, want %v", got, want)
		}
	}
}
