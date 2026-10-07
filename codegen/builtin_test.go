package codegen_test

// The codegen tests drive generation through the language registry, so they
// register the built-in emitters the same way the CLI binary does.
import (
	"strings"
	"testing"

	"github.com/angzarr-io/angzarr-cli/codegen"
	_ "github.com/angzarr-io/angzarr-cli/codegen/emit/builtin"
)

func TestRegister_SecondEmitterForALanguagePanics(t *testing.T) {
	before := codegen.Languages()
	defer func() {
		if recover() == nil {
			t.Fatal("registering a second go emitter did not panic")
		}
		if after := codegen.Languages(); strings.Join(after, ",") != strings.Join(before, ",") {
			t.Errorf("registry changed by a refused registration: %v -> %v", before, after)
		}
	}()
	codegen.Register(dupEmitter{})
}

// dupEmitter claims the built-in go emitter's language.
type dupEmitter struct{ codegen.Emitter }

func (dupEmitter) Lang() string { return "go" }
