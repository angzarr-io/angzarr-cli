// Package cmd wires the angzarr CLI: cobra commands over viper config.
package cmd

import (
	"errors"
	"fmt"
	"io"
	"os"

	"github.com/spf13/cobra"
	"github.com/spf13/viper"
)

var cfgFile string

// configErr carries a hard failure from loading an explicit --config file.
// cobra.OnInitialize hooks (see initConfig) can't return an error directly,
// so initConfig stashes it here for rootCmd's PersistentPreRunE to surface
// before any subcommand's business logic runs.
var configErr error

// No subcommand reads a config key yet: codegen, scaffold and lint take all
// their input from flags, arguments and stdin. viper, --config and the
// ANGZARR_* env prefix are the config surface for commands that do; read
// values through viper and leave failure handling to loadConfig.
var rootCmd = &cobra.Command{
	Use:   "angzarr",
	Short: "Angzarr framework tooling",
	Long: `angzarr is the command-line tool for the Angzarr CQRS/ES framework.

Capabilities grow as subcommands; codegen (per-language dispatch wiring
from proto component declarations) is the first.`,
	SilenceUsage: true,
	Version:      version,
	PersistentPreRunE: func(cmd *cobra.Command, args []string) error {
		return configErr
	},
}

// Execute runs the CLI.
func Execute() {
	if err := rootCmd.Execute(); err != nil {
		os.Exit(1)
	}
}

func init() {
	cobra.OnInitialize(initConfig)
	rootCmd.PersistentFlags().StringVar(&cfgFile, "config", "", "config file (default $XDG_CONFIG_HOME/angzarr/config.yaml)")
}

// initConfig runs via cobra.OnInitialize: after flags are parsed, before
// any command's RunE. It loads config and records a hard failure (explicit
// --config that didn't load) in configErr.
func initConfig() {
	configErr = loadConfig(cfgFile, os.Stderr)
}

// loadConfig loads config into viper: explicit --config, else the user
// config dir, plus ANGZARR_* environment overrides.
//
// Failure handling distinguishes explicit from implicit:
//   - explicit (cfgFile != ""): any failure to load — missing file,
//     malformed YAML, whatever — is a hard error. The user pointed at a
//     specific file; silently proceeding as if it didn't exist would hide
//     a real mistake.
//   - implicit (auto-discovered): not-found is silent-OK, since most
//     invocations have no config file at all and that's not an error.
//     But a file that IS found and fails to parse is a real problem the
//     user should hear about even though the command still proceeds.
func loadConfig(cfgFile string, warn io.Writer) error {
	viper.Reset()
	explicit := cfgFile != ""
	if explicit {
		viper.SetConfigFile(cfgFile)
	} else {
		if dir, err := os.UserConfigDir(); err == nil {
			viper.AddConfigPath(dir + "/angzarr")
		}
		viper.SetConfigName("config")
		viper.SetConfigType("yaml")
	}
	viper.SetEnvPrefix("ANGZARR")
	viper.AutomaticEnv()

	err := viper.ReadInConfig()
	if err == nil {
		fmt.Fprintln(warn, "using config:", viper.ConfigFileUsed())
		return nil
	}
	if explicit {
		return fmt.Errorf("load config %q: %w", cfgFile, err)
	}
	var notFound viper.ConfigFileNotFoundError
	if errors.As(err, &notFound) {
		return nil
	}
	fmt.Fprintln(warn, "warning: config found but failed to parse:", err)
	return nil
}
