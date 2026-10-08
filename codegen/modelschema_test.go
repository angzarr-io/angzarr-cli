package codegen_test

// The model JSON schema (docs/model.v1.schema.json) is the frozen contract
// templates read: every model the CLI emits validates against it, the schema
// admits nothing undocumented, and docs/templates.md names every key.

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"testing"

	"github.com/santhosh-tekuri/jsonschema/v6"

	"github.com/angzarr-io/angzarr-cli/codegen"
)

const (
	modelSchemaPath = "../docs/model.v1.schema.json"
	templatesDoc    = "../docs/templates.md"
	goldenDir       = "testdata/golden"
)

func compileModelSchema(t *testing.T) *jsonschema.Schema {
	t.Helper()
	c := jsonschema.NewCompiler()
	c.DefaultDraft(jsonschema.Draft2020)
	c.AssertFormat()
	raw, err := os.ReadFile(modelSchemaPath)
	if err != nil {
		t.Fatal(err)
	}
	doc, err := jsonschema.UnmarshalJSON(bytes.NewReader(raw))
	if err != nil {
		t.Fatalf("schema is not JSON: %v", err)
	}
	if err := c.AddResource("model.v1.schema.json", doc); err != nil {
		t.Fatal(err)
	}
	s, err := c.Compile("model.v1.schema.json")
	if err != nil {
		t.Fatalf("compile schema: %v", err)
	}
	return s
}

func validateModelJSON(t *testing.T, s *jsonschema.Schema, raw []byte) error {
	t.Helper()
	inst, err := jsonschema.UnmarshalJSON(bytes.NewReader(raw))
	if err != nil {
		t.Fatalf("model is not JSON: %v", err)
	}
	return s.Validate(inst)
}

func TestModelSchema_EmittedModelsValidate(t *testing.T) {
	s := compileModelSchema(t)
	o := buildOptionTypes(t, ioPkg)
	for name, msgs := range map[string][]declMsg{
		"aggregate":                orderAggregate(o),
		"multi-domain projector":   projectorMultiDomain(o),
		"aggregate, saga and PM":   pageContextSurface(o),
		"unnamed saga":             unnamedSaga(o),
		"input-domain projector":   projectorInputDomainOnly(o),
		"input+handler projectors": projectorInputPlusHandlerDomains(o),
	} {
		gen, err := buildGen(t, ioPkg, msgs...)
		if err != nil {
			t.Fatal(err)
		}
		model, diags := codegen.AnalyzeModel(gen)
		if model == nil {
			t.Fatalf("%s: AnalyzeModel: %v", name, diags)
		}
		raw, err := codegen.ModelJSON(model)
		if err != nil {
			t.Fatal(err)
		}
		if err := validateModelJSON(t, s, raw); err != nil {
			t.Errorf("%s: model does not validate:\n%v", name, err)
		}
	}
}

func TestModelSchema_GoldenModelsValidate(t *testing.T) {
	// The goldens are the models of the conformance and blackjack protos;
	// `just golden` keeps them current.
	s := compileModelSchema(t)
	for _, suite := range []string{"conformance", "blackjack"} {
		raw, err := os.ReadFile(filepath.Join(goldenDir, suite, "model", codegen.ModelFileName))
		if err != nil {
			t.Fatalf("%s golden model: %v", suite, err)
		}
		if err := validateModelJSON(t, s, raw); err != nil {
			t.Errorf("%s golden model does not validate:\n%v", suite, err)
		}
		var doc struct {
			Files []struct {
				Components []json.RawMessage `json:"components"`
			} `json:"files"`
		}
		if err := json.Unmarshal(raw, &doc); err != nil {
			t.Fatal(err)
		}
		n := 0
		for _, f := range doc.Files {
			n += len(f.Components)
		}
		if n < 5 {
			t.Errorf("%s golden model holds %d components; want the suite's components (≥ 5)", suite, n)
		}
	}
}

