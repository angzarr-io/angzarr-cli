//! Stamps the commit the binary was built from, when built from a git
//! checkout that has one: `git rev-parse --short=12 HEAD`.
//!
//! The stamp names the commit only. Whether the tree had uncommitted edits
//! is not stamped: a build script that watches git's files does not rerun on
//! a source edit, so such a mark would go stale.

use std::path::PathBuf;
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Watch a path only when it exists: cargo reruns a script whose watched
/// path is missing on every build.
fn watch(path: PathBuf) {
    if path.exists() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // HEAD is per worktree; the refs it points at are shared by all of them.
    if let Some(git_dir) = git(&["rev-parse", "--path-format=absolute", "--git-dir"]) {
        watch(PathBuf::from(git_dir).join("HEAD"));
    }
    if let Some(common) = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"]) {
        let common = PathBuf::from(common);
        watch(common.join("refs"));
        watch(common.join("packed-refs"));
    }
    if let Some(commit) = git(&["rev-parse", "--short=12", "HEAD"]) {
        println!("cargo:rustc-env=ANGZARR_COMMIT={commit}");
    }
}
