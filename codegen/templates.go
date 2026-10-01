package codegen

// Template rendering: a client repo's template set turns the language-neutral
// component model into that language's wiring and scaffold files. The
// contract (manifest fields, render data, helper functions) is documented in
// docs/templates.md; the model the templates read is modeljson.go.

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"text/template"

	"go.yaml.in/yaml/v3"
	"google.golang.org/protobuf/compiler/protogen"
)

// ManifestFile is the file naming a template set's outputs.
const ManifestFile = "manifest.yaml"

// Output modes a manifest entry renders in.
const (
	ModeCodegen  = "codegen"
	ModeScaffold = "scaffold"
)

// Manifest is a template set's manifest.yaml.
type Manifest struct {
	// SchemaVersion is the model schema version the templates read; it must
	// equal ModelSchemaVersion.
	SchemaVersion int `yaml:"schema_version"`
	// Language is the codegen/scaffold subcommand the set renders for.
	Language string `yaml:"language"`
	// Params are the template parameters and their defaults; a plugin option
	// param.<name>=<value> overrides one. Undeclared names are refused.
	Params map[string]string `yaml:"params"`
	// Imports configures the per-component import aliases.
	Imports ImportRules `yaml:"imports"`
	// Types holds the type mapping hooks: template strings mapping a model
	// reference to a language expression.
	Types TypeHooks `yaml:"types"`
	// Outputs lists the files rendered per component.
	Outputs []Output `yaml:"outputs"`
}

// ImportRules derives one alias per referenced proto file: AliasPrefix + the
// file stem, suffixed with the file's index in the sorted import list when
// that alias is reserved or already taken.
type ImportRules struct {
	AliasPrefix     string   `yaml:"alias_prefix"`
	ReservedAliases []string `yaml:"reserved_aliases"`
}

// TypeHooks are template strings. Message renders a MessageRef as a language
// type expression with data {message, import, file}; the typeRef helper
// invokes it.
type TypeHooks struct {
	Message string `yaml:"message"`
}

// Output is one rendered file per matching component.
type Output struct {
	// Mode is codegen (regenerated every run) or scaffold (emitted once, never
	// overwritten).
	Mode string `yaml:"mode"`
	// Kinds restricts the output to these component kinds; empty means all.
	Kinds []string `yaml:"kinds"`
	// Path is a template string rendering the output path, relative to the
	// plugin's out directory.
	Path string `yaml:"path"`
	// Template is the template file (a *.tmpl in the set's directory) whose
	// output is the file content.
	Template string `yaml:"template"`
}

// TemplateSet is a parsed manifest plus its templates.
type TemplateSet struct {
	Dir      string
	Manifest Manifest
	tmpl     *template.Template
}

const (
	typeMessageTemplate = "angzarr:types.message"
	outputPathTemplate  = "angzarr:outputs.%d.path"
)

var componentKinds = map[string]bool{
	KindAggregate.String():      true,
	KindSaga.String():           true,
	KindProcessManager.String(): true,
	KindProjector.String():      true,
}

