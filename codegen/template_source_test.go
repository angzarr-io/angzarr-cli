package codegen_test

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/angzarr-io/angzarr-cli/codegen"
)

// Full commit SHAs (SHA-1 and SHA-256) for template source specs.
const (
	sha1   = "5147f50bdc9aee5607e33e1b35a9a303b410df19"
	sha256 = "5147f50bdc9aee5607e33e1b35a9a303b410df195147f50bdc9aee5607e33e1b"
)

func TestParseTemplateSource(t *testing.T) {
	for spec, want := range map[string]codegen.TemplateSource{
		"github.com/angzarr-io/angzarr-client-python@" + sha1: {Repo: "github.com/angzarr-io/angzarr-client-python", URL: "https://github.com/angzarr-io/angzarr-client-python.git", Rev: sha1},
		"github.com/org/repo.git@" + sha256:                   {Repo: "github.com/org/repo.git", URL: "https://github.com/org/repo.git", Rev: sha256},
		"file:///srv/repo@" + sha1:                            {Repo: "file:///srv/repo", URL: "file:///srv/repo", Rev: sha1},
		"../client-python":                                    {Local: "../client-python"},
		"/abs/dir@with-at":                                    {Local: "/abs/dir@with-at"},
		"codegen":                                             {Local: "codegen"},
		"~/x":                                                 {Local: "~/x"},
	} {
		got, err := codegen.ParseTemplateSource(spec)
		if err != nil {
			t.Errorf("%s: %v", spec, err)
			continue
		}
		if got != want {
			t.Errorf("%s = %+v, want %+v", spec, got, want)
		}
	}
	for spec, want := range map[string]string{
		"":                                  "empty",
		"github.com/org/repo@":              "<repository>@<revision>",
		"@abc":                              "<repository>@<revision>",
		"https://github.com/o/r@" + sha1:    "host/path",
		"repo@" + sha1:                      "not host/path",
		"localhost/repo@" + sha1:            "not host/path",
		"github.com/org/repo@v1.2.3":        "not a full commit SHA",
		"github.com/org/repo@main":          "not a full commit SHA",
		"github.com/org/repo@5147f50":       "not a full commit SHA",
		"github.com/org/repo@" + sha1[1:]:   "not a full commit SHA",
		"github.com/org/repo@" + sha1 + "0": "not a full commit SHA",
		"github.com/org/repo@" + strings.ToUpper(sha1): "not a full commit SHA",
		"file:///srv/repo@v1":                          "not a full commit SHA",
	} {
		if _, err := codegen.ParseTemplateSource(spec); err == nil || !strings.Contains(err.Error(), want) {
			t.Errorf("%q: want error containing %q, got %v", spec, want, err)
		}
	}
}

func TestResolve_LocalFindsManifestAtRootOrInCodegen(t *testing.T) {
	root := t.TempDir()
	if err := os.MkdirAll(filepath.Join(root, "codegen"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "codegen", "manifest.yaml"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	dir, err := codegen.TemplateSource{Local: root}.Resolve("")
	if err != nil || dir != filepath.Join(root, "codegen") {
		t.Fatalf("repo root resolves to its codegen/ set: %q, %v", dir, err)
	}
	dir, err = codegen.TemplateSource{Local: filepath.Join(root, "codegen")}.Resolve("")
	if err != nil || dir != filepath.Join(root, "codegen") {
		t.Fatalf("the set directory resolves to itself: %q, %v", dir, err)
	}
	if _, err := (codegen.TemplateSource{Local: t.TempDir()}).Resolve(""); err == nil || !strings.Contains(err.Error(), "no manifest.yaml") {
		t.Fatalf("a directory without a manifest is refused, got %v", err)
	}
}

func git(t *testing.T, dir string, args ...string) string {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir, "-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null"}, args...)...)
	// Run against dir alone, even when the test itself runs inside a git hook
	// that exported GIT_DIR / GIT_INDEX_FILE for the enclosing repository.
	cmd.Env = cleanGitEnv()
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
	return strings.TrimSpace(string(out))
}

