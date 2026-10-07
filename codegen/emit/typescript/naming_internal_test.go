package typescript

import (
	"testing"

	"github.com/angzarr-io/angzarr-cli/codegen/internal/codegentest"
)

func TestNestedTypeName(t *testing.T) {
	if got, want := tsNestedName(codegentest.NestedFixture(t)), "Outer_Mid_Inner"; got != want {
		t.Errorf("nested name = %q, want %q", got, want)
	}
}
