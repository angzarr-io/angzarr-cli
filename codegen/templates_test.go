package codegen_test

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/angzarr-io/angzarr-cli/codegen"
)

const tmplSet = "testdata/tmplset"

// renderOpts renders the fixture template set through the public plugin path.
func renderOpts(params map[string]string) codegen.Options {
	return codegen.Options{Templates: tmplSet, Params: params}
}

func TestModel_CarriesTheValidatedComponent(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	gen, err := buildGen(t, ioPkg, orderAggregate(o)...)
	if err != nil {
		t.Fatal(err)
	}
	model, diags := codegen.AnalyzeModel(gen)
	if model == nil {
		t.Fatalf("AnalyzeModel: %v", diags)
	}
	if model.SchemaVersion != codegen.ModelSchemaVersion || codegen.ModelSchemaVersion != 1 {
		t.Errorf("schema_version = %d, want 1", model.SchemaVersion)
	}
	if len(model.Files) != 1 || len(model.Files[0].Components) != 1 {
		t.Fatalf("want one file with one component, got %+v", model.Files)
	}
	f := model.Files[0]
	if f.Path != testPath || f.Dir != "." || f.Stem != "validation_test" || f.Package != testPkg {
		t.Errorf("file = %+v", f.ProtoFile)
	}
	if got := f.Options["go_package"]; got != "example.test/validation;validationtest" {
		t.Errorf("go_package option = %q", got)
	}
	c := f.Components[0]
	if c.Kind != "AGGREGATE" || c.Name != "OrderAggregate" || c.StubName != "OrderAggregate" || c.Domain != "orders" {
		t.Errorf("component header = %+v", c)
	}
	if c.State == nil || c.State.FullName != fq("State") || c.Anchor.FullName != fq("State") {
		t.Errorf("state/anchor = %+v / %+v", c.State, c.Anchor)
	}
	if len(c.Handlers) != 1 {
		t.Fatalf("handlers = %+v", c.Handlers)
	}
	h := c.Handlers[0]
	if h.Method != "CreateOrder" || h.Message.FullName != fq("CreateOrder") || !h.TypedEmit ||
		len(h.Emits) != 1 || h.Emits[0].FullName != fq("OrderCreated") {
		t.Errorf("handler = %+v", h)
	}
	if len(c.Appliers) != 1 || c.Appliers[0].Method != "ApplyOrderCreated" {
		t.Errorf("appliers = %+v", c.Appliers)
	}
	if len(c.ReferencedFiles) != 1 || c.ReferencedFiles[0] != testPath {
		t.Errorf("referenced_files = %v", c.ReferencedFiles)
	}
	if _, ok := model.ProtoFiles[testPath]; !ok || len(model.ProtoFiles) != 1 {
		t.Errorf("proto_files = %v", model.ProtoFiles)
	}
	if c.FinishMethod != "" {
		t.Errorf("finish_method on an aggregate = %q", c.FinishMethod)
	}
}

func TestModelJSON_UsesTheDocumentedKeys(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	gen, err := buildGen(t, ioPkg, projectorMultiDomain(o)...)
	if err != nil {
		t.Fatal(err)
	}
	model, diags := codegen.AnalyzeModel(gen)
	if model == nil {
		t.Fatalf("AnalyzeModel: %v", diags)
	}
	raw, err := codegen.ModelJSON(model)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasSuffix(string(raw), "}\n") {
		t.Errorf("model JSON must end with a newline")
	}
	var doc map[string]any
	if err := json.Unmarshal(raw, &doc); err != nil {
		t.Fatal(err)
	}
	if doc["schema_version"] != float64(1) {
		t.Errorf("schema_version = %v", doc["schema_version"])
	}
	comp := doc["files"].([]any)[0].(map[string]any)["components"].([]any)[0].(map[string]any)
	for _, key := range []string{"kind", "name", "stub_name", "anchor", "state", "domain", "input_domain",
		"output_domains", "projector_domains", "emits_facts", "handlers", "appliers", "rejections",
		"undos", "facts", "finish_method", "referenced_files"} {
		if _, ok := comp[key]; !ok {
			t.Errorf("component JSON lacks %q", key)
		}
	}
	if comp["finish_method"] != "Finish" {
		t.Errorf("projector finish_method = %v", comp["finish_method"])
	}
	domains, _ := comp["projector_domains"].([]any)
	if len(domains) != 2 || domains[0] != "hand" || domains[1] != "table" {
		t.Errorf("projector_domains = %v, want sorted [hand table]", comp["projector_domains"])
	}
	h := comp["handlers"].([]any)[0].(map[string]any)
	if h["source_domain"] != "table" || h["typed_emit"] != false {
		t.Errorf("handler JSON = %v", h)
	}
	msg := h["message"].(map[string]any)
	if msg["full_name"] != fq("TableCreated") || msg["file"] != testPath || msg["package"] != testPkg {
		t.Errorf("message ref JSON = %v", msg)
	}
}

