package cpp

import (
	"testing"

	"github.com/angzarr-io/angzarr-cli/codegen/internal/codegentest"
)

func TestNestedTypeName(t *testing.T) {
	if got, want := cppNestedName(codegentest.NestedFixture(t)), "Outer::Mid::Inner"; got != want {
		t.Errorf("nested name = %q, want %q", got, want)
	}
}
