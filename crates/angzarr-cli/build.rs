//! Stamps the commit the binary was built from (described), when built
//! from a git checkout that has one; the revision of angzarr-build the
//! workspace manifest pins (`ANGZARR_BUILD_REV`, which the manifests the
//! binary scaffolds name); and writes the crate's catalogs to OUT_DIR under
//! its package name.

use std::process::Command;

/// The `rev` of the workspace manifest's `angzarr-build` dependency.
fn angzarr_build_rev() -> String {
    let manifest = std::fs::read_to_string("../../Cargo.toml").expect("the workspace manifest");
    let line = manifest
        .lines()
        .find(|l| l.starts_with("angzarr-build = "))
        .expect("the workspace manifest pins angzarr-build");
    let rev = line
        .split("rev = \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("angzarr-build is pinned by rev");
    rev.to_string()
}

fn main() {
    angzarr_diagnostic::build::catalogs();
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    println!("cargo:rustc-env=ANGZARR_BUILD_REV={}", angzarr_build_rev());
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
}