func TestGenerateModel_WritesOneJSONFile(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	gen, err := buildGen(t, ioPkg, orderAggregate(o)...)
	if err != nil {
		t.Fatal(err)
	}
	if err := codegen.GenerateModel(gen, codegen.ModelFileName); err != nil {
		t.Fatal(err)
	}
	resp := gen.Response()
	if len(resp.File) != 1 || resp.File[0].GetName() != "angzarr.model.json" {
		t.Fatalf("want angzarr.model.json, got %v", resp.File)
	}
	if !strings.Contains(resp.File[0].GetContent(), `"name": "OrderAggregate"`) {
		t.Errorf("model file lacks the component:\n%s", resp.File[0].GetContent())
	}
}

func TestGenerateModel_RefusesInvalidDeclarations(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	gen, err := buildGen(t, ioPkg, declMsg{"State", o.ownedDecl(1, "", "", "OrderAggregate")})
	if err != nil {
		t.Fatal(err)
	}
	if err := codegen.GenerateModel(gen, codegen.ModelFileName); err == nil || !strings.Contains(err.Error(), "ANZ008") {
		t.Fatalf("want ANZ008 (aggregate without domain), got %v", err)
	}
}

func TestTemplates_RenderEveryOutputOfTheMode(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	gen, err := buildGen(t, ioPkg, orderAggregate(o)...)
	if err != nil {
		t.Fatal(err)
	}
	if err := codegen.Generate(gen, "testlang", renderOpts(nil)); err != nil {
		t.Fatalf("Generate: %v", err)
	}
	files := map[string]string{}
	for _, f := range gen.Response().File {
		files[f.GetName()] = f.GetContent()
	}
	if len(files) != 2 {
		t.Fatalf("want the two codegen outputs, got %v", files)
	}
	want := "hello OrderAggregate aggregate schema=1 mode=codegen lang=testlang\n" +
		"import validation_test.proto as m_validation_test pkg=validation.test\n" +
		"state m_validation_test::State\n" +
		"handler create_order(m_validation_test::CreateOrder) typed=true -> m_validation_test::OrderCreated\n" +
		"applier apply_order_created(m_validation_test::OrderCreated)\n" +
		"shared:OrderAggregate\n"
	if got := files["order_aggregate.wire"]; got != want {
		t.Errorf("order_aggregate.wire =\n%q\nwant\n%q", got, want)
	}
	if got := files["order_aggregate.aggonly"]; got != "aggregate OrderAggregate domain=\"orders\"\n" {
		t.Errorf("order_aggregate.aggonly = %q", got)
	}
}

func TestTemplates_KindsFilterSkipsOtherKinds(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	gen, err := buildGen(t, ioPkg, projectorMultiDomain(o)...)
	if err != nil {
		t.Fatal(err)
	}
	if err := codegen.Generate(gen, "testlang", renderOpts(nil)); err != nil {
		t.Fatalf("Generate: %v", err)
	}
	resp := gen.Response()
	if len(resp.File) != 1 || resp.File[0].GetName() != "multi_domain_projector.wire" {
		t.Fatalf("a projector renders only the unfiltered output, got %v", resp.File)
	}
	if !strings.Contains(resp.File[0].GetContent(), "typed=false\n") {
		t.Errorf("projector handlers are not typed-emit:\n%s", resp.File[0].GetContent())
	}
}

