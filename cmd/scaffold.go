package cmd

import (
	"fmt"
	"io"
	"os"

	"github.com/spf13/cobra"
	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/pluginpb"

	"github.com/angzarr-io/angzarr-cli/codegen"
)

// scaffoldCmd hosts one subcommand per target language. Like codegen, each
// language subcommand IS a protoc plugin, but it emits the GENERATE-ONCE
// handler stub rather than the regenerated wiring:
//
//	plugins:
//	  - local: ["angzarr", "scaffold", "go"]
//	    out: .
//	    opt: paths=source_relative
//
// A stub is emitted only when its file does not yet exist; once a developer
// owns it, regeneration leaves it untouched. The existence check is relative
// to the working directory buf runs the plugin in (the module root), so the
// stub must be configured with out: . and paths=source_relative — the response
// path then matches the on-disk path.
var scaffoldCmd = &cobra.Command{
	Use:   "scaffold",
	Short: "Generate developer-owned handler stubs (once) from proto component declarations",
	Long: `Generate a handler stub per declared component: a struct implementing
the strict <Component>Handler interface with one TODO method per command and
event. Each stub is emitted ONCE — regeneration never overwrites an existing
stub, so the implementation is yours to keep. A compile-time interface
assertion fails the build when a command or event is added to the proto until
the matching method is implemented.

Each language subcommand speaks the protoc plugin contract on stdin/stdout.`,
}

func init() {
	for _, lang := range codegen.Languages() {
		scaffoldCmd.AddCommand(scaffoldLanguageCommand(lang))
	}
	rootCmd.AddCommand(scaffoldCmd)
}

func scaffoldLanguageCommand(lang string) *cobra.Command {
	return &cobra.Command{
		Use:   lang,
		Short: fmt.Sprintf("protoc plugin emitting %s handler stubs once (CodeGeneratorRequest on stdin)", lang),
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			return runScaffold(cmd.InOrStdin(), cmd.OutOrStdout(), lang)
		},
	}
}

// runScaffold speaks the protoc plugin protocol, emitting only the stubs whose
// files are absent on disk. Generation failures travel inside the response;
// only protocol-level failures (unreadable request) exit nonzero.
func runScaffold(in io.Reader, out io.Writer, lang string) error {
	raw, err := io.ReadAll(in)
	if err != nil {
		return fmt.Errorf("read CodeGeneratorRequest: %w", err)
	}
	req := &pluginpb.CodeGeneratorRequest{}
	if err := proto.Unmarshal(raw, req); err != nil {
		return fmt.Errorf("parse CodeGeneratorRequest: %w", err)
	}
	gen, err := protogen.Options{}.New(req)
	if err != nil {
		return err
	}
	if err := codegen.GenerateScaffold(gen, lang, fileExists); err != nil {
		gen.Error(err)
	}
	resp, err := proto.Marshal(gen.Response())
	if err != nil {
		return fmt.Errorf("marshal CodeGeneratorResponse: %w", err)
	}
	_, err = out.Write(resp)
	return err
}

// fileExists reports whether a response-relative path already exists on disk,
// resolved against the working directory buf runs the plugin in.
func fileExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}
