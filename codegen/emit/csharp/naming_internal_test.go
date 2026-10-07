package csharp

import (
	"testing"

	"github.com/angzarr-io/angzarr-cli/codegen/internal/codegentest"
)

func TestNestedTypeName(t *testing.T) {
	if got, want := csNestedName(codegentest.NestedFixture(t)), "Outer.Types.Mid.Types.Inner"; got != want {
		t.Errorf("nested name = %q, want %q", got, want)
	}
}
