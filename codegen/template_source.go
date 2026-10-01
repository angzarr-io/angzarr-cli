package codegen

// Template sources: where a template set comes from. A source is either a
// local directory (development) or a git repository at a pinned revision,
// fetched once into a cache keyed by repository and revision.

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
)

// TemplateDirName is the directory inside a client repository that holds its
// template set when the manifest is not at the repository root.
const TemplateDirName = "codegen"

// TemplateCacheEnv overrides the template cache directory.
const TemplateCacheEnv = "ANGZARR_TEMPLATE_CACHE"

// fetchedMarker records, inside a cache entry, the commit the entry holds. An
// entry without it is incomplete and is fetched again.
const fetchedMarker = ".angzarr-template-commit"

// TemplateSource is a parsed templates= plugin option.
type TemplateSource struct {
	// Local is a directory on disk; empty for a git source.
	Local string
	// Repo is the repository as written (github.com/org/repo, or a file://
	// URL); URL is what git fetches; Rev is the commit, tag or branch.
	Repo, URL, Rev string
}

// ParseTemplateSource parses a templates= value. A value containing "@" whose
// repository part is a host path (github.com/org/repo) or a file:// URL is a
// git source; anything else is a local directory.
func ParseTemplateSource(spec string) (TemplateSource, error) {
	if spec == "" {
		return TemplateSource{}, errors.New("empty templates= value")
	}
	at := strings.LastIndex(spec, "@")
	looksLocal := strings.HasPrefix(spec, "/") || strings.HasPrefix(spec, ".") || strings.HasPrefix(spec, "~")
	if at < 0 || looksLocal {
		return TemplateSource{Local: spec}, nil
	}
	repo, rev := spec[:at], spec[at+1:]
	if repo == "" || rev == "" {
		return TemplateSource{}, fmt.Errorf("templates=%s: want <repository>@<revision>", spec)
	}
	if strings.HasPrefix(repo, "file://") {
		return TemplateSource{Repo: repo, URL: repo, Rev: rev}, nil
	}
	if strings.Contains(repo, "://") {
		return TemplateSource{}, fmt.Errorf("templates=%s: write the repository as host/path (github.com/org/repo) or a file:// URL", spec)
	}
	host, _, ok := strings.Cut(repo, "/")
	if !ok || !strings.Contains(host, ".") {
		return TemplateSource{}, fmt.Errorf("templates=%s: repository %q is not host/path (github.com/org/repo)", spec, repo)
	}
	return TemplateSource{Repo: repo, URL: "https://" + strings.TrimSuffix(repo, ".git") + ".git", Rev: rev}, nil
}

// TemplateCacheDir is the cache root: $ANGZARR_TEMPLATE_CACHE, else
// <user cache dir>/angzarr/templates.
func TemplateCacheDir() (string, error) {
	if dir := os.Getenv(TemplateCacheEnv); dir != "" {
		return dir, nil
	}
	base, err := os.UserCacheDir()
	if err != nil {
		return "", fmt.Errorf("no template cache directory (set %s): %w", TemplateCacheEnv, err)
	}
	return filepath.Join(base, "angzarr", "templates"), nil
}

var unsafeKeyChars = regexp.MustCompile(`[^A-Za-z0-9._-]+`)

// cacheKey is the cache entry name for a repository + revision.
func cacheKey(repo, rev string) string {
	clean := func(s string) string { return strings.Trim(unsafeKeyChars.ReplaceAllString(s, "_"), "_") }
	return clean(strings.TrimPrefix(repo, "file://")) + "@" + clean(rev)
}

