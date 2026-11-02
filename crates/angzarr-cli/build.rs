//! Stamps the commit the binary was built from (described), when built
//! from a git checkout that has one; and writes the crate's catalogs to
//! OUT_DIR under its package name.

use std::process::Command;

fn main() {
    angzarr_diagnostic::build::catalogs();
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
