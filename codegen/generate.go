package codegen

import (
	"fmt"
	"sort"

	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/types/pluginpb"
)

// Emitter turns one proto file's validated component declarations into
// generated source for one target language. The emitter owns the whole
// file shape — header, imports, per-component wiring.
type Emitter interface {
	// Lang is the subcommand / plugin-option name ("go", "python", …).
	Lang() string
	// Suffix names the wiring file: <proto path prefix> + Suffix. The wiring
	// file is regenerated wholesale every run.
	Suffix() string
	// EmitFile writes the wiring file for one proto file's components.
	EmitFile(g *protogen.GeneratedFile, file *protogen.File, services []*Service) error
	// ScaffoldSuffix names the generate-once handler stub file:
	// <proto path prefix> + ScaffoldSuffix.
	ScaffoldSuffix() string
	// EmitScaffold writes the handler stub file for one proto file's
	// components — generated once, then owned by the developer.
	EmitScaffold(g *protogen.GeneratedFile, file *protogen.File, services []*Service) error
}

// emitters is the language registry. Adding a language = adding an
// Emitter implementation and registering it here.
var emitters = map[string]Emitter{
	goEmitter{}.Lang():     goEmitter{},
	pyEmitter{}.Lang():     pyEmitter{},
	javaEmitter{}.Lang():   javaEmitter{},
	csharpEmitter{}.Lang(): csharpEmitter{},
	cppEmitter{}.Lang():    cppEmitter{},
}

// Languages lists the registered target languages.
func Languages() []string {
	langs := make([]string, 0, len(emitters))
	for lang := range emitters {
		langs = append(langs, lang)
	}
	sort.Strings(langs)
	return langs
}

// Generate walks the request's messages, validates every component
// declaration, and emits wiring for the requested language. All declaration
// validation happens in buildModel regardless of language — the same
// misdeclaration fails generation identically everywhere. Components are
// declared by message annotations, and a component's commands/events may live
// in other files than its anchor, so the model is built globally and then
// grouped by the file each anchor lives in.
func Generate(gen *protogen.Plugin, lang string) error {
	emitter, ok := emitters[lang]
	if !ok {
		return fmt.Errorf("no emitter for language %q (have %v)", lang, Languages())
	}
	gen.SupportedFeatures = uint64(pluginpb.CodeGeneratorResponse_FEATURE_PROTO3_OPTIONAL)

	model, diags := analyze(gen)
	if HasErrors(diags) {
		return diagError(diags)
	}

	for _, fs := range model {
		g := gen.NewGeneratedFile(fs.File.GeneratedFilenamePrefix+emitter.Suffix(), fs.File.GoImportPath)
		if err := emitter.EmitFile(g, fs.File, fs.Services); err != nil {
			return fmt.Errorf("%s: %w", fs.File.Desc.Path(), err)
		}
	}
	return nil
}

// GenerateScaffold emits the generate-once handler stub for every component,
// skipping any whose stub file already exists per the exists predicate. A
// skipped file is simply absent from the response, so the consumer (buf)
// writes nothing for it and the developer-owned stub is preserved untouched.
// exists receives the response-relative file path; a nil predicate emits every
// stub (overwriting), which callers should avoid in normal use.
func GenerateScaffold(gen *protogen.Plugin, lang string, exists func(path string) bool) error {
	emitter, ok := emitters[lang]
	if !ok {
		return fmt.Errorf("no emitter for language %q (have %v)", lang, Languages())
	}
	gen.SupportedFeatures = uint64(pluginpb.CodeGeneratorResponse_FEATURE_PROTO3_OPTIONAL)

	model, diags := analyze(gen)
	if HasErrors(diags) {
		return diagError(diags)
	}

	for _, fs := range model {
		path := fs.File.GeneratedFilenamePrefix + emitter.ScaffoldSuffix()
		if exists != nil && exists(path) {
			continue
		}
		g := gen.NewGeneratedFile(path, fs.File.GoImportPath)
		if err := emitter.EmitScaffold(g, fs.File, fs.Services); err != nil {
			return fmt.Errorf("%s: %w", fs.File.Desc.Path(), err)
		}
	}
	return nil
}