// LoadTemplateSet reads dir/manifest.yaml and parses every *.tmpl beside it
// into one template set, so templates can share {{define}}d blocks.
func LoadTemplateSet(dir string) (*TemplateSet, error) {
	raw, err := os.ReadFile(filepath.Join(dir, ManifestFile))
	if err != nil {
		return nil, fmt.Errorf("template set: %w", err)
	}
	var m Manifest
	dec := yaml.NewDecoder(bytes.NewReader(raw))
	dec.KnownFields(true)
	if err := dec.Decode(&m); err != nil {
		return nil, fmt.Errorf("template set %s: %w", ManifestFile, err)
	}
	if m.SchemaVersion != ModelSchemaVersion {
		return nil, fmt.Errorf("template set %s targets model schema_version %d; this CLI emits %d",
			ManifestFile, m.SchemaVersion, ModelSchemaVersion)
	}
	if m.Language == "" {
		return nil, fmt.Errorf("template set %s: language is required", ManifestFile)
	}
	if len(m.Outputs) == 0 {
		return nil, fmt.Errorf("template set %s: no outputs", ManifestFile)
	}
	files, err := filepath.Glob(filepath.Join(dir, "*.tmpl"))
	if err != nil {
		return nil, err
	}
	if len(files) == 0 {
		return nil, fmt.Errorf("template set %s: no *.tmpl files", dir)
	}
	sort.Strings(files)
	t := template.New(ManifestFile).Option("missingkey=error").Funcs(baseFuncs())
	t, err = t.ParseFiles(files...)
	if err != nil {
		return nil, fmt.Errorf("template set: %w", err)
	}
	if m.Types.Message != "" {
		if _, err := t.New(typeMessageTemplate).Parse(m.Types.Message); err != nil {
			return nil, fmt.Errorf("template set %s types.message: %w", ManifestFile, err)
		}
	}
	for i, o := range m.Outputs {
		if o.Mode != ModeCodegen && o.Mode != ModeScaffold {
			return nil, fmt.Errorf("template set %s outputs[%d]: mode %q is not %s or %s", ManifestFile, i, o.Mode, ModeCodegen, ModeScaffold)
		}
		for _, k := range o.Kinds {
			if !componentKinds[k] {
				return nil, fmt.Errorf("template set %s outputs[%d]: unknown kind %q", ManifestFile, i, k)
			}
		}
		if t.Lookup(o.Template) == nil {
			return nil, fmt.Errorf("template set %s outputs[%d]: template %q not found in %s", ManifestFile, i, o.Template, dir)
		}
		if o.Path == "" {
			return nil, fmt.Errorf("template set %s outputs[%d]: path is required", ManifestFile, i)
		}
		if _, err := t.New(fmt.Sprintf(outputPathTemplate, i)).Parse(o.Path); err != nil {
			return nil, fmt.Errorf("template set %s outputs[%d].path: %w", ManifestFile, i, err)
		}
	}
	return &TemplateSet{Dir: dir, Manifest: m, tmpl: t}, nil
}

// resolveParams merges plugin overrides onto the manifest defaults, refusing
// names the manifest does not declare.
func (ts *TemplateSet) resolveParams(overrides map[string]string) (map[string]any, error) {
	out := make(map[string]any, len(ts.Manifest.Params))
	for k, v := range ts.Manifest.Params {
		out[k] = v
	}
	names := make([]string, 0, len(overrides))
	for k := range overrides {
		names = append(names, k)
	}
	sort.Strings(names)
	for _, k := range names {
		if _, ok := ts.Manifest.Params[k]; !ok {
			return nil, fmt.Errorf("template parameter %q is not declared by the %s template set (declared: %s)",
				k, ts.Manifest.Language, strings.Join(sortedKeys(ts.Manifest.Params), ", "))
		}
		out[k] = overrides[k]
	}
	return out, nil
}

func sortedKeys(m map[string]string) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}

// RenderedFile is one rendered output.
type RenderedFile struct {
	Path    string
	Content []byte
}

// Render renders every output of the given mode for every component in the
// model. skip, when non-nil, drops an output whose rendered path it reports
// true for (scaffold's never-overwrite rule) before its content is rendered.
func (ts *TemplateSet) Render(model *Model, mode string, params map[string]string, skip func(string) bool) ([]RenderedFile, error) {
	resolved, err := ts.resolveParams(params)
	if err != nil {
		return nil, err
	}
	generic, err := toGeneric(model)
	if err != nil {
		return nil, err
	}
	protoFiles, _ := generic["proto_files"].(map[string]any)
	files, _ := generic["files"].([]any)
	var out []RenderedFile
	seen := map[string]string{}
	for fi, f := range files {
		file := f.(map[string]any)
		comps, _ := file["components"].([]any)
		for ci, c := range comps {
			comp := c.(map[string]any)
			mc := model.Files[fi].Components[ci]
			imports := ts.imports(mc, protoFiles)
			data := map[string]any{
				"schema_version": model.SchemaVersion,
				"language":       ts.Manifest.Language,
				"mode":           mode,
				"params":         resolved,
				"file":           file,
				"component":      comp,
				"imports":        imports,
				"proto_files":    protoFiles,
			}
			t, err := ts.bind(imports, protoFiles)
			if err != nil {
				return nil, err
			}
			for i, o := range ts.Manifest.Outputs {
				if o.Mode != mode || !kindMatches(o.Kinds, mc.Kind) {
					continue
				}
				where := fmt.Sprintf("%s/%s outputs[%d]", model.Files[fi].Path, mc.Name, i)
				p, err := execString(t, fmt.Sprintf(outputPathTemplate, i), data)
				if err != nil {
					return nil, fmt.Errorf("%s path: %w", where, err)
				}
				p = path.Clean(strings.TrimSpace(p))
				if p == "." || strings.HasPrefix(p, "../") || path.IsAbs(p) {
					return nil, fmt.Errorf("%s: output path %q escapes the output directory", where, p)
				}
				if prev, dup := seen[p]; dup {
					return nil, fmt.Errorf("%s: output path %q is also rendered by %s", where, p, prev)
				}
				seen[p] = where
				if skip != nil && skip(p) {
					continue
				}
				content, err := execString(t, o.Template, data)
				if err != nil {
					return nil, fmt.Errorf("%s: %w", where, err)
				}
				out = append(out, RenderedFile{Path: p, Content: []byte(content)})
			}
		}
	}
	return out, nil
}

