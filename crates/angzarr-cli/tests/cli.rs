//! The binary end to end: tests run with stdout piped, which is exactly the
//! off-terminal case.

use std::process::{Command, Output};

fn angzarr(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_angzarr"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn version_off_a_terminal_is_json_by_default() {
    let out = angzarr(&["version"]);
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["name"], "angzarr");
    assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn an_explicit_format_wins_off_a_terminal() {
    let out = angzarr(&["version", "--format", "text"]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.starts_with("Name: angzarr\nVersion: "));
}

#[test]
fn the_format_flag_works_before_the_subcommand_too() {
    let out = angzarr(&["--format", "yaml", "version"]);
    assert!(out.status.success());
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .starts_with("name: angzarr\n")
    );
}

#[test]
fn an_unknown_format_fails_before_running_anything() {
    let out = angzarr(&["version", "--format", "xml"]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
}