func TestTemplates_ParamOverrideAndUndeclaredParam(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	gen, err := buildGen(t, ioPkg, orderAggregate(o)...)
	if err != nil {
		t.Fatal(err)
	}
	if err := codegen.Generate(gen, "testlang", renderOpts(map[string]string{"greeting": "hi"})); err != nil {
		t.Fatal(err)
	}
	if c := gen.Response().File[0].GetContent(); !strings.HasPrefix(c, "hi OrderAggregate") {
		t.Errorf("param.greeting override not applied: %q", c)
	}

	gen, _ = buildGen(t, ioPkg, orderAggregate(o)...)
	err = codegen.Generate(gen, "testlang", renderOpts(map[string]string{"nope": "x"}))
	if err == nil || !strings.Contains(err.Error(), `"nope" is not declared`) || !strings.Contains(err.Error(), "greeting") {
		t.Fatalf("undeclared parameter must be refused naming the declared ones, got %v", err)
	}
}

func TestTemplates_ScaffoldSkipsExistingFiles(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	gen, err := buildGen(t, ioPkg, orderAggregate(o)...)
	if err != nil {
		t.Fatal(err)
	}
	if err := codegen.GenerateScaffold(gen, "testlang", func(string) bool { return false }, renderOpts(nil)); err != nil {
		t.Fatal(err)
	}
	resp := gen.Response()
	if len(resp.File) != 1 || resp.File[0].GetName() != "order_aggregate.stub" || resp.File[0].GetContent() != "stub OrderAggregate\n" {
		t.Fatalf("scaffold = %v", resp.File)
	}

	gen, _ = buildGen(t, ioPkg, orderAggregate(o)...)
	var asked []string
	exists := func(p string) bool { asked = append(asked, p); return true }
	if err := codegen.GenerateScaffold(gen, "testlang", exists, renderOpts(nil)); err != nil {
		t.Fatal(err)
	}
	if n := len(gen.Response().File); n != 0 {
		t.Errorf("an existing stub must not be re-emitted, got %d files", n)
	}
	if len(asked) != 1 || asked[0] != "order_aggregate.stub" {
		t.Errorf("exists consulted with %v, want [order_aggregate.stub]", asked)
	}
}

func TestTemplates_LanguageMismatchRefused(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	gen, err := buildGen(t, ioPkg, orderAggregate(o)...)
	if err != nil {
		t.Fatal(err)
	}
	err = codegen.Generate(gen, "go", renderOpts(nil))
	if err == nil || !strings.Contains(err.Error(), "is a testlang template set, not go") {
		t.Fatalf("want a language mismatch error, got %v", err)
	}
}

func TestTemplates_InvalidDeclarationsFailBeforeRendering(t *testing.T) {
	o := buildOptionTypes(t, ioPkg)
	gen, err := buildGen(t, ioPkg, declMsg{"State", o.ownedDecl(1, "", "", "OrderAggregate")})
	if err != nil {
		t.Fatal(err)
	}
	if err := codegen.Generate(gen, "testlang", renderOpts(nil)); err == nil || !strings.Contains(err.Error(), "ANZ008") {
		t.Fatalf("want ANZ008, got %v", err)
	}
}

// writeSet writes a template set into a temp dir: manifest plus named files.
func writeSet(t *testing.T, manifest string, files map[string]string) string {
	t.Helper()
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "manifest.yaml"), []byte(manifest), 0o644); err != nil {
		t.Fatal(err)
	}
	for name, body := range files {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return dir
}

const minimalManifest = `schema_version: 1
language: x
outputs:
  - mode: codegen
    path: '{{ .component.name }}.out'
    template: a.tmpl
`

