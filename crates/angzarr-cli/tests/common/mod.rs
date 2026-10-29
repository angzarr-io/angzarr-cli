//! What the end-to-end tests share: scaffolding with the CLI, laying the
//! user's files over the scaffold's, and running the application built.

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

/// The scaffolded application at `app`, its store in memory, run with
/// `cargo run` into the workspace's `target/e2e`.
pub fn run(app: &Path) -> Output {
    let config = std::fs::read_to_string(app.join("angzarr.toml"))
        .unwrap()
        .replace("path = \"angzarr.db\"", "in_memory = true");
    std::fs::write(app.join("angzarr.toml"), config).unwrap();
    Command::new(env!("CARGO"))
        .args(["run", "--quiet"])
        .current_dir(app)
        .env("CARGO_TARGET_DIR", here().join("../../target/e2e"))
        .output()
        .expect("cargo runs")
}