func cleanGitEnv() []string {
	var env []string
	for _, kv := range os.Environ() {
		if !strings.HasPrefix(kv, "GIT_") {
			env = append(env, kv)
		}
	}
	return env
}

// templateRepo makes a git repo whose codegen/manifest.yaml content is body,
// serving any commit by SHA, and returns its path and commit.
func templateRepo(t *testing.T, body string) (string, string) {
	t.Helper()
	repo := t.TempDir()
	git(t, repo, "init", "-q")
	if err := os.MkdirAll(filepath.Join(repo, "codegen"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(repo, "codegen", "manifest.yaml"), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	git(t, repo, "add", ".")
	git(t, repo, "commit", "-q", "-m", "templates")
	git(t, repo, "config", "uploadpack.allowAnySHA1InWant", "true")
	return repo, git(t, repo, "rev-parse", "HEAD")
}

func TestResolve_GitFetchesOnceIntoTheCache(t *testing.T) {
	repo, commit := templateRepo(t, "first")
	cache := t.TempDir()
	src, err := codegen.ParseTemplateSource("file://" + repo + "@" + commit)
	if err != nil {
		t.Fatal(err)
	}
	dir, err := src.Resolve(cache)
	if err != nil {
		t.Fatalf("Resolve: %v", err)
	}
	if got, _ := os.ReadFile(filepath.Join(dir, "manifest.yaml")); string(got) != "first" {
		t.Fatalf("fetched manifest = %q", got)
	}
	if !strings.HasPrefix(dir, cache+string(filepath.Separator)) || filepath.Base(dir) != "codegen" {
		t.Errorf("fetched set %q is not a cache entry's codegen/", dir)
	}
	marker, err := os.ReadFile(filepath.Join(filepath.Dir(dir), ".angzarr-template-commit"))
	if err != nil || strings.TrimSpace(string(marker)) != commit {
		t.Errorf("cache marker = %q (%v), want %s", marker, err, commit)
	}
	if _, err := os.Stat(filepath.Join(filepath.Dir(dir), ".git")); !os.IsNotExist(err) {
		t.Errorf("cache entry keeps no .git directory (stat err %v)", err)
	}

	// A cached entry is reused without fetching: the origin is gone.
	if err := os.RemoveAll(repo); err != nil {
		t.Fatal(err)
	}
	again, err := src.Resolve(cache)
	if err != nil || again != dir {
		t.Fatalf("cached Resolve = %q, %v; want %q", again, err, dir)
	}
	entries, _ := os.ReadDir(cache)
	if len(entries) != 1 {
		t.Errorf("cache holds %d entries, want 1 (no leftover fetch dirs)", len(entries))
	}
}

func TestResolve_GitByCommitAndUnknownCommit(t *testing.T) {
	repo, commit := templateRepo(t, "pinned")
	cache := t.TempDir()
	src, _ := codegen.ParseTemplateSource("file://" + repo + "@" + commit)
	dir, err := src.Resolve(cache)
	if err != nil {
		t.Fatalf("Resolve by commit: %v", err)
	}
	if got, _ := os.ReadFile(filepath.Join(dir, "manifest.yaml")); string(got) != "pinned" {
		t.Errorf("manifest = %q", got)
	}
	const unknown = "0000000000000000000000000000000000000000"
	bad, err := codegen.ParseTemplateSource("file://" + repo + "@" + unknown)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := bad.Resolve(cache); err == nil || !strings.Contains(err.Error(), "fetch templates") {
		t.Fatalf("an unknown commit must fail the fetch, got %v", err)
	}
	if _, err := os.Stat(filepath.Join(cache, filepath.Base(repo)+"@"+unknown)); !os.IsNotExist(err) {
		t.Errorf("a failed fetch leaves no cache entry")
	}
}

func TestTemplateCacheDir_HonoursTheEnvironment(t *testing.T) {
	t.Setenv(codegen.TemplateCacheEnv, "/custom/cache")
	if dir, err := codegen.TemplateCacheDir(); err != nil || dir != "/custom/cache" {
		t.Fatalf("TemplateCacheDir = %q, %v", dir, err)
	}
	t.Setenv(codegen.TemplateCacheEnv, "")
	t.Setenv("XDG_CACHE_HOME", "/xdg")
	if dir, err := codegen.TemplateCacheDir(); err != nil || dir != "/xdg/angzarr/templates" {
		t.Fatalf("TemplateCacheDir = %q, %v", dir, err)
	}
}

func TestGenerate_TemplateOptionResolvesAGitSource(t *testing.T) {
	// The fixture set, committed to a repo, renders through templates=file://…@<commit>.
	repo := t.TempDir()
	git(t, repo, "init", "-q")
	src, err := filepath.Abs(tmplSet)
	if err != nil {
		t.Fatal(err)
	}
	dst := filepath.Join(repo, "codegen")
	if err := os.MkdirAll(dst, 0o755); err != nil {
		t.Fatal(err)
	}
	files, _ := filepath.Glob(filepath.Join(src, "*"))
	for _, f := range files {
		raw, _ := os.ReadFile(f)
		if err := os.WriteFile(filepath.Join(dst, filepath.Base(f)), raw, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	git(t, repo, "add", ".")
	git(t, repo, "commit", "-q", "-m", "set")
	git(t, repo, "config", "uploadpack.allowAnySHA1InWant", "true")
	commit := git(t, repo, "rev-parse", "HEAD")
	t.Setenv(codegen.TemplateCacheEnv, t.TempDir())

	o := buildOptionTypes(t, ioPkg)
	gen, err := buildGen(t, ioPkg, orderAggregate(o)...)
	if err != nil {
		t.Fatal(err)
	}
	if err := codegen.Generate(gen, "testlang", codegen.Options{Templates: "file://" + repo + "@" + commit}); err != nil {
		t.Fatalf("Generate: %v", err)
	}
	if n := len(gen.Response().File); n != 2 {
		t.Fatalf("want the two codegen outputs, got %d", n)
	}
}

func TestResolve_GitIgnoresAnInheritedRepository(t *testing.T) {
	// A hook-run CLI inherits GIT_DIR / GIT_INDEX_FILE / GIT_WORK_TREE for the
	// caller's repository; fetching templates must leave that repository alone.
	repo, commit := templateRepo(t, "fetched")
	decoy, decoyHead := templateRepo(t, "decoy")
	t.Setenv("GIT_DIR", filepath.Join(decoy, ".git"))
	t.Setenv("GIT_INDEX_FILE", filepath.Join(decoy, ".git", "index"))
	t.Setenv("GIT_WORK_TREE", decoy)
	src, _ := codegen.ParseTemplateSource("file://" + repo + "@" + commit)
	dir, err := src.Resolve(t.TempDir())
	if err != nil {
		t.Fatalf("Resolve: %v", err)
	}
	if got, _ := os.ReadFile(filepath.Join(dir, "manifest.yaml")); string(got) != "fetched" {
		t.Errorf("fetched manifest = %q, want the source repo's", got)
	}
	if head := git(t, decoy, "rev-parse", "HEAD"); head != decoyHead {
		t.Errorf("the inherited repository moved: HEAD %s, was %s", head, decoyHead)
	}
	if status := git(t, decoy, "status", "--porcelain"); status != "" {
		t.Errorf("the inherited repository's index/work tree changed:\n%s", status)
	}
}

func TestResolve_LocalExpandsHome(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	if err := os.MkdirAll(filepath.Join(home, "set"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(home, "set", "manifest.yaml"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	dir, err := codegen.TemplateSource{Local: "~/set"}.Resolve("")
	if err != nil || dir != filepath.Join(home, "set") {
		t.Fatalf("~/set resolves under $HOME: %q, %v", dir, err)
	}
}
