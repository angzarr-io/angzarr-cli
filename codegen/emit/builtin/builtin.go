// Package builtin registers the CLI's languages with the codegen registry:
// the built-in emitters, and the languages rendered only from a client
// repository's template set. Importing it (for its side effect) is how a
// binary or a test opts in to them; the codegen core itself never names a
// language.
package builtin

import (
	"github.com/angzarr-io/angzarr-cli/codegen"
	"github.com/angzarr-io/angzarr-cli/codegen/emit/cpp"
	"github.com/angzarr-io/angzarr-cli/codegen/emit/csharp"
	"github.com/angzarr-io/angzarr-cli/codegen/emit/golang"
	"github.com/angzarr-io/angzarr-cli/codegen/emit/java"
	"github.com/angzarr-io/angzarr-cli/codegen/emit/typescript"
)

func init() {
	codegen.Register(
		golang.Emitter{},
		java.Emitter{},
		csharp.Emitter{},
		cpp.Emitter{},
		typescript.Emitter{},
	)
	codegen.RegisterTemplateLanguages("python")
}
