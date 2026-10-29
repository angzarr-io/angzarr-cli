//! Stamps the commit the binary was built from (described, and in full),
//! when built from a git checkout that has one.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs");
    let commit = Command::new("git")
        .args(["describe", "--always", "--dirty", "--abbrev=12"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(commit) = commit {
        println!("cargo:rustc-env=ANGZARR_COMMIT={commit}");
    }
    // The full commit, which a generated application's angzarr-build
    // dependency is pinned to.
    let rev = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(rev) = rev {
        println!("cargo:rustc-env=ANGZARR_REV={rev}");
    }
}
