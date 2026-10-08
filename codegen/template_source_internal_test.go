package codegen

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

// shaRepo makes a git repo with codegen/manifest.yaml = body, serving any
// commit by SHA, and returns its path and commit.
func shaRepo(t *testing.T, body string) (string, string) {
	t.Helper()
	repo := t.TempDir()
	run := func(args ...string) string {
		cmd := exec.Command("git", append([]string{"-C", repo, "-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null"}, args...)...)
		cmd.Env = gitEnv(os.Environ())
		out, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
		return strings.TrimSpace(string(out))
	}
	run("init", "-q")
	if err := os.MkdirAll(filepath.Join(repo, "codegen"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(repo, "codegen", ManifestFile), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	run("add", ".")
	run("commit", "-q", "-m", "set")
	run("config", "uploadpack.allowAnySHA1InWant", "true")
	return repo, run("rev-parse", "HEAD")
}

func TestFetch_EntryPublishedConcurrentlyIsKeptAndUsed(t *testing.T) {
	repo, commit := shaRepo(t, "fetched")
	cache := t.TempDir()
	src, err := ParseTemplateSource("file://" + repo + "@" + commit)
	if err != nil {
		t.Fatal(err)
	}
	entry := filepath.Join(cache, cacheKey(src.Repo, src.Rev))
	// Another run publishes the entry while this one is fetching; a file in
	// it stands for that run reading its templates.
	beforePublish = func() {
		if err := os.MkdirAll(filepath.Join(entry, "codegen"), 0o755); err != nil {
			t.Fatal(err)
		}
		for name, body := range map[string]string{fetchedMarker: commit + "\n", "codegen/" + ManifestFile: "other run"} {
			if err := os.WriteFile(filepath.Join(entry, name), []byte(body), 0o644); err != nil {
				t.Fatal(err)
			}
		}
	}
	t.Cleanup(func() { beforePublish = func() {} })
	dir, err := src.Resolve(cache)
	if err != nil {
		t.Fatalf("Resolve: %v", err)
	}
	if got, _ := os.ReadFile(filepath.Join(dir, ManifestFile)); string(got) != "other run" {
		t.Errorf("the concurrently published entry was replaced: manifest %q", got)
	}
	if entries, _ := os.ReadDir(cache); len(entries) != 1 {
		t.Errorf("cache holds %d entries, want 1 (the losing fetch is discarded)", len(entries))
	}
}

func TestFetch_IncompleteEntryIsReportedNotRemoved(t *testing.T) {
	repo, commit := shaRepo(t, "fetched")
	cache := t.TempDir()
	src, _ := ParseTemplateSource("file://" + repo + "@" + commit)
	entry := filepath.Join(cache, cacheKey(src.Repo, src.Rev))
	if err := os.MkdirAll(entry, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(entry, "stray"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := src.Resolve(cache); err == nil || !strings.Contains(err.Error(), "not a complete fetch") {
		t.Fatalf("want an incomplete-entry error, got %v", err)
	}
	if _, err := os.Stat(filepath.Join(entry, "stray")); err != nil {
		t.Errorf("the incomplete entry was touched: %v", err)
	}
}

func TestFetch_ConcurrentResolvesAllSucceed(t *testing.T) {
	repo, commit := shaRepo(t, "shared")
	cache := t.TempDir()
	src, _ := ParseTemplateSource("file://" + repo + "@" + commit)
	var wg sync.WaitGroup
	errs := make(chan error, 8)
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			dir, err := src.Resolve(cache)
			if err == nil {
				var raw []byte
				raw, err = os.ReadFile(filepath.Join(dir, ManifestFile))
				if err == nil && string(raw) != "shared" {
					err = os.ErrInvalid
				}
			}
			errs <- err
		}()
	}
	wg.Wait()
	close(errs)
	for err := range errs {
		if err != nil {
			t.Errorf("concurrent Resolve: %v", err)
		}
	}
}
