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

// componentFile builds a per-component output path: the proto file's directory
// (so generated wiring sits beside the messages, source_relative) joined with a
// component-derived stem + suffix.
func componentFile(file *protogen.File, stem, suffix string) string {
	return path.Join(path.Dir(file.GeneratedFilenamePrefix), stem+suffix)
}

// emitters is the language registry. Adding a language = adding an
// Emitter implementation and registering it here.
var emitters = map[string]Emitter{
	goEmitter{}.Lang():     goEmitter{},
	pyEmitter{}.Lang():     pyEmitter{},
	javaEmitter{}.Lang():   javaEmitter{},
	csharpEmitter{}.Lang(): csharpEmitter{},
	cppEmitter{}.Lang():    cppEmitter{},
	tsEmitter{}.Lang():     tsEmitter{},
}

// Languages lists the registered target languages.
func Languages() []string {
	langs := make([]string, 0, len(emitters)+len(templateLanguages))
	for lang := range emitters {
		langs = append(langs, lang)
	}
	for lang := range templateLanguages {
		if _, dup := emitters[lang]; !dup {
			langs = append(langs, lang)
		}
	}
	sort.Strings(langs)
	return langs
}

// Options carries codegen settings parsed from the plugin parameter.
// PyFrameworkPackage, when set, is the package a python consumer imports the
// angzarr framework protos from (see pyEmitter.frameworkPkg). Templates, when
// set, is a template source (ParseTemplateSource): the language is rendered
// from that template set instead of a built-in emitter, with Params
// overriding the set's declared parameters.
type Options struct {
	PyFrameworkPackage string
	Templates          string
	Params             map[string]string
}

// templateLanguages are the languages generated only from a client repo's
// template set (templates= is required).
var templateLanguages = map[string]bool{}

// renderFromTemplates resolves opts.Templates and renders its outputs of one
// mode, refusing a set written for another language.
func renderFromTemplates(gen *protogen.Plugin, lang, mode string, opts Options, skip func(string) bool) error {
	src, err := ParseTemplateSource(opts.Templates)
	if err != nil {
		return err
	}
	cache, err := TemplateCacheDir()
	if err != nil && src.Local == "" {
		return err
	}
	dir, err := src.Resolve(cache)
	if err != nil {
		return err
	}
	ts, err := LoadTemplateSet(dir)
	if err != nil {
		return err
	}
	if ts.Manifest.Language != lang {
		return fmt.Errorf("templates=%s is a %s template set, not %s", opts.Templates, ts.Manifest.Language, lang)
	}
	gen.SupportedFeatures = uint64(pluginpb.CodeGeneratorResponse_FEATURE_PROTO3_OPTIONAL)
	return RenderTemplates(gen, ts, mode, opts.Params, skip)
}

// lookupEmitter finds the built-in emitter for lang, explaining when the
// language is template-only.
func lookupEmitter(lang string) (Emitter, error) {
	if emitter, ok := emitters[lang]; ok {
		return emitter, nil
	}
	if templateLanguages[lang] {
		return nil, fmt.Errorf("%s is generated from the client repository's templates: pass the plugin option "+
			"templates=github.com/angzarr-io/angzarr-client-%s@<commit|tag> (or templates=<local path>)", lang, lang)
	}
	return nil, fmt.Errorf("no emitter for language %q (have %v)", lang, Languages())
}

// GenerateModel writes the language-neutral component model of the request
// as one JSON file at name.
func GenerateModel(gen *protogen.Plugin, name string) error {
	gen.SupportedFeatures = uint64(pluginpb.CodeGeneratorResponse_FEATURE_PROTO3_OPTIONAL)
	model, diags := AnalyzeModel(gen)
	if HasErrors(diags) {
		return diagError(diags)
	}
	raw, err := ModelJSON(model)
	if err != nil {
		return err
	}
	_, err = gen.NewGeneratedFile(name, "").Write(raw)
	return err
}

// withOptions returns the emitter configured for opts. Only the python emitter
// has options today; others are returned unchanged.
func withOptions(emitter Emitter, opts Options) Emitter {
	if pe, ok := emitter.(pyEmitter); ok {
		pe.frameworkPkg = opts.PyFrameworkPackage
		return pe
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
	if opts.Templates != "" {
		return renderFromTemplates(gen, lang, ModeCodegen, opts, nil)
	}
	emitter, err := lookupEmitter(lang)
	if err != nil {
		return err
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
	if opts.Templates != "" {
		return renderFromTemplates(gen, lang, ModeScaffold, opts, exists)
	}
	emitter, err := lookupEmitter(lang)
	if err != nil {
		return err
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
