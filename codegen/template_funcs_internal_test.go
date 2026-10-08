package codegen

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"
	"text/template"
)

func TestCaseHelpers(t *testing.T) {
	for _, tc := range []struct{ in, snake, pascal, camel string }{
		{"OrderCreated", "order_created", "OrderCreated", "orderCreated"},
		{"HTTPGet", "http_get", "HTTPGet", "hTTPGet"},
		{"ApplyIOEvent", "apply_io_event", "ApplyIOEvent", "applyIOEvent"},
		{"order_created", "order_created", "OrderCreated", "orderCreated"},
		{"Finish", "finish", "Finish", "finish"},
		{"", "", "", ""},
	} {
		if got := Snake(tc.in); got != tc.snake {
			t.Errorf("Snake(%q) = %q, want %q", tc.in, got, tc.snake)
		}
		if got := pascal(tc.in); got != tc.pascal {
			t.Errorf("pascal(%q) = %q, want %q", tc.in, got, tc.pascal)
		}
		if got := camel(tc.in); got != tc.camel {
			t.Errorf("camel(%q) = %q, want %q", tc.in, got, tc.camel)
		}
	}
}

// run executes a one-line template over data with the base helpers.
func run(t *testing.T, text string, data any) (string, error) {
	t.Helper()
	tmpl, err := template.New("t").Option("missingkey=error").Funcs(baseFuncs()).Parse(text)
	if err != nil {
		t.Fatalf("parse %q: %v", text, err)
	}
	var b bytes.Buffer
	err = tmpl.Execute(&b, data)
	return b.String(), err
}

func TestBaseHelpers(t *testing.T) {
	// JSON-decoded data, as templates see the model.
	var data map[string]any
	dec := json.NewDecoder(strings.NewReader(`{"names":["io","angzarr","v1"],"other":["io","angzarr","examples"],"n":3,"empty":[],"words":["b","a"]}`))
	dec.UseNumber()
	if err := dec.Decode(&data); err != nil {
		t.Fatal(err)
	}
	for text, want := range map[string]string{
		`{{ join "." .names }}`:                                                        "io.angzarr.v1",
		`{{ join "," (split "/" "a/b/c") }}`:                                           "a,b,c",
		`{{ commonPrefix .names .other }}`:                                             "2",
		`{{ commonPrefix .names .empty }}`:                                             "0",
		`{{ commonPrefix (list "io") .names }}{{ commonPrefix .names (list "io") }}`:   "11",
		`{{ repeat (add (sub .n 2) 1) "." }}`:                                          "..",
		`{{ repeat (sub 0 4) "." }}`:                                                   "",
		`{{ last .names }}|{{ first .names }}`:                                         "v1|io",
		`{{ join "." (initial .names) }}`:                                              "io.angzarr",
		`{{ join "." (rest .names) }}`:                                                 "angzarr.v1",
		`{{ len (initial .empty) }}{{ len (rest .empty) }}`:                            "00",
		`{{ join "" (sortStrings .words) }}`:                                           "ab",
		`{{ has .names "v1" }} {{ has .names "v2" }}`:                                  "true false",
		`{{ join "," (append .words "c") }}|{{ join "," .words }}`:                     "b,a,c|b,a",
		`{{ quote "a\"b" }}`:                                                           `"a\"b"`,
		`{{ replace "/" "." "a/b" }}`:                                                  "a.b",
		`{{ trimPrefix "io." "io.x" }}{{ trimSuffix ".proto" "c.proto" }}`:             "xc",
		`{{ hasPrefix "io/" "io/a" }}{{ hasSuffix "x" "ab" }}{{ contains "b" "abc" }}`: "truefalsetrue",
		`{{ upper "ab" }}{{ lower "CD" }}`:                                             "ABcd",
		`{{ $d := dict "k" "v" }}{{ set $d "k2" "w" }}{{ get $d "k" }}{{ get $d "k2" }}{{ hasKey $d "k" }}{{ hasKey $d "z" }}`: "vwtruefalse",
		`{{ len (list 1 2 3) }}`: "3",
	} {
		got, err := run(t, text, data)
		if err != nil {
			t.Errorf("%s: %v", text, err)
			continue
		}
		if got != want {
			t.Errorf("%s = %q, want %q", text, got, want)
		}
	}
}

