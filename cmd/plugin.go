package cmd

import (
	"fmt"
	"io"
	"strings"

	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/proto"
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
	params := &pluginParams{}
	pgo := protogen.Options{
		ParamFunc: func(name, value string) error {
			switch {
			case name == "py_framework_package":
				// The package a python consumer imports the angzarr framework
				// protos from (e.g. angzarr_router_ffi.gen).
				params.opts.PyFrameworkPackage = value
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