func TestModelSchema_RefusesWhatTheContractDoesNotDefine(t *testing.T) {
	s := compileModelSchema(t)
	raw, err := os.ReadFile(filepath.Join(goldenDir, "conformance", "model", codegen.ModelFileName))
	if err != nil {
		t.Fatal(err)
	}
	mutate := func(edit func(doc map[string]any)) []byte {
		var doc map[string]any
		if err := json.Unmarshal(raw, &doc); err != nil {
			t.Fatal(err)
		}
		edit(doc)
		out, err := json.Marshal(doc)
		if err != nil {
			t.Fatal(err)
		}
		return out
	}
	comp := func(doc map[string]any) map[string]any {
		return doc["files"].([]any)[0].(map[string]any)["components"].([]any)[0].(map[string]any)
	}
	for name, edit := range map[string]func(map[string]any){
		"unknown top-level key":   func(d map[string]any) { d["extra"] = 1 },
		"other schema version":    func(d map[string]any) { d["schema_version"] = 2 },
		"unknown component key":   func(d map[string]any) { comp(d)["extra"] = "x" },
		"missing component key":   func(d map[string]any) { delete(comp(d), "referenced_files") },
		"unknown kind":            func(d map[string]any) { comp(d)["kind"] = "SERVICE" },
		"null list":               func(d map[string]any) { comp(d)["handlers"] = nil },
		"aggregate without state": func(d map[string]any) { comp(d)["state"] = nil },
		"lower-case method": func(d map[string]any) {
			comp(d)["handlers"].([]any)[0].(map[string]any)["method"] = "handleIt"
		},
		"unknown file key": func(d map[string]any) { d["files"].([]any)[0].(map[string]any)["extra"] = 1 },
		"unknown file option": func(d map[string]any) {
			d["files"].([]any)[0].(map[string]any)["options"] = map[string]any{"cc_enable_arenas": "true"}
		},
		"missing proto file key": func(d map[string]any) {
			for _, pf := range d["proto_files"].(map[string]any) {
				delete(pf.(map[string]any), "top_level_enums")
			}
		},
		"unknown message ref key": func(d map[string]any) { comp(d)["anchor"].(map[string]any)["extra"] = 1 },
	} {
		if err := validateModelJSON(t, s, mutate(edit)); err == nil {
			t.Errorf("%s: the schema accepted it", name)
		}
	}
	if err := validateModelJSON(t, s, mutate(func(map[string]any) {})); err != nil {
		t.Fatalf("the unmodified golden must validate: %v", err)
	}
}

// schemaKeys collects every property name the schema defines.
func schemaKeys(node any, into map[string]bool) {
	switch v := node.(type) {
	case map[string]any:
		if props, ok := v["properties"].(map[string]any); ok {
			for k, sub := range props {
				into[k] = true
				schemaKeys(sub, into)
			}
		}
		for k, sub := range v {
			if k != "properties" {
				schemaKeys(sub, into)
			}
		}
	case []any:
		for _, sub := range v {
			schemaKeys(sub, into)
		}
	}
}

func TestModelSchema_EveryKeyIsDocumented(t *testing.T) {
	raw, err := os.ReadFile(modelSchemaPath)
	if err != nil {
		t.Fatal(err)
	}
	var schema any
	if err := json.Unmarshal(raw, &schema); err != nil {
		t.Fatal(err)
	}
	keys := map[string]bool{}
	schemaKeys(schema, keys)
	doc, err := os.ReadFile(templatesDoc)
	if err != nil {
		t.Fatal(err)
	}
	documented := map[string]bool{}
	for _, m := range regexp.MustCompile("`([a-z_]+)`").FindAllStringSubmatch(string(doc), -1) {
		documented[m[1]] = true
	}
	var missing []string
	for k := range keys {
		if !documented[k] {
			missing = append(missing, k)
		}
	}
	sort.Strings(missing)
	if len(missing) > 0 {
		t.Errorf("docs/templates.md does not document schema keys: %s", strings.Join(missing, ", "))
	}
	if len(keys) < 40 {
		t.Errorf("found %d schema keys; the walk lost the schema's properties", len(keys))
	}
}
