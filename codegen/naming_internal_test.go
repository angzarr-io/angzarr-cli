package codegen

import (
	"testing"

	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/reflect/protodesc"
	"google.golang.org/protobuf/types/descriptorpb"
)

func TestSnakeToPascal_MatchesProtocOuterClassRule(t *testing.T) {
	// protoc's UnderscoresToCamelCase: capitalise the first letter and any
	// letter following an underscore or a digit; drop non-alphanumerics.
	for in, want := range map[string]string{
		"orders":      "Orders",
		"order_saga":  "OrderSaga",
		"foo2bar":     "Foo2Bar",
		"v1_2beta":    "V12Beta",
		"FOO_bar":     "FOOBar",
		"a-b.c":       "ABC",
		"__x":         "X",
		"table_hand9": "TableHand9",
	} {
		if got := snakeToPascal(in); got != want {
			t.Errorf("snakeToPascal(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestQuoteLiteral_EscapesForCFamilyAndScriptLanguages(t *testing.T) {
	for in, want := range map[string]string{
		`orders`:  `"orders"`,
		`or"ders`: `"or\"ders"`,
		`a\b`:     `"a\\b"`,
		"l1\nl2":  `"l1\nl2"`,
		"t\tr\r":  `"t\tr\r"`,
	} {
		for name, q := range map[string]func(string) string{"cpp": cppQuote, "python": pyQuote, "typescript": tsQuote} {
			if got := q(in); got != want {
				t.Errorf("%s quote(%q) = %s, want %s", name, in, got, want)
			}
		}
	}
}

func TestNestedTypeNames_PerLanguage(t *testing.T) {
	fdp := &descriptorpb.FileDescriptorProto{
		Name:    proto.String("n.proto"),
		Package: proto.String("n"),
		Syntax:  proto.String("proto3"),
		MessageType: []*descriptorpb.DescriptorProto{{
			Name:       proto.String("Outer"),
			NestedType: []*descriptorpb.DescriptorProto{{Name: proto.String("Mid"), NestedType: []*descriptorpb.DescriptorProto{{Name: proto.String("Inner")}}}},
		}},
	}
	fd, err := protodesc.NewFile(fdp, nil)
	if err != nil {
		t.Fatal(err)
	}
	inner := fd.Messages().Get(0).Messages().Get(0).Messages().Get(0)
	for name, c := range map[string]struct {
		got, want string
	}{
		"java":   {messageNestedName(inner), "Outer.Mid.Inner"},
		"csharp": {csNestedName(inner), "Outer.Types.Mid.Types.Inner"},
		"cpp":    {cppNestedName(inner), "Outer::Mid::Inner"},
		"ts":     {tsNestedName(inner), "Outer_Mid_Inner"},
	} {
		if c.got != c.want {
			t.Errorf("%s nested name = %q, want %q", name, c.got, c.want)
		}
	}
}
