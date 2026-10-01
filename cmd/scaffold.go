package cmd

import (
	"fmt"
	"io"
	"os"
	"path/filepath"

	"github.com/angzarr-io/angzarr-cli/codegen"
	"github.com/spf13/cobra"
)

// scaffoldCmd hosts one subcommand per target language. Like codegen, each
// language subcommand IS a protoc plugin, but it emits the GENERATE-ONCE
// handler stub rather than the regenerated wiring:
//
//	plugins:
//	  - local: ["angzarr", "scaffold", "go"]
//	    out: src
//	    opt: [paths=source_relative, out_dir=src]
//
// A stub is emitted only when its file does not yet exist; once a developer
// owns it, regeneration leaves it untouched. protoc never tells a plugin where
// its output lands, so out_dir must repeat the out: directory: existing stubs
// are looked up at out_dir/<response path>, relative to the directory buf runs
// in. Scaffold refuses to run without it rather than risk overwriting a stub.
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
// files are absent under out_dir. Generation failures travel inside the
// response; only protocol-level failures (unreadable request, unknown
// parameter) exit nonzero.
func runScaffold(in io.Reader, out io.Writer, lang string) error {
	gen, params, err := readPlugin(in, paramKeys{outDir: true})
	if err != nil {
		return err
	}
	if !params.outDirSet {
		gen.Error(fmt.Errorf("scaffold requires the plugin parameter out_dir=<the plugin's out: directory>, " +
			"so existing stubs are found where buf writes them and never overwritten (e.g. opt: [paths=source_relative, out_dir=.] with out: .)"))
		return writeResponse(out, gen)
	}
	if err := codegen.GenerateScaffold(gen, lang, existsUnder(params.outDir), params.opts); err != nil {
		gen.Error(err)
	}
	return writeResponse(out, gen)
}

// existsUnder reports whether a response-relative path already exists on disk
// beneath dir (resolved against the working directory buf runs the plugin in,
// the same base buf resolves out: against).
func existsUnder(dir string) func(string) bool {
	return func(path string) bool {
		_, err := os.Stat(filepath.Join(dir, filepath.FromSlash(path)))
		return err == nil
	}
}
