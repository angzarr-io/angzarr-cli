package cmd

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// resetConfigState clears the package-level config state that persists
// across rootCmd.Execute() calls (cobra/pflag do not reset unspecified
// flags between Parse() calls on a shared FlagSet), so each test case
// starts from a clean slate regardless of run order.
func resetConfigState(t *testing.T) {
	t.Helper()
	cfgFile = ""
	configErr = nil
}

// TestLoadConfig_ExplicitNonexistent_HardErrors: a user who typed --config <path> and got the path wrong deserves a
// hard failure, not a silent fall-through as if no config were requested.
func TestLoadConfig_ExplicitNonexistent_HardErrors(t *testing.T) {
	var warn bytes.Buffer
	err := loadConfig(filepath.Join(t.TempDir(), "does-not-exist.yaml"), &warn)
	if err == nil {
		t.Fatal("loadConfig with explicit nonexistent path: got nil error, want hard error")
	}
}

// TestLoadConfig_ExplicitMalformed_HardErrors covers the other explicit
// failure mode: the file exists but doesn't parse. Same treatment as
// not-found — the user pointed at a specific file, and it's broken.
func TestLoadConfig_ExplicitMalformed_HardErrors(t *testing.T) {
	dir := t.TempDir()
	bad := filepath.Join(dir, "bad.yaml")
	if err := os.WriteFile(bad, []byte("key: [unterminated"), 0o644); err != nil {
		t.Fatalf("write malformed config: %v", err)
	}

	var warn bytes.Buffer
	err := loadConfig(bad, &warn)
	if err == nil {
		t.Fatal("loadConfig with explicit malformed file: got nil error, want hard error")
	}
}

// TestLoadConfig_ImplicitNotFound_RunsClean is the common case: most
// invocations have no config file at all, and that must stay silent-OK —
// auto-discovery finding nothing is not a user error.
func TestLoadConfig_ImplicitNotFound_RunsClean(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir()) // empty dir: no angzarr/config.yaml

	var warn bytes.Buffer
	if err := loadConfig("", &warn); err != nil {
		t.Fatalf("loadConfig with implicit not-found: got err = %v, want nil", err)
	}
	if warn.Len() != 0 {
		t.Errorf("loadConfig with implicit not-found: unexpected output %q, want silent", warn.String())
	}
}

// TestLoadConfig_ImplicitMalformed_WarnsButSucceeds: an auto-discovered
// config that IS present but fails to parse must be reported, not ignored.
// It also must not hard-fail the run, since the user never asked for this
// particular file.
func TestLoadConfig_ImplicitMalformed_WarnsButSucceeds(t *testing.T) {
	xdg := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", xdg)
	dir := filepath.Join(xdg, "angzarr")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "config.yaml"), []byte("key: [unterminated"), 0o644); err != nil {
		t.Fatalf("write malformed config: %v", err)
	}

	var warn bytes.Buffer
	if err := loadConfig("", &warn); err != nil {
		t.Fatalf("loadConfig with implicit malformed config: got err = %v, want nil (should warn, not fail)", err)
	}
	if warn.Len() == 0 {
		t.Error("loadConfig with implicit malformed config: got no warning, want a warning about the parse failure")
	}
}

// TestExecute_ExplicitConfigNonexistent_HardErrors exercises the full
// cobra wiring (OnInitialize -> configErr -> PersistentPreRunE), confirming
// an explicit --config failure aborts the run before any subcommand logic
// executes and rootCmd.Execute() returns a non-nil error (Execute() maps
// this to os.Exit(1)).
func TestExecute_ExplicitConfigNonexistent_HardErrors(t *testing.T) {
	resetConfigState(t)
	rootCmd.SetArgs([]string{"--config", filepath.Join(t.TempDir(), "typo.yaml"), "version"})
	if err := rootCmd.Execute(); err == nil {
		t.Fatal("rootCmd.Execute() with explicit nonexistent --config: got nil error, want hard error")
	}
}

// TestExecute_ExplicitConfigMalformed_HardErrors is the malformed-file
// counterpart, driven the same way.
func TestExecute_ExplicitConfigMalformed_HardErrors(t *testing.T) {
	resetConfigState(t)
	dir := t.TempDir()
	bad := filepath.Join(dir, "bad.yaml")
	if err := os.WriteFile(bad, []byte("key: [unterminated"), 0o644); err != nil {
		t.Fatalf("write malformed config: %v", err)
	}

	rootCmd.SetArgs([]string{"--config", bad, "version"})
	if err := rootCmd.Execute(); err == nil {
		t.Fatal("rootCmd.Execute() with explicit malformed --config: got nil error, want hard error")
	}
}

// TestExecute_ImplicitConfigNotFound_RunsClean is the regression guard: the
// overwhelmingly common invocation (no --config, no config file present)
// must keep working exactly as before.
func TestExecute_ImplicitConfigNotFound_RunsClean(t *testing.T) {
	resetConfigState(t)
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	rootCmd.SetArgs([]string{"version"})
	var out bytes.Buffer
	rootCmd.SetOut(&out)
	if err := rootCmd.Execute(); err != nil {
		t.Fatalf("rootCmd.Execute() with implicit not-found config: got err = %v, want nil", err)
	}
	if !strings.Contains(out.String(), "dev") {
		t.Errorf("version output = %q, want it to contain the version string", out.String())
	}
}
