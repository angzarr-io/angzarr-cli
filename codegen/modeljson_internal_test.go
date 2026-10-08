package codegen

import (
	"reflect"
	"testing"

	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/types/descriptorpb"
	"google.golang.org/protobuf/types/pluginpb"
)

func TestProtoFile_ListsTopLevelDeclarationsInOrder(t *testing.T) {
	s := func(v string) *string { return &v }
	fdp := &descriptorpb.FileDescriptorProto{
		Name:    s("a/b/shop.proto"),
		Package: s("a.b"),
		Syntax:  s("proto3"),
		Options: &descriptorpb.FileOptions{GoPackage: s("x/b;b")},
		MessageType: []*descriptorpb.DescriptorProto{
			{Name: s("Zeta"), NestedType: []*descriptorpb.DescriptorProto{{Name: s("Inner")}}},
			{Name: s("Alpha")},
		},
		EnumType: []*descriptorpb.EnumDescriptorProto{
			{Name: s("Shop"), Value: []*descriptorpb.EnumValueDescriptorProto{{Name: s("SHOP_UNSPECIFIED"), Number: new(int32)}}},
		},
		Service: []*descriptorpb.ServiceDescriptorProto{{Name: s("ShopService")}},
	}
	gen, err := protogen.Options{}.New(&pluginpb.CodeGeneratorRequest{FileToGenerate: []string{"a/b/shop.proto"}, ProtoFile: []*descriptorpb.FileDescriptorProto{fdp}})
	if err != nil {
		t.Fatal(err)
	}
	pf := protoFile(gen.Files[0])
	if pf.Path != "a/b/shop.proto" || pf.Dir != "a/b" || pf.Stem != "shop" || pf.Package != "a.b" {
		t.Errorf("identity = %+v", pf)
	}
	if !reflect.DeepEqual(pf.TopLevelMessages, []string{"Zeta", "Alpha"}) {
		t.Errorf("top_level_messages = %v, want [Zeta Alpha] (declaration order, nested excluded)", pf.TopLevelMessages)
	}
	if !reflect.DeepEqual(pf.TopLevelEnums, []string{"Shop"}) {
		t.Errorf("top_level_enums = %v", pf.TopLevelEnums)
	}
	if !reflect.DeepEqual(pf.TopLevelServices, []string{"ShopService"}) {
		t.Errorf("top_level_services = %v", pf.TopLevelServices)
	}
}

func TestProtoFile_EmptyDeclarationListsAreNotNull(t *testing.T) {
	s := func(v string) *string { return &v }
	fdp := &descriptorpb.FileDescriptorProto{Name: s("e.proto"), Package: s("e"), Syntax: s("proto3"), Options: &descriptorpb.FileOptions{GoPackage: s("x/e;e")}}
	gen, err := protogen.Options{}.New(&pluginpb.CodeGeneratorRequest{FileToGenerate: []string{"e.proto"}, ProtoFile: []*descriptorpb.FileDescriptorProto{fdp}})
	if err != nil {
		t.Fatal(err)
	}
	pf := protoFile(gen.Files[0])
	if pf.Dir != "." || pf.TopLevelMessages == nil || pf.TopLevelEnums == nil || pf.TopLevelServices == nil {
		t.Errorf("empty file = %+v; lists must be present (empty), dir \".\"", pf)
	}
}
