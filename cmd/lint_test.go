package cmd

import (
	"bytes"
	"strings"
	"testing"

	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/descriptorpb"
)

func strp(s string) *string { return &s }

// cleanImage is a one-file FileDescriptorSet with no angzarr markers: lint has
// nothing to flag, so the command must succeed.
func cleanImage(t *testing.T) []byte {
	t.Helper()
	set := &descriptorpb.FileDescriptorSet{
		File: []*descriptorpb.FileDescriptorProto{{
			Name:    strp("plain.proto"),
			Package: strp("plain"),
			Syntax:  strp("proto3"),
			Options: &descriptorpb.FileOptions{GoPackage: strp("example.test/plain;plain")},
			MessageType: []*descriptorpb.DescriptorProto{
				{Name: strp("Thing")},
			},
		}},
	}
	raw, err := proto.Marshal(set)
	if err != nil {
		t.Fatalf("marshal image: %v", err)
	}
	return raw
}

func TestRunLint_CleanImage_Succeeds(t *testing.T) {
	var out, errOut bytes.Buffer
	if err := runLint(bytes.NewReader(cleanImage(t)), &out, &errOut, false); err != nil {
		t.Fatalf("runLint on clean image: %v (stderr: %s)", err, errOut.String())
	}
	if !strings.Contains(out.String(), "lint OK") {
		t.Errorf("stdout = %q, want lint OK", out.String())
	}
}

func TestRunLint_GarbageInput_Errors(t *testing.T) {
	var out, errOut bytes.Buffer
	err := runLint(strings.NewReader("not a descriptor set at all"), &out, &errOut, false)
	if err == nil {
		t.Fatal("expected error on garbage input")
	}
}

func TestRunLint_ImageWithoutGoPackage_Lints(t *testing.T) {
	// Lint emits no Go: files need no go_package.
	set := &descriptorpb.FileDescriptorSet{File: []*descriptorpb.FileDescriptorProto{{
		Name: strp("a/plain.proto"), Package: strp("plain"), Syntax: strp("proto3"),
		MessageType: []*descriptorpb.DescriptorProto{{Name: strp("Thing")}},
	}}}
	raw, err := proto.Marshal(set)
	if err != nil {
		t.Fatal(err)
	}
	var out, errOut bytes.Buffer
	if err := runLint(bytes.NewReader(raw), &out, &errOut, false); err != nil {
		t.Fatalf("runLint: %v (stderr: %s)", err, errOut.String())
	}
	if !strings.Contains(out.String(), "lint OK") {
		t.Errorf("stdout = %q, want lint OK", out.String())
	}
}

func TestRunLint_RequestWithoutGoPackage_Lints(t *testing.T) {
	var out, errOut bytes.Buffer
	if err := runLint(bytes.NewReader(aggregateRequestGoPackage(t, "", "")), &out, &errOut, true); err != nil {
		t.Fatalf("runLint --request: %v (stderr %s)", err, errOut.String())
	}
	if !strings.Contains(errOut.String(), "ANZ103") {
		t.Errorf("want ANZ103 from analysing the request, got stderr %q", errOut.String())
	}
}
