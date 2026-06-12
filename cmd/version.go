package cmd

import (
	"fmt"

	"github.com/spf13/cobra"
)

// version is stamped at build time via
// -ldflags "-X github.com/angzarr-io/angzarr-cli/cmd.version=v…".
var version = "dev"

func init() {
	rootCmd.AddCommand(&cobra.Command{
		Use:   "version",
		Short: "Print the angzarr CLI version",
		Run: func(cmd *cobra.Command, _ []string) {
			fmt.Fprintln(cmd.OutOrStdout(), version)
		},
	})
}
