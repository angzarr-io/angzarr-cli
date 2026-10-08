package cmd

import (
	"fmt"
	"io"
	"path"
	"strings"

	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/descriptorpb"
	"google.golang.org/protobuf/types/pluginpb"

	"github.com/angzarr-io/angzarr-cli/codegen"
)

// pluginParams are the angzarr-specific protoc plugin parameters. protogen
// handles its own (paths, module, M<file>) and hands every other key here.
type pluginParams struct {
	opts codegen.Options
	// outDir is the directory buf writes this plugin's output to (its out:),
	// relative to the directory buf runs in. Scaffold resolves existing stubs
	// against it.
	outDir    string
	outDirSet bool
}

// paramKeys lists the parameters a plugin accepts; any other key is an error.
type paramKeys struct {
	outDir bool
	// modelOnly refuses the rendering parameters (templates=, param.*): the
	// model plugin emits the language-neutral model and renders nothing.
	modelOnly bool
	// analysisOnly marks a run that only analyses the request (lint).
	analysisOnly bool
}

// placeholderImportPath prefixes the Go import path given to a file that sets
// no go_package in a run that emits no Go. The .invalid TLD never resolves.
const placeholderImportPath = "angzarr.invalid"

// rendersOnly reports whether a run emits no Go of its own: the model
// plugin, lint, and any run rendering a template set (templates=). Such runs
// do not need the Go import paths protogen otherwise demands of every file.
func rendersOnly(parameter string, keys paramKeys) bool {
	if keys.modelOnly || keys.analysisOnly {
		return true
	}
	for _, p := range strings.Split(parameter, ",") {
		if name, _, _ := strings.Cut(p, "="); name == "templates" {
			return true
		}
	}
	return false
}

// withPlaceholderImportPaths appends an M<file>=<import path> mapping to the
// plugin parameter for every file that sets no go_package and has no M
// mapping yet, so protogen accepts a request carrying no Go options. The
// placeholder never reaches generated code or the model (which reports the
// options the files set).
func withPlaceholderImportPaths(parameter string, files []*descriptorpb.FileDescriptorProto) string {
	mapped := map[string]bool{}
	for _, p := range strings.Split(parameter, ",") {
		if name, _, ok := strings.Cut(p, "="); ok && strings.HasPrefix(name, "M") {
			mapped[name[1:]] = true
		}
	}
	params := []string{}
	if parameter != "" {
		params = append(params, parameter)
	}
	for _, f := range files {
		if f.GetOptions().GetGoPackage() != "" || mapped[f.GetName()] {
			continue
		}
		params = append(params, "M"+f.GetName()+"="+path.Join(placeholderImportPath, path.Dir(f.GetName())))
	}
	return strings.Join(params, ",")
}

// readPlugin reads a CodeGeneratorRequest and builds the protogen.Plugin,
// collecting the angzarr parameters the caller accepts.
func readPlugin(in io.Reader, keys paramKeys) (*protogen.Plugin, *pluginParams, error) {
	raw, err := io.ReadAll(in)
	if err != nil {
		return nil, nil, fmt.Errorf("read CodeGeneratorRequest: %w", err)
	}
	return pluginFromRequest(raw, keys)
}

// pluginFromRequest builds a protogen.Plugin from a serialized
// CodeGeneratorRequest.
func pluginFromRequest(raw []byte, keys paramKeys) (*protogen.Plugin, *pluginParams, error) {
	req := &pluginpb.CodeGeneratorRequest{}
	if err := proto.Unmarshal(raw, req); err != nil {
		return nil, nil, fmt.Errorf("parse CodeGeneratorRequest: %w", err)
	}
	if rendersOnly(req.GetParameter(), keys) {
		req.Parameter = proto.String(withPlaceholderImportPaths(req.GetParameter(), req.GetProtoFile()))
	}
	params := &pluginParams{}
	pgo := protogen.Options{
		ParamFunc: func(name, value string) error {
			switch {
			case name == "templates" && !keys.modelOnly:
				// A client repo's template set: github.com/org/repo@rev
				// (fetched and cached) or a local directory.
				params.opts.Templates = value
			case strings.HasPrefix(name, "param.") && !keys.modelOnly:
				// A template parameter override (param.<name>=<value>);
				// the template set declares the names it accepts.
				if params.opts.Params == nil {
					params.opts.Params = map[string]string{}
				}
				params.opts.Params[strings.TrimPrefix(name, "param.")] = value
			case name == "out_dir" && keys.outDir:
				params.outDir = value
				params.outDirSet = true
			default:
				return fmt.Errorf("unknown parameter %q", name)
			}
			return nil
		},
	}
	gen, err := pgo.New(req)
	if err != nil {
		return nil, nil, err
	}
	return gen, params, nil
}

// writeResponse marshals the plugin's CodeGeneratorResponse to out.
func writeResponse(out io.Writer, gen *protogen.Plugin) error {
	resp, err := proto.Marshal(gen.Response())
	if err != nil {
		return fmt.Errorf("marshal CodeGeneratorResponse: %w", err)
	}
	_, err = out.Write(resp)
	return err
}
