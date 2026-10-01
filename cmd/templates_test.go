package cmd

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/pluginpb"
)

// goTemplateSet writes a one-output-per-mode template set for "go".
func goTemplateSet(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	manifest := `schema_version: 1
language: go
params:
  greet: hello
outputs:
  - mode: codegen
    path: '{{ snake .component.name }}.tmpl.out'
    template: wire.tmpl
  - mode: scaffold
    path: '{{ snake .component.name }}.stub.out'
    template: stub.tmpl
`
	for name, body := range map[string]string{
		"manifest.yaml": manifest,
		"wire.tmpl":     "{{ .params.greet }} {{ .component.name }}",
		"stub.tmpl":     "stub {{ .component.stub_name }}",
	} {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return dir
}

func decodeResponse(t *testing.T, raw []byte) *pluginpb.CodeGeneratorResponse {
	t.Helper()
	resp := &pluginpb.CodeGeneratorResponse{}
	if err := proto.Unmarshal(raw, resp); err != nil {
		t.Fatal(err)
	}
	return resp
}

func TestRunPlugin_TemplatesOptionRendersTheSetWithParams(t *testing.T) {
	var out bytes.Buffer
	param := "paths=source_relative,templates=" + goTemplateSet(t) + ",param.greet=yo"
	if err := runPlugin(bytes.NewReader(aggregateRequest(t, param)), &out, "go"); err != nil {
		t.Fatal(err)
	}
	resp := decodeResponse(t, out.Bytes())
	if resp.GetError() != "" {
		t.Fatalf("response error: %s", resp.GetError())
	}
	if len(resp.File) != 1 || resp.File[0].GetName() != "order_aggregate.tmpl.out" || resp.File[0].GetContent() != "yo OrderAggregate" {
		t.Fatalf("templates= must replace the built-in emitter, got %v", resp.File)
	}
}

func TestRunScaffold_TemplatesOptionRendersScaffoldOutputs(t *testing.T) {
	var out bytes.Buffer
	param := "templates=" + goTemplateSet(t) + ",out_dir=" + t.TempDir()
	if err := runScaffold(bytes.NewReader(aggregateRequest(t, param)), &out, "go"); err != nil {
		t.Fatal(err)
	}
	resp := decodeResponse(t, out.Bytes())
	if len(resp.File) != 1 || resp.File[0].GetName() != "order_aggregate.stub.out" || resp.File[0].GetContent() != "stub OrderAggregate" {
		t.Fatalf("scaffold outputs = %v (error %q)", resp.File, resp.GetError())
	}
}

func TestRunModel_EmitsTheModelJSON(t *testing.T) {
	var out bytes.Buffer
	if err := runModel(bytes.NewReader(aggregateRequest(t, "paths=source_relative")), &out); err != nil {
		t.Fatal(err)
	}
	resp := decodeResponse(t, out.Bytes())
	if len(resp.File) != 1 || resp.File[0].GetName() != "angzarr.model.json" {
		t.Fatalf("model plugin output = %v (error %q)", resp.File, resp.GetError())
	}
	c := resp.File[0].GetContent()
	for _, want := range []string{`"schema_version": 1`, `"name": "OrderAggregate"`, `"path": "orders.proto"`} {
		if !strings.Contains(c, want) {
			t.Errorf("model JSON lacks %s:\n%s", want, c)
		}
	}
}

func TestRunModel_RefusesRenderingParameters(t *testing.T) {
	for _, param := range []string{"templates=x", "param.a=b"} {
		var out bytes.Buffer
		err := runModel(bytes.NewReader(aggregateRequest(t, param)), &out)
		if err == nil || !strings.Contains(err.Error(), "unknown parameter") {
			t.Errorf("%s: the model plugin renders nothing and must refuse it, got %v", param, err)
		}
	}
}
