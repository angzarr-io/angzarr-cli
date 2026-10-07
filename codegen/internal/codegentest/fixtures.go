// Package codegentest holds descriptor fixtures shared by the codegen and
// emitter tests.
package codegentest

import (
	"testing"

	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/reflect/protodesc"
	"google.golang.org/protobuf/reflect/protoreflect"
	"google.golang.org/protobuf/types/descriptorpb"
)

// NestedFixture is the descriptor of n.Outer.Mid.Inner, a message nested two
// levels deep, for tests of per-language nested type names.
func NestedFixture(t testing.TB) protoreflect.MessageDescriptor {
	t.Helper()
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
	return fd.Messages().Get(0).Messages().Get(0).Messages().Get(0)
}
