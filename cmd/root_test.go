package cmd

import (
	"bytes"
	"strings"
	"testing"
)

// TestRootCmd_VersionField_NonEmpty guards against a regression where
// rootCmd.Version is left unset: cobra only wires the --version flag when
// Version is non-empty, so an unset field silently disables the flag while
// `angzarr version` (the subcommand) keeps working — a confusing split.
func TestRootCmd_VersionField_NonEmpty(t *testing.T) {
	if rootCmd.Version == "" {
		t.Fatal("rootCmd.Version must be set (to the `version` build var) so cobra registers --version")
	}
	if rootCmd.Version != version {
		t.Fatalf("rootCmd.Version = %q, want it to track the `version` build var %q", rootCmd.Version, version)
	}
}

// TestRootCmd_DashDashVersionFlag_Works exercises the actual CLI surface:
// `angzarr --version` must succeed and report the same version string as
// the `angzarr version` subcommand, rather than erroring with "unknown flag".
func TestRootCmd_DashDashVersionFlag_Works(t *testing.T) {
	var out bytes.Buffer
	rootCmd.SetOut(&out)
	rootCmd.SetErr(&out)
	rootCmd.SetArgs([]string{"--version"})
	defer rootCmd.SetArgs(nil)

	if err := rootCmd.Execute(); err != nil {
		t.Fatalf("angzarr --version returned error: %v (output: %s)", err, out.String())
	}
	if !strings.Contains(out.String(), version) {
		t.Fatalf("--version output %q does not contain version %q", out.String(), version)
	}
}
