// Package builtin registers the CLI's built-in language emitters with the
// codegen registry. Importing it (for its side effect) is how a binary or a
// test opts in to the built-in languages; the codegen core itself never
// imports an emitter.
package builtin

import (
	"github.com/angzarr-io/angzarr-cli/codegen"
	"github.com/angzarr-io/angzarr-cli/codegen/emit/cpp"
	"github.com/angzarr-io/angzarr-cli/codegen/emit/csharp"
	"github.com/angzarr-io/angzarr-cli/codegen/emit/golang"
	"github.com/angzarr-io/angzarr-cli/codegen/emit/java"
	"github.com/angzarr-io/angzarr-cli/codegen/emit/python"
	"github.com/angzarr-io/angzarr-cli/codegen/emit/typescript"
)

func init() {
	codegen.Register(
		golang.Emitter{},
		python.Emitter{},
		java.Emitter{},
		csharp.Emitter{},
		cpp.Emitter{},
		typescript.Emitter{},
	)
}
