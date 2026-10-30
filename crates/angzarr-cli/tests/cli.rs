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

#[test]
fn reasons_renders_the_catalog_into_the_file_named() {
    let catalog = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../angzarr-codegen/tests/fixtures/catalog/reasons.toml"
    );
    let dir = std::env::temp_dir().join(format!("angzarr-cli-reasons-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (lang, file, constant) in [
        ("rust", "reasons.rs", "pub const COMMAND_REFUSED: &str"),
        (
            "python",
            "reasons.py",
            "\nCOMMAND_REFUSED = \"COMMAND_REFUSED\"\n",
        ),
    ] {
        let path = dir.join(file);
        let out = angzarr(&["reasons", lang, catalog, "--out", path.to_str().unwrap()]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(value["path"], path.display().to_string());
        assert_eq!(value["keys"], 33);
        assert!(
            std::fs::read_to_string(&path).unwrap().contains(constant),
            "{lang}"
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