func TestLoadTemplateSet_RefusesBadManifests(t *testing.T) {
	cases := map[string]struct {
		manifest string
		files    map[string]string
		want     string
	}{
		"schema mismatch": {strings.Replace(minimalManifest, "schema_version: 1", "schema_version: 2", 1), map[string]string{"a.tmpl": ""}, "schema_version 2"},
		"no language":     {strings.Replace(minimalManifest, "language: x\n", "", 1), map[string]string{"a.tmpl": ""}, "language is required"},
		"unknown field":   {minimalManifest + "extra: 1\n", map[string]string{"a.tmpl": ""}, "extra"},
		"bad mode":        {strings.Replace(minimalManifest, "mode: codegen", "mode: wiring", 1), map[string]string{"a.tmpl": ""}, `mode "wiring"`},
		"missing tmpl":    {minimalManifest, map[string]string{"b.tmpl": ""}, `template "a.tmpl" not found`},
		"no tmpl files":   {minimalManifest, nil, "no *.tmpl files"},
		"bad kind":        {strings.Replace(minimalManifest, "mode: codegen", "mode: codegen\n    kinds: [SERVICE]", 1), map[string]string{"a.tmpl": ""}, `unknown kind "SERVICE"`},
		"no outputs":      {"schema_version: 1\nlanguage: x\n", map[string]string{"a.tmpl": ""}, "no outputs"},
		"empty path":      {strings.Replace(minimalManifest, "path: '{{ .component.name }}.out'", "path: ''", 1), map[string]string{"a.tmpl": ""}, "path is required"},
		"bad template":    {minimalManifest, map[string]string{"a.tmpl": "{{ .x "}, "a.tmpl"},
		"unknown helper":  {minimalManifest, map[string]string{"a.tmpl": "{{ nosuch }}"}, "nosuch"},
	}
	for name, tc := range cases {
		t.Run(name, func(t *testing.T) {
			_, err := codegen.LoadTemplateSet(writeSet(t, tc.manifest, tc.files))
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("want error containing %q, got %v", tc.want, err)
			}
		})
	}
}

// oneComponentModel is a hand-built model: one saga in a/b/c.proto whose
// handler messages live in two files, one of them named so its alias
// collides with a reserved alias.
func oneComponentModel() *codegen.Model {
	ref := func(file, name string) codegen.MessageRef {
		return codegen.MessageRef{FullName: "p." + name, Name: name, NestedNames: []string{"Outer", name}, Package: "p", File: file}
	}
	pf := func(p, dir, stem string) codegen.ProtoFile {
		return codegen.ProtoFile{Path: p, Dir: dir, Stem: stem, Package: "p", Options: map[string]string{}}
	}
	comp := codegen.ModelComponent{
		Kind: "SAGA", Name: "S", StubName: "SImpl",
		Anchor:        ref("a/b/c.proto", "S"),
		OutputDomains: []string{}, ProjectorDomains: []string{}, EmitsFacts: []string{},
		Handlers: []codegen.ModelHandler{
			{Message: ref("a/b/c.proto", "E1"), Method: "E1", Emits: []codegen.MessageRef{}},
			{Message: ref("x/t.proto", "E2"), Method: "E2", Emits: []codegen.MessageRef{}},
		},
		Appliers: []codegen.ModelApplier{}, Rejections: []codegen.ModelRejection{}, Undos: []codegen.ModelUndo{}, Facts: []codegen.ModelFact{},
		ReferencedFiles: []string{"a/b/c.proto", "x/t.proto"},
	}
	return &codegen.Model{
		SchemaVersion: 1,
		Files:         []codegen.ModelFile{{ProtoFile: pf("a/b/c.proto", "a/b", "c"), Components: []codegen.ModelComponent{comp}}},
		ProtoFiles:    map[string]codegen.ProtoFile{"a/b/c.proto": pf("a/b/c.proto", "a/b", "c"), "x/t.proto": pf("x/t.proto", "x", "t")},
	}
}

