//! What `angzarr` prints for an error, byte for byte, in both formats: each
//! error path's stderr against its golden under `tests/errors/`, named
//! `<case>.<format>`. The English is rendered here, at the output, from the
//! crates' catalogs (decision 14.37); this pins what a person reads.
//! `ANGZARR_BLESS=1` rewrites the goldens.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[path = "../../../test-support/test_dir.rs"]
mod test_dir;
use test_dir::TestDir;

const BLACKJACK: &[&str] = &[
    "io/angzarr/examples/v1/table.proto",
    "io/angzarr/examples/v1/player.proto",
    "io/angzarr/examples/v1/policies.proto",
];

const BUYIN: &[&str] = &[
    "buyin/v1/wallet.proto",
    "buyin/v1/buy_in.proto",
    "buyin/v1/policies.proto",
];

const FORMATS: [&str; 2] = ["text", "json"];

fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The spec's protos, which hold blackjack's.
fn spec() -> String {
    here()
        .join("../../angzarr-project/proto")
        .display()
        .to_string()
}

fn fixtures() -> String {
    here()
        .join("../angzarr-codegen/tests/fixtures")
        .display()
        .to_string()
}

fn empty_dir() -> TestDir {
    TestDir::new("angzarr-cli-error-output-test")
}

/// `angzarr <args> --format <format>` in a fresh directory holding
/// `files`, its stdout `stdout`.
fn run(args: &[&str], format: &str, files: &[(&str, &str)], stdout: Stdio) -> Output {
    let dir = empty_dir();
    for (name, text) in files {
        std::fs::write(dir.join(name), text).unwrap();
    }
    Command::new(env!("CARGO_BIN_EXE_angzarr"))
        .args(args)
        .args(["--format", format])
        .current_dir(&dir)
        .stdout(stdout)
        .output()
        .unwrap()
}

/// `out` failed, and its stderr is the golden `case.format`.
fn assert_golden(case: &str, format: &str, out: &Output) {
    assert_eq!(out.status.code(), Some(1), "{case}.{format}");
    let path = here().join("tests/errors").join(format!("{case}.{format}"));
    let stderr = String::from_utf8(out.stderr.clone()).unwrap();
    if std::env::var_os("ANGZARR_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &stderr).unwrap();
    }
    let golden = std::fs::read_to_string(&path).unwrap();
    assert_eq!(stderr, golden, "{case}.{format}");
}

/// Each format's stderr for `args` (protos and includes appended) against
/// its golden.
fn check(case: &str, args: &[&str], tail: &[&str], files: &[(&str, &str)]) {
    for format in FORMATS {
        let mut all = args.to_vec();
        all.extend(tail);
        assert_golden(case, format, &run(&all, format, files, Stdio::piped()));
    }
}

fn rust(extra: &[&str]) -> Vec<String> {
    let mut args: Vec<String> = ["scaffold", "rust", "-I", &fixtures(), "--src", "s"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    args.extend(extra.iter().map(|s| s.to_string()));
    args
}

fn app(extra: &[&str]) -> Vec<String> {
    let mut args: Vec<String> = [
        "scaffold",
        "app",
        "-I",
        &spec(),
        "--src",
        ".",
        "--name",
        "blackjack",
        "--in-memory",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.extend(extra.iter().map(|s| s.to_string()));
    args
}

fn strs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

#[test]
fn a_cli_error_with_a_remedy() {
    check(
        "translate-malformed",
        &strs(&rust(&["--translate", "buyin.v1.Hold"])),
        BUYIN,
        &[],
    );
    let spec = spec();
    check(
        "domain-required",
        &["scaffold", "python", "-I", &spec, "--src", "s"],
        &["io/angzarr/examples/v1/player.proto"],
        &[],
    );
    check(
        "language-malformed",
        &strs(&app(&["--language", "player"])),
        BLACKJACK,
        &[],
    );
}

#[test]
fn a_cli_error_without_a_remedy() {
    check(
        "expose-malformed",
        &strs(&app(&["--expose", "table"])),
        BLACKJACK,
        &[],
    );
}

#[test]
fn a_build_error_and_the_generators_nested_in_one() {
    check("protoc", &strs(&rust(&[])), &["buyin/v1/absent.proto"], &[]);
    check(
        "translate-unknown-policy",
        &strs(&rust(&["--translate", "x.Y:a.B=c.D"])),
        BUYIN,
        &[],
    );
    check(
        "app-refused",
        &strs(&app(&[
            "--language",
            "player=python",
            "--expose",
            "policy-table-player=ip",
        ])),
        BLACKJACK,
        &[],
    );
}

const LINTED: &str = "schema_version: 1\
                      \ncomponent:\
                      \n  - name: table\
                      \n    kind: domain\
                      \n    language: rust\
                      \n  - name: player\
                      \n    kind: domain\
                      \n    language: python\
                      \n    launch: []\
                      \n  - name: policy-table-player\
                      \n    kind: policy\
                      \n    language: python\
                      \n    expose: ip\
                      \n  - name: dealer\
                      \n    kind: domain\
                      \n    language: cobol\
                      \n";

#[test]
fn lint_warnings_and_the_lints_error() {
    let spec = spec();
    check(
        "lints",
        &["lint", "app", "-I", &spec],
        BLACKJACK,
        &[("angzarr.yaml", LINTED)],
    );
}

#[test]
fn a_config_that_does_not_read_nests_another_crates_cause() {
    let spec = spec();
    check(
        "app-config",
        &["lint", "app", "-I", &spec],
        BLACKJACK,
        &[("angzarr.yaml", "schema_version: 1\ncomponent: 7\n")],
    );
}

/// The output's own failure: stdout that refuses the bytes.
#[cfg(target_os = "linux")]
#[test]
fn an_output_that_cannot_be_written() {
    for format in FORMATS {
        let full = std::fs::OpenOptions::new()
            .write(true)
            .open(Path::new("/dev/full"))
            .unwrap();
        let out = run(&["version"], format, &[], Stdio::from(full));
        assert_golden("fmt-write", format, &out);
    }
}
