// Package cmd wires the angzarr CLI: cobra commands over viper config.
package cmd

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
	"github.com/spf13/viper"
)

var cfgFile string

var rootCmd = &cobra.Command{
	Use:   "angzarr",
	Short: "Angzarr framework tooling",
	Long: `angzarr is the command-line tool for the Angzarr CQRS/ES framework.

Capabilities grow as subcommands; codegen (per-language dispatch wiring
from proto component declarations) is the first.`,
	SilenceUsage: true,
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

// initConfig loads viper config: explicit --config, else the user config
// dir, plus ANGZARR_* environment overrides.
func initConfig() {
	if cfgFile != "" {
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
	if err := viper.ReadInConfig(); err == nil {
		fmt.Fprintln(os.Stderr, "using config:", viper.ConfigFileUsed())
	}
}
