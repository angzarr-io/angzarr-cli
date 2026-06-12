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
	// Suffix names the generated file: <proto path prefix> + Suffix.
	Suffix() string
	// EmitFile writes the generated file for one proto file's components.
	EmitFile(g *protogen.GeneratedFile, file *protogen.File, services []*Service) error
}

// emitters is the language registry. Adding a language = adding an
// Emitter implementation and registering it here.
var emitters = map[string]Emitter{
	goEmitter{}.Lang(): goEmitter{},
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

// Generate walks the request's files, validates every component
// declaration, and emits wiring for the requested language. All
// declaration validation happens here regardless of language — the same
// misdeclaration fails generation identically everywhere.
func Generate(gen *protogen.Plugin, lang string) error {
	emitter, ok := emitters[lang]
	if !ok {
		return fmt.Errorf("no emitter for language %q (have %v)", lang, Languages())
	}
	gen.SupportedFeatures = uint64(pluginpb.CodeGeneratorResponse_FEATURE_PROTO3_OPTIONAL)

	exts := resolveExtensions(gen)
	registry := messageRegistry(gen)

	for _, file := range gen.Files {
		if !file.Generate {
			continue
		}
		var services []*Service
		for _, svc := range file.Services {
			component := componentOptions(svc, exts)
			if component == nil {
				continue
			}
			s, err := buildService(svc, component, registry, exts)
			if err != nil {
				return fmt.Errorf("%s: %w", svc.Desc.FullName(), err)
			}
			services = append(services, s)
		}
		if len(services) == 0 {
			continue
		}
		g := gen.NewGeneratedFile(file.GeneratedFilenamePrefix+emitter.Suffix(), file.GoImportPath)
		if err := emitter.EmitFile(g, file, services); err != nil {
			return fmt.Errorf("%s: %w", file.Desc.Path(), err)
		}
	}
	return nil
}
