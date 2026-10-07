package java

import (
	"testing"

	"github.com/angzarr-io/angzarr-cli/codegen/internal/codegentest"
)

func TestNestedTypeName(t *testing.T) {
	if got, want := messageNestedName(codegentest.NestedFixture(t)), "Outer.Mid.Inner"; got != want {
		t.Errorf("nested name = %q, want %q", got, want)
	}
}