func kindMatches(kinds []string, kind string) bool {
	if len(kinds) == 0 {
		return true
	}
	for _, k := range kinds {
		if k == kind {
			return true
		}
	}
	return false
}

// imports builds a component's import list: one entry per referenced proto
// file (sorted by path) carrying that file's ProtoFile fields plus its alias.
func (ts *TemplateSet) imports(c ModelComponent, protoFiles map[string]any) []any {
	used := map[string]bool{}
	for _, r := range ts.Manifest.Imports.ReservedAliases {
		used[r] = true
	}
	out := make([]any, 0, len(c.ReferencedFiles))
	for i, p := range c.ReferencedFiles {
		pf, _ := protoFiles[p].(map[string]any)
		entry := make(map[string]any, len(pf)+1)
		for k, v := range pf {
			entry[k] = v
		}
		alias := ts.Manifest.Imports.AliasPrefix + strings.TrimSuffix(path.Base(p), ".proto")
		if used[alias] {
			alias += strconv.Itoa(i)
		}
		used[alias] = true
		entry["alias"] = alias
		out = append(out, entry)
	}
	return out
}

// bind clones the set with the render-scoped helpers: typeRef resolves a
// message through the component's import list.
func (ts *TemplateSet) bind(imports []any, protoFiles map[string]any) (*template.Template, error) {
	t, err := ts.tmpl.Clone()
	if err != nil {
		return nil, err
	}
	byPath := map[string]any{}
	for _, imp := range imports {
		m := imp.(map[string]any)
		byPath[m["path"].(string)] = m
	}
	t.Funcs(template.FuncMap{
		"include": func(name string, data any) (string, error) {
			return execString(t, name, data)
		},
		"typeRef": func(msg any) (string, error) {
			m, ok := msg.(map[string]any)
			if !ok {
				return "", fmt.Errorf("typeRef: want a message reference, got %T", msg)
			}
			if t.Lookup(typeMessageTemplate) == nil {
				return "", fmt.Errorf("typeRef: the manifest declares no types.message hook")
			}
			file, _ := m["file"].(string)
			imp, ok := byPath[file]
			if !ok {
				return "", fmt.Errorf("typeRef: %v lives in %q, which is not in the component's imports", m["full_name"], file)
			}
			s, err := execString(t, typeMessageTemplate, map[string]any{"message": m, "import": imp, "file": protoFiles[file]})
			return strings.TrimSpace(s), err
		},
	})
	return t, nil
}

func execString(t *template.Template, name string, data any) (string, error) {
	var b bytes.Buffer
	if err := t.ExecuteTemplate(&b, name, data); err != nil {
		return "", err
	}
	return b.String(), nil
}

// toGeneric round-trips the model through JSON so templates address it by
// its JSON keys — the documented schema — rather than Go field names.
func toGeneric(m *Model) (map[string]any, error) {
	raw, err := json.Marshal(m)
	if err != nil {
		return nil, err
	}
	var out map[string]any
	dec := json.NewDecoder(bytes.NewReader(raw))
	dec.UseNumber()
	if err := dec.Decode(&out); err != nil {
		return nil, err
	}
	return out, nil
}

// RenderTemplates renders the template set's outputs of one mode into the
// plugin response. Codegen and scaffold share it; skip is scaffold's
// existing-stub predicate (nil for codegen).
func RenderTemplates(gen *protogen.Plugin, ts *TemplateSet, mode string, params map[string]string, skip func(string) bool) error {
	model, diags := AnalyzeModel(gen)
	if HasErrors(diags) {
		return diagError(diags)
	}
	files, err := ts.Render(model, mode, params, skip)
	if err != nil {
		return err
	}
	for _, f := range files {
		g := gen.NewGeneratedFile(f.Path, "")
		if _, err := g.Write(f.Content); err != nil {
			return err
		}
	}
	return nil
}
