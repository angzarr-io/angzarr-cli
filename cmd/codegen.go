package cmd

import (
	"fmt"
	"io"

	"github.com/angzarr-io/angzarr-cli/codegen"
	// Registers the built-in language emitters before this package's init
	// builds one subcommand per registered language.
	_ "github.com/angzarr-io/angzarr-cli/codegen/emit/builtin"
	"github.com/spf13/cobra"
)

// codegenCmd hosts one subcommand per target language. Each language
// subcommand IS a protoc plugin: it reads a CodeGeneratorRequest on stdin
// and writes a CodeGeneratorResponse on stdout, so buf invokes it as
//
//	plugins:
//	  - local: ["angzarr", "codegen", "go"]
//	    out: gen
//	    opt: paths=source_relative
//	    strategy: all
//
// Declaration validation is language-independent and runs identically for
// every emitter — a misdeclared component fails generation the same way
// everywhere.
var codegenCmd = &cobra.Command{
	Use:   "codegen",
	Short: "Generate per-language dispatch wiring from proto component declarations",
	Long: `Generate dispatch wiring from messages carrying the
(io.angzarr.v1.component / .command / .event) options: a strict handler
interface plus a dispatch-table constructor over the angzarr-router binding
per declared component.

Each language subcommand speaks the protoc plugin contract on
stdin/stdout. Components reference their commands and events by name, so
every file declaring part of a component must be in the same plugin run:
configure buf with strategy: all on the angzarr plugins (a split run fails
with ANZ013).`,
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
// protocol-level failures (unreadable request, unknown parameter) exit
// nonzero.
//
// protogen.Options.Run is not used: it inspects os.Args itself and
// rejects the subcommand arguments cobra routes on.
func runPlugin(in io.Reader, out io.Writer, lang string) error {
	gen, params, err := readPlugin(in, paramKeys{})
	if err != nil {
		return err
	}
	if err := codegen.Generate(gen, lang, params.opts); err != nil {
		gen.Error(err)
	}
	return writeResponse(out, gen)
}
