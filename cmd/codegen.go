package cmd

import (
	"fmt"

	"github.com/spf13/cobra"
	"google.golang.org/protobuf/compiler/protogen"

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
		RunE: func(*cobra.Command, []string) error {
			protogen.Options{}.Run(func(gen *protogen.Plugin) error {
				return codegen.Generate(gen, lang)
			})
			return nil
		},
	}
}