func TestRender_ImportAliasesAvoidReservedNamesAndTypeRefUsesThem(t *testing.T) {
	manifest := `schema_version: 1
language: x
imports:
  alias_prefix: "_"
  reserved_aliases: [_t]
types:
  message: '{{ .import.alias }}.{{ join "." .message.nested_names }}@{{ .file.dir }}'
outputs:
  - mode: codegen
    path: '{{ .file.dir }}/{{ snake .component.name }}.out'
    template: a.tmpl
`
	tmpl := `{{ range .imports }}{{ .alias }}={{ .path }};{{ end }}|{{ range .component.handlers }}{{ typeRef .message }};{{ end }}`
	ts, err := codegen.LoadTemplateSet(writeSet(t, manifest, map[string]string{"a.tmpl": tmpl}))
	if err != nil {
		t.Fatal(err)
	}
	out, err := ts.Render(oneComponentModel(), codegen.ModeCodegen, nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(out) != 1 || out[0].Path != "a/b/s.out" {
		t.Fatalf("outputs = %+v", out)
	}
	// x/t.proto is index 1 in the sorted import list; "_t" is reserved.
	want := "_c=a/b/c.proto;_t1=x/t.proto;|_c.Outer.E1@a/b;_t1.Outer.E2@x;"
	if got := string(out[0].Content); got != want {
		t.Errorf("render = %q, want %q", got, want)
	}
}

func TestRender_RefusesEscapingAndDuplicatePaths(t *testing.T) {
	for name, tc := range map[string]struct{ outputs, want string }{
		"escape": {`  - mode: codegen
    path: '../{{ .component.name }}'
    template: a.tmpl
`, "escapes the output directory"},
		"absolute": {`  - mode: codegen
    path: '/{{ .component.name }}'
    template: a.tmpl
`, "escapes the output directory"},
		"duplicate": {`  - mode: codegen
    path: 'same'
    template: a.tmpl
  - mode: codegen
    path: 'same'
    template: a.tmpl
`, `output path "same" is also rendered`},
	} {
		t.Run(name, func(t *testing.T) {
			ts, err := codegen.LoadTemplateSet(writeSet(t, "schema_version: 1\nlanguage: x\noutputs:\n"+tc.outputs, map[string]string{"a.tmpl": "x"}))
			if err != nil {
				t.Fatal(err)
			}
			if _, err := ts.Render(oneComponentModel(), codegen.ModeCodegen, nil, nil); err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("want %q, got %v", tc.want, err)
			}
		})
	}
}

func TestRender_TypeRefOutsideImportsAndWithoutHookFails(t *testing.T) {
	m := oneComponentModel()
	m.Files[0].Components[0].ReferencedFiles = []string{"a/b/c.proto"}
	withHook := "schema_version: 1\nlanguage: x\ntypes:\n  message: '{{ .import.alias }}'\noutputs:\n  - mode: codegen\n    path: o\n    template: a.tmpl\n"
	ts, err := codegen.LoadTemplateSet(writeSet(t, withHook, map[string]string{"a.tmpl": "{{ range .component.handlers }}{{ typeRef .message }}{{ end }}"}))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := ts.Render(m, codegen.ModeCodegen, nil, nil); err == nil || !strings.Contains(err.Error(), "not in the component's imports") {
		t.Fatalf("want an imports error, got %v", err)
	}
	ts, err = codegen.LoadTemplateSet(writeSet(t, minimalManifest, map[string]string{"a.tmpl": "{{ typeRef .component.anchor }}"}))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := ts.Render(oneComponentModel(), codegen.ModeCodegen, nil, nil); err == nil || !strings.Contains(err.Error(), "no types.message hook") {
		t.Fatalf("want a missing-hook error, got %v", err)
	}
}

func TestRender_MissingKeyIsAnError(t *testing.T) {
	ts, err := codegen.LoadTemplateSet(writeSet(t, minimalManifest, map[string]string{"a.tmpl": "{{ .component.no_such_key }}"}))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := ts.Render(oneComponentModel(), codegen.ModeCodegen, nil, nil); err == nil || !strings.Contains(err.Error(), "no_such_key") {
		t.Fatalf("a missing model key must fail rendering, got %v", err)
	}
}

func TestRender_MissingKeyInPathsAndIncludesIsAnError(t *testing.T) {
	for name, tc := range map[string]struct{ manifest, tmpl string }{
		"output path":     {strings.Replace(minimalManifest, "'{{ .component.name }}.out'", "'{{ .component.no_such_key }}.out'", 1), "x"},
		"included define": {minimalManifest, `{{ define "d" }}{{ .no_such_key }}{{ end }}{{ include "d" .component }}`},
	} {
		t.Run(name, func(t *testing.T) {
			ts, err := codegen.LoadTemplateSet(writeSet(t, tc.manifest, map[string]string{"a.tmpl": tc.tmpl}))
			if err != nil {
				t.Fatal(err)
			}
			if _, err := ts.Render(oneComponentModel(), codegen.ModeCodegen, nil, nil); err == nil || !strings.Contains(err.Error(), "no_such_key") {
				t.Fatalf("a missing model key must fail rendering, got %v", err)
			}
		})
	}
}
