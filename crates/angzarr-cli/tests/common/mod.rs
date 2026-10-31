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

/// The angzarr-embedded commit the applications are built on.
const ANGZARR_EMBEDDED_REV: &str = "c23457c14c857fc457324c563a12ca14fe4e7e79";
/// Where [`ANGZARR_EMBEDDED_REV`] is fetched from.
const ANGZARR_EMBEDDED_GIT: &str = "https://github.com/angzarr-io/angzarr-embedded";

/// The application `name`'s Cargo.toml, written into `app`: angzarr-embedded,
/// its markup, diagnostics and host at [`ANGZARR_EMBEDDED_REV`], prost,
/// tokio (the App is async, on the caller's runtime; already in the graph
/// through angzarr-embedded-host),
/// `dependencies` (TOML lines), and this workspace's angzarr-build for its
/// build script. A workspace of its own, wherever the temporary directory
/// is.
pub fn manifest(app: &Path, name: &str, dependencies: &str) {
    let embedded =
        format!("{{ git = \"{ANGZARR_EMBEDDED_GIT}\", rev = \"{ANGZARR_EMBEDDED_REV}\" }}");
    let build = here().join("../angzarr-build").display().to_string();
    let manifest = format!(
        "[package]\nname = {name:?}\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n\
         [dependencies]\n\
         angzarr-embedded = {embedded}\n\
         angzarr-embedded-diagnostic = {embedded}\n\
         angzarr-embedded-host = {embedded}\n\
         angzarr-embedded-macros = {embedded}\n\
         prost = \"0.14\"\n\
         tokio = {{ version = \"1\", features = [\"macros\", \"rt-multi-thread\"] }}\n\
         {dependencies}\n\
         [build-dependencies]\nangzarr-build = {{ path = {build:?} }}\n\n\
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
