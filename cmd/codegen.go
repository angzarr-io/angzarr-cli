package cmd

import (
	"fmt"
	"io"

	"github.com/spf13/cobra"
	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/pluginpb"

	"github.com/angzarr-io/angzarr-cli/codegen"
)

// codegenCmd hosts one subcommand per target language. Each language
// subcommand IS a protoc plugin: it reads a CodeGeneratorRequest on stdin
// and writes a CodeGeneratorResponse on stdout, so buf invokes it as
//
//	plugins:
//	  - local: ["angzarr", "codegen", "go"]
//	    out: proto
//
// Declaration validation is language-independent and runs identically for
// every emitter — a misdeclared component fails generation the same way
// everywhere.
var codegenCmd = &cobra.Command{
	Use:   "codegen",
	Short: "Generate per-language dispatch wiring from proto component declarations",
	Long: `Generate dispatch wiring from proto services carrying
(angzarr.v1.component) options: a strict handler interface plus an engine
dispatch-table constructor per declared component.

Each language subcommand speaks the protoc plugin contract on
stdin/stdout. Generated code is a thin table population over that
language's angzarr client engine; transport stays on the generic
framework services and the Any envelope.`,
}

func init() {
	for _, lang := range codegen.Languages() {
		codegenCmd.AddCommand(languageCommand(lang))
	}
	codegenCmd.AddCommand(&cobra.Command{
		Use:   "languages",
		Short: "List target languages with registered emitters",
		Run: func(cmd *cobra.Command, _ []string) {
			for _, lang := range codegen.Languages() {
				fmt.Fprintln(cmd.OutOrStdout(), lang)
			}
		},
	})
	rootCmd.AddCommand(codegenCmd)
}

func languageCommand(lang string) *cobra.Command {
	return &cobra.Command{
		Use:   lang,
		Short: fmt.Sprintf("protoc plugin emitting %s dispatch wiring (CodeGeneratorRequest on stdin)", lang),
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			return runPlugin(cmd.InOrStdin(), cmd.OutOrStdout(), lang)
		},
	}
}

// runPlugin speaks the protoc plugin protocol. Generation failures travel
// inside the response per the protocol (protoc/buf surface them); only
// protocol-level failures (unreadable request) exit nonzero.
//
// protogen.Options.Run is not used: it inspects os.Args itself and
// rejects the subcommand arguments cobra routes on.
func runPlugin(in io.Reader, out io.Writer, lang string) error {
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
	if err := codegen.Generate(gen, lang); err != nil {
		gen.Error(err)
	}
	resp, err := proto.Marshal(gen.Response())
	if err != nil {
		return fmt.Errorf("marshal CodeGeneratorResponse: %w", err)
	}
	_, err = out.Write(resp)
	return err
}
