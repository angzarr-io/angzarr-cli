//! What the end-to-end tests share: scaffolding with the CLI, laying the
//! user's files over the scaffold's, the application's manifest, and
//! running the application built. Each test binary uses its own part.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The angzarr-cli crate's directory.
pub fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// `angzarr <args>` in `dir`, which must succeed.
pub fn angzarr(dir: &Path, args: &[&str]) {
    let out = Command::new(env!("CARGO_BIN_EXE_angzarr"))
        .args(args)
        .args(["--format", "text"])
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "angzarr {args:?}:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The user's files over the scaffold's: every file under `from`, at the
/// same path under `to`.
pub fn overlay(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            overlay(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

// The angzarr-embedded commit the applications are built on: the one the
// generator writes against.
use angzarr_codegen::{ANGZARR_EMBEDDED_GIT, ANGZARR_EMBEDDED_REV};

/// The manifest value of the angzarr-build dependency: the repository at
/// the revision the CLI is built against.
pub const ANGZARR_BUILD_DEPENDENCY: &str = concat!(
    "{ git = \"https://github.com/angzarr-io/angzarr-build\", rev = \"",
    env!("ANGZARR_BUILD_REV"),
    "\" }"
);

/// The application `name`'s Cargo.toml, written into `app`: angzarr-embedded,
/// its markup and host at [`ANGZARR_EMBEDDED_REV`] (the kernel re-exports
/// the diagnostics), prost,
/// tokio (the App is async, on the caller's runtime; already in the graph
/// through angzarr-embedded-host),
/// `dependencies` (TOML lines), and angzarr-build, at the revision the
/// workspace pins, for its build script. A workspace of its own, wherever the temporary directory
/// is.
pub fn manifest(app: &Path, name: &str, dependencies: &str) {
    let embedded =
        format!("{{ git = \"{ANGZARR_EMBEDDED_GIT}\", rev = \"{ANGZARR_EMBEDDED_REV}\" }}");
    let build = ANGZARR_BUILD_DEPENDENCY;
    let manifest = format!(
        "[package]\nname = {name:?}\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n\
         [dependencies]\n\
         angzarr-embedded = {embedded}\n\
         angzarr-embedded-host = {embedded}\n\
         angzarr-embedded-macros = {embedded}\n\
         prost = \"0.14\"\n\
         tokio = {{ version = \"1\", features = [\"macros\", \"rt-multi-thread\"] }}\n\
         {dependencies}\n\
         [build-dependencies]\nangzarr-build = {build}\n\n\
         [workspace]\n"
    );
    std::fs::write(app.join("Cargo.toml"), manifest).unwrap();
}

/// The application at `app` run with `cargo run` into the workspace's
/// `target/e2e`.
pub fn run(app: &Path) -> Output {
    Command::new(env!("CARGO"))
        .args(["run", "--quiet"])
        .current_dir(app)
        .env("CARGO_TARGET_DIR", here().join("../../target/e2e"))
        .output()
        .expect("cargo runs")
}
