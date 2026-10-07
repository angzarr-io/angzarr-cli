package codegen

import (
	"fmt"
	"path"
	"sort"

	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/types/pluginpb"
)

// Emitter turns one validated component declaration into generated source for
// one target language. Output is one file PER COMPONENT (per handler interface),
// not per proto file: the emitter owns the whole file shape — header, imports,
// the single component's wiring.
type Emitter interface {
	// Lang is the subcommand / plugin-option name ("go", "python", …).
	Lang() string
	// WiringPath is the generated wiring file path for one component
	// (response-relative). The wiring file is regenerated wholesale every run.
	WiringPath(file *protogen.File, s *Component) string
	// EmitComponent writes the wiring file for ONE component.
	EmitComponent(g *protogen.GeneratedFile, file *protogen.File, s *Component) error
	// ScaffoldPath is the generate-once handler stub file path for one component.
	ScaffoldPath(file *protogen.File, s *Component) string
	// EmitScaffoldComponent writes the handler stub for ONE component —
	// generated once, then owned by the developer.
	EmitScaffoldComponent(g *protogen.GeneratedFile, file *protogen.File, s *Component) error
}

// ComponentFile builds a per-component output path: the proto file's directory
// (so generated wiring sits beside the messages, source_relative) joined with a
// component-derived stem + suffix.
func ComponentFile(file *protogen.File, stem, suffix string) string {
	return path.Join(path.Dir(file.GeneratedFilenamePrefix), stem+suffix)
}

// emitters is the language registry, filled by Register. The core never
// names a language: emitter packages are registered by the wiring layer
// (codegen/emit/builtin), so model, lint and orchestration stay
// language-neutral.
var emitters = map[string]Emitter{}

// Register adds emitters to the language registry, keyed by Lang. A second
// emitter for the same language panics: registration happens once, at
// program start.
func Register(es ...Emitter) {
	for _, e := range es {
		if _, dup := emitters[e.Lang()]; dup {
			panic(fmt.Sprintf("codegen: emitter for %q registered twice", e.Lang()))
		}
		emitters[e.Lang()] = e
	}
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

// Options carries language-specific codegen settings parsed from the plugin
// parameter. PyFrameworkPackage, when set, is the package a python consumer
// imports the angzarr framework protos from.
type Options struct {
	PyFrameworkPackage string
}

// Configurable is an Emitter whose output depends on Options.
type Configurable interface {
	Emitter
	// Configure returns the emitter configured for opts.
	Configure(opts Options) Emitter
}

// withOptions returns the emitter configured for opts; an emitter without
// options is returned unchanged.
func withOptions(emitter Emitter, opts Options) Emitter {
	if c, ok := emitter.(Configurable); ok {
		return c.Configure(opts)
	}
	return emitter
}

// Generate validates every component declaration in the request and emits
// wiring for the requested language. Validation (analyze) is language
// independent, so a misdeclaration fails generation identically everywhere.
// A component's commands and events may live in other files than its anchor,
// so the model is built over the whole request and then grouped by the file
// each anchor lives in.
func Generate(gen *protogen.Plugin, lang string, opts Options) error {
	emitter, ok := emitters[lang]
	if !ok {
		return fmt.Errorf("no emitter for language %q (have %v)", lang, Languages())
	}
	emitter = withOptions(emitter, opts)
	gen.SupportedFeatures = uint64(pluginpb.CodeGeneratorResponse_FEATURE_PROTO3_OPTIONAL)

	model, diags := analyze(gen)
	if HasErrors(diags) {
		return diagError(diags)
	}

	for _, fs := range model {
		for _, s := range fs.Components {
			g := gen.NewGeneratedFile(emitter.WiringPath(fs.File, s), fs.File.GoImportPath)
			if err := emitter.EmitComponent(g, fs.File, s); err != nil {
				return fmt.Errorf("%s/%s: %w", fs.File.Desc.Path(), s.BaseName, err)
			}
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
func GenerateScaffold(gen *protogen.Plugin, lang string, exists func(path string) bool, opts Options) error {
	emitter, ok := emitters[lang]
	if !ok {
		return fmt.Errorf("no emitter for language %q (have %v)", lang, Languages())
	}
	emitter = withOptions(emitter, opts)
	gen.SupportedFeatures = uint64(pluginpb.CodeGeneratorResponse_FEATURE_PROTO3_OPTIONAL)

	model, diags := analyze(gen)
	if HasErrors(diags) {
		return diagError(diags)
	}

	for _, fs := range model {
		for _, s := range fs.Components {
			stub := emitter.ScaffoldPath(fs.File, s)
			if exists != nil && exists(stub) {
				continue
			}
			g := gen.NewGeneratedFile(stub, fs.File.GoImportPath)
			if err := emitter.EmitScaffoldComponent(g, fs.File, s); err != nil {
				return fmt.Errorf("%s/%s: %w", fs.File.Desc.Path(), s.BaseName, err)
			}
		}
	}
	return nil
}
