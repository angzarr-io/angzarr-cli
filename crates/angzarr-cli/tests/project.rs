//! `angzarr init` and the project's output default, end to end, each in a
//! fresh directory under the target's temporary directory. Stdout is piped,
//! so the off-terminal default applies.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const SHA: &str = "b1f00d2710eb7bee60308f6df520893d782012a6";

/// An empty directory of its own for each test.
fn fresh(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("cli-project")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn angzarr_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_angzarr"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or_else(|e| {
        panic!("{e}: {}", String::from_utf8_lossy(bytes));
    })
}

fn write_config(root: &Path, text: &str) {
    std::fs::create_dir_all(root.join(".angzarr")).unwrap();
    std::fs::write(root.join(".angzarr").join("config.yaml"), text).unwrap();
}

#[test]
fn init_writes_the_project_directory_and_reports_it() {
    let root = fresh("init");
    let out = angzarr_in(
        &root,
        &[
            "init",
            "--target",
            "rust",
            "--target",
            "python",
            "--spec-commit",
            SHA,
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let value = json(&out.stdout);
    assert_eq!(value["dry_run"], false);
    assert_eq!(
        value["files"],
        serde_json::json!([
            ".angzarr/config.yaml",
            ".angzarr/lock.yaml",
            ".angzarr/.gitignore"
        ])
    );
    assert_eq!(value["config"]["targets"]["rust"]["out"], "gen/rust");
    assert_eq!(value["lock"]["spec"]["commit"], SHA);

    let dir = root.join(".angzarr");
    assert_eq!(
        std::fs::read_to_string(dir.join("config.yaml")).unwrap(),
        "version: 1\nprotos: proto\ntargets:\n  rust:\n    out: gen/rust\n  python:\n    out: gen/python\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("lock.yaml")).unwrap(),
        format!("version: 1\nspec:\n  commit: {SHA}\ntargets: {{}}\n")
    );
    assert_eq!(
        std::fs::read_to_string(dir.join(".gitignore")).unwrap(),
        "cache/\n"
    );
}

#[test]
fn a_second_init_is_refused_with_a_remedy_and_changes_nothing() {
    let root = fresh("init-twice");
    let first = angzarr_in(&root, &["init", "--spec-commit", SHA]);
    assert_eq!(first.status.code(), Some(0), "{first:?}");
    let config = std::fs::read_to_string(root.join(".angzarr/config.yaml")).unwrap();

    let out = angzarr_in(&root, &["init", "--target", "rust", "--spec-commit", SHA]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(out.stdout.is_empty());
    let value = json(&out.stderr);
    assert_eq!(value["error"]["code"], "angzarr-cli-app-project-exists");
    assert_eq!(
        value["remedy"]["code"],
        "angzarr-cli-app-fix-project-exists"
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".angzarr/config.yaml")).unwrap(),
        config
    );
}

#[test]
fn a_dry_run_reports_what_it_would_write_and_writes_nothing() {
    let root = fresh("init-dry");
    let out = angzarr_in(
        &root,
        &[
            "init",
            "--target",
            "rust",
            "--spec-commit",
            SHA,
            "--dry-run",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let value = json(&out.stdout);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["config"]["targets"]["rust"]["out"], "gen/rust");
    assert!(!root.join(".angzarr").exists());
}

#[test]
fn init_refuses_a_bad_commit_or_target_and_writes_nothing() {
    let root = fresh("init-bad");
    let out = angzarr_in(&root, &["init", "--spec-commit", "b1f00d2"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let value = json(&out.stderr);
    assert_eq!(
        value["error"]["code"],
        "angzarr-cli-core-project-bad-commit"
    );
    assert_eq!(value["error"]["args"]["source"], "--spec-commit");
    assert_eq!(value["remedy"]["code"], "angzarr-cli-core-fix-commit");

    let out = angzarr_in(&root, &["init", "--target", "go", "--spec-commit", SHA]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let value = json(&out.stderr);
    assert_eq!(
        value["error"]["code"],
        "angzarr-cli-core-project-unknown-target"
    );
    assert_eq!(value["error"]["args"]["target"], "go");
    assert!(!root.join(".angzarr").exists());

    let out = angzarr_in(&root, &["init"]);
    assert!(!out.status.success());
    assert!(!root.join(".angzarr").exists());
}

#[test]
fn off_a_terminal_the_project_default_format_applies_from_any_subdirectory() {
    let root = fresh("format-config");
    write_config(
        &root,
        "version: 1\nprotos: proto\noutput:\n  non_tty_format: yaml\n",
    );
    let deep = root.join("src").join("deep");
    std::fs::create_dir_all(&deep).unwrap();
    for dir in [&root, &deep] {
        let out = angzarr_in(dir, &["version"]);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(text.starts_with("name: angzarr\n"), "{text}");
    }
}

#[test]
fn an_explicit_format_beats_the_project_default() {
    let root = fresh("format-explicit");
    write_config(
        &root,
        "version: 1\nprotos: proto\noutput:\n  non_tty_format: yaml\n",
    );
    let out = angzarr_in(&root, &["version", "--format", "json"]);
    assert_eq!(json(&out.stdout)["name"], "angzarr");
}

#[test]
fn a_project_without_a_default_is_json_off_a_terminal() {
    let root = fresh("format-unset");
    write_config(&root, "version: 1\nprotos: proto\n");
    let out = angzarr_in(&root, &["version"]);
    assert_eq!(json(&out.stdout)["name"], "angzarr");
}

#[test]
fn an_invalid_project_config_is_a_warning_and_json_is_used() {
    let root = fresh("format-invalid");
    write_config(
        &root,
        "version: 1\nprotos: proto\noutput:\n  non_tty_format: xml\n",
    );
    let out = angzarr_in(&root, &["version"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(json(&out.stdout)["name"], "angzarr");
    let warning = json(&out.stderr);
    assert_eq!(
        warning["warning"]["code"],
        "angzarr-cli-core-project-bad-format"
    );
}

#[test]
fn the_format_help_names_where_the_off_terminal_default_comes_from() {
    let out = angzarr_in(&fresh("help"), &["--help"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("output.non_tty_format in the nearest .angzarr/config.yaml"),
        "{text}"
    );
}
