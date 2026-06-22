package cmd

import (
	"fmt"
	"io"
	"os"

	"github.com/spf13/cobra"
	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/descriptorpb"
	"google.golang.org/protobuf/types/pluginpb"

	"github.com/angzarr-io/angzarr-cli/codegen"
)

// lintCmd validates the angzarr component declarations a descriptor set carries
// BEFORE codegen runs. It reads a compiled descriptor set — a buf image
// (`buf build -o image.bin`, or `buf build -o -` piped in) or a protoc
// CodeGeneratorRequest — reports every problem at once with source locations,
// and exits non-zero when any error-severity diagnostic fires. Warnings are
// printed but do not fail the run.
//
// Codegen runs the same analysis internally and refuses to emit on errors;
// this command surfaces it standalone for CI, pre-commit, and `just`.
var lintCmd = &cobra.Command{
	Use:   "lint [image]",
	Short: "Validate proto component declarations for codegen consumption",
	Long: `Validate the (angzarr.v1.component / .command / .event) declarations in a
compiled descriptor set so codegen produces compile-safe, working wiring.

Input is a buf image or FileDescriptorSet from a file argument, or stdin
when omitted or "-":

  buf build proto -o - | angzarr lint
  buf build proto -o image.bin && angzarr lint image.bin

Errors (unresolved references, missing required fields, identifier
collisions) fail the run; warnings (uncovered emits, dangling domains,
orphan components) are reported but do not.`,
	Args: cobra.MaximumNArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		in := cmd.InOrStdin()
		if len(args) == 1 && args[0] != "-" {
			f, err := os.Open(args[0])
			if err != nil {
				return err
			}
			defer f.Close()
			in = f
		}
		asRequest, _ := cmd.Flags().GetBool("request")
		return runLint(in, cmd.OutOrStdout(), cmd.ErrOrStderr(), asRequest)
	},
}

func init() {
	lintCmd.Flags().Bool("request", false, "read a protoc CodeGeneratorRequest instead of a buf image / FileDescriptorSet")
	rootCmd.AddCommand(lintCmd)
}

// runLint reads a descriptor set, lints it, prints diagnostics to errOut, and
// returns a non-nil error when any diagnostic is error-severity.
func runLint(in io.Reader, out, errOut io.Writer, asRequest bool) error {
	raw, err := io.ReadAll(in)
	if err != nil {
		return fmt.Errorf("read descriptor set: %w", err)
	}
	gen, err := pluginFromDescriptors(raw, asRequest)
	if err != nil {
		return err
	}

	diags := codegen.Lint(gen)
	errs := 0
	for _, d := range diags {
		fmt.Fprintln(errOut, d.String())
		if d.Severity == codegen.SeverityError {
			errs++
		}
	}
	if errs > 0 {
		return fmt.Errorf("lint failed: %d error(s), %d warning(s)", errs, len(diags)-errs)
	}
	fmt.Fprintf(out, "lint OK: %d warning(s)\n", len(diags))
	return nil
}

// pluginFromDescriptors builds a protogen.Plugin from a buf image /
// FileDescriptorSet (the default) or a protoc CodeGeneratorRequest (asRequest).
// The two encodings overlap on the wire, so the form is chosen explicitly
// rather than guessed. A FileDescriptorSet carries no file_to_generate, so
// every file in it is marked for linting.
func pluginFromDescriptors(raw []byte, asRequest bool) (*protogen.Plugin, error) {
	if asRequest {
		req := &pluginpb.CodeGeneratorRequest{}
		if err := proto.Unmarshal(raw, req); err != nil {
			return nil, fmt.Errorf("parse CodeGeneratorRequest: %w", err)
		}
		return protogen.Options{}.New(req)
	}
	set := &descriptorpb.FileDescriptorSet{}
	if err := proto.Unmarshal(raw, set); err != nil {
		return nil, fmt.Errorf("parse FileDescriptorSet / buf image: %w", err)
	}
	req := &pluginpb.CodeGeneratorRequest{ProtoFile: set.File}
	for _, f := range set.File {
		req.FileToGenerate = append(req.FileToGenerate, f.GetName())
	}
	gen, err := protogen.Options{}.New(req)
	if err != nil {
		return nil, fmt.Errorf("build descriptor set (every proto needs option go_package; build the image with buf managed mode, or pass --request): %w", err)
	}
	return gen, nil
}