func TestBaseHelpers_Errors(t *testing.T) {
	for text, want := range map[string]string{
		`{{ fail "boom" }}`:             "boom",
		`{{ first (list) }}`:            "empty list",
		`{{ dict "k" }}`:                "key/value pairs",
		`{{ dict 1 2 }}`:                "want string",
		`{{ join "," "notalist" }}`:     "want a list",
		`{{ add "x" 1 }}`:               "want an integer",
		`{{ include "x" . }}`:           "outside a render",
		`{{ typeRef . }}`:               "outside a render",
		`{{ commonPrefix "a" (list) }}`: "want a list",
		`{{ has 3 "x" }}`:               "want a list",
		`{{ sortStrings 1 }}`:           "want a list",
		`{{ repeat "x" "." }}`:          "want an integer",
		`{{ initial 1 }}{{ rest 1 }}`:   "want a list",
		`{{ append 1 2 }}`:              "want a list",
	} {
		if _, err := run(t, text, nil); err == nil || !strings.Contains(err.Error(), want) {
			t.Errorf("%s: want error containing %q, got %v", text, want, err)
		}
	}
}

func TestNamingHelpers(t *testing.T) {
	for text, want := range map[string]string{
		// protocPascal: protoc's file-name / package-segment rule.
		`{{ protocPascal "buy_in" }}`:      "BuyIn",
		`{{ protocPascal "foo2bar" }}`:     "Foo2Bar",
		`{{ protocPascal "buy-in.v1" }}`:   "BuyInV1",
		`{{ protocPascal "HTTPGet" }}`:     "HTTPGet",
		`{{ protocPascal "" }}`:            "",
		`{{ lowerFirst "OrderCreated" }}`:  "orderCreated",
		`{{ lowerFirst "Order_created" }}`: "order_created",
		`{{ lowerFirst "" }}`:              "",
		`{{ upperFirst "orderCreated" }}`:  "OrderCreated",
		`{{ upperFirst "éa" }}`:            "Éa",
		`{{ upperFirst "" }}`:              "",
		`{{ identifier "examples-v1" }}`:   "examples_v1",
		`{{ identifier "2fa" }}`:           "_2fa",
		`{{ identifier "a.b/c" }}`:         "a_b_c",
		`{{ identifier "_ok9" }}`:          "_ok9",
		`{{ identifier "ünï" }}`:           "ünï",
		`{{ identifier "" }}`:              "_",
	} {
		got, err := run(t, text, nil)
		if err != nil {
			t.Errorf("%s: %v", text, err)
			continue
		}
		if got != want {
			t.Errorf("%s = %q, want %q", text, got, want)
		}
	}
}

func TestLayoutHelpers(t *testing.T) {
	var data map[string]any
	if err := json.Unmarshal([]byte(`{"paths":["b/x.proto","a/y.proto","b/x.proto","a/y.proto","c"],"empty":[]}`), &data); err != nil {
		t.Fatal(err)
	}
	for text, want := range map[string]string{
		`{{ join "," (uniq .paths) }}`:           "b/x.proto,a/y.proto,c",
		`{{ len (uniq .empty) }}`:                "0",
		`{{ pathJoin "." "a" "b.go" }}`:          "a/b.go",
		`{{ pathJoin "io/angzarr" "../x" }}`:     "io/x",
		`{{ pathJoin }}`:                         "",
		`{{ pathDir "io/angzarr/v1/x.proto" }}`:  "io/angzarr/v1",
		`{{ pathDir "x.proto" }}`:                ".",
		`{{ pathBase "io/angzarr/v1/x.proto" }}`: "x.proto",
		`{{ trim "  a b \n" }}`:                  "a b",
		"{{ indent 2 \"a\\n\\nb\\n\" }}":         "  a\n\n  b\n",
		"{{ indent 4 \"x\" }}":                   "    x",
		"{{ indent 0 \"x\\ny\" }}":               "x\ny",
		"{{ indent -1 \"x\" }}":                  "x",
		"{{ indent 1 \" \\t\\nz\" }}":            " \t\n z",
	} {
		got, err := run(t, text, data)
		if err != nil {
			t.Errorf("%s: %v", text, err)
			continue
		}
		if got != want {
			t.Errorf("%s = %q, want %q", text, got, want)
		}
	}
	for text, want := range map[string]string{
		`{{ uniq 1 }}`:        "want a list",
		`{{ indent "x" "" }}`: "want an integer",
	} {
		if _, err := run(t, text, nil); err == nil || !strings.Contains(err.Error(), want) {
			t.Errorf("%s: want error containing %q, got %v", text, want, err)
		}
	}
}