// Resolve returns the directory holding the source's manifest.yaml, fetching
// a git source into cacheRoot first when it is not cached. A cached entry is
// reused as is: pin commits or immutable tags, not branches.
func (s TemplateSource) Resolve(cacheRoot string) (string, error) {
	root := s.Local
	if root == "" {
		var err error
		root, err = s.fetch(cacheRoot)
		if err != nil {
			return "", err
		}
	} else if strings.HasPrefix(root, "~/") {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", err
		}
		root = filepath.Join(home, root[2:])
	}
	for _, dir := range []string{root, filepath.Join(root, TemplateDirName)} {
		if _, err := os.Stat(filepath.Join(dir, ManifestFile)); err == nil {
			return dir, nil
		}
	}
	return "", fmt.Errorf("no %s in %s or %s/%s", ManifestFile, root, root, TemplateDirName)
}

func (s TemplateSource) fetch(cacheRoot string) (string, error) {
	entry := filepath.Join(cacheRoot, cacheKey(s.Repo, s.Rev))
	if _, err := os.Stat(filepath.Join(entry, fetchedMarker)); err == nil {
		return entry, nil
	}
	if err := os.MkdirAll(cacheRoot, 0o755); err != nil {
		return "", err
	}
	tmp, err := os.MkdirTemp(cacheRoot, ".fetch-")
	if err != nil {
		return "", err
	}
	defer os.RemoveAll(tmp)
	git := func(args ...string) (string, error) {
		cmd := exec.Command("git", append([]string{"-C", tmp}, args...)...)
		cmd.Env = append(gitEnv(os.Environ()), "GIT_TERMINAL_PROMPT=0")
		out, err := cmd.CombinedOutput()
		if err != nil {
			return "", fmt.Errorf("git %s: %w: %s", strings.Join(args, " "), err, strings.TrimSpace(string(out)))
		}
		return strings.TrimSpace(string(out)), nil
	}
	if _, err := git("init", "-q"); err != nil {
		return "", err
	}
	if _, err := git("fetch", "-q", "--depth", "1", s.URL, s.Rev); err != nil {
		return "", fmt.Errorf("fetch templates %s@%s: %w", s.Repo, s.Rev, err)
	}
	if _, err := git("checkout", "-q", "FETCH_HEAD"); err != nil {
		return "", err
	}
	commit, err := git("rev-parse", "HEAD")
	if err != nil {
		return "", err
	}
	if err := os.RemoveAll(filepath.Join(tmp, ".git")); err != nil {
		return "", err
	}
	if err := os.WriteFile(filepath.Join(tmp, fetchedMarker), []byte(commit+"\n"), 0o644); err != nil {
		return "", err
	}
	_ = os.RemoveAll(entry) // an incomplete entry from an interrupted fetch
	if err := os.Rename(tmp, entry); err != nil {
		// A concurrent fetch of the same entry finished first.
		if _, statErr := os.Stat(filepath.Join(entry, fetchedMarker)); statErr == nil {
			return entry, nil
		}
		return "", err
	}
	return entry, nil
}

// repoLocatingGitVars are the variables that point git at a repository
// (`git rev-parse --local-env-vars`). git exports several of them to hooks, so
// a CLI run from a hook inherits the caller's repository; the fetch must not.
var repoLocatingGitVars = map[string]bool{
	"GIT_ALTERNATE_OBJECT_DIRECTORIES": true, "GIT_CONFIG": true, "GIT_CONFIG_PARAMETERS": true,
	"GIT_CONFIG_COUNT": true, "GIT_OBJECT_DIRECTORY": true, "GIT_DIR": true, "GIT_WORK_TREE": true,
	"GIT_IMPLICIT_WORK_TREE": true, "GIT_GRAFT_FILE": true, "GIT_INDEX_FILE": true,
	"GIT_NO_REPLACE_OBJECTS": true, "GIT_REPLACE_REF_BASE": true, "GIT_PREFIX": true,
	"GIT_SHALLOW_FILE": true, "GIT_COMMON_DIR": true,
}

// gitEnv is env without the repository-locating git variables, so git
// commands act on the fetch directory alone.
func gitEnv(env []string) []string {
	out := make([]string, 0, len(env))
	for _, kv := range env {
		name, _, _ := strings.Cut(kv, "=")
		if !repoLocatingGitVars[name] {
			out = append(out, kv)
		}
	}
	return out
}
