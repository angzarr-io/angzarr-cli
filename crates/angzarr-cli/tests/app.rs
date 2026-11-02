//! `angzarr scaffold app` and `angzarr lint app`: an application's startup
//! generated from its protos (the host config and driver the user's,
//! written once; build.rs and main.rs the CLI's, rewritten every run), and
//! the host config checked against the protos.

use std::path::Path;
use std::process::{Command, Output};

#[path = "../../../test-support/test_dir.rs"]
mod test_dir;
use test_dir::TestDir;

use angzarr_codegen::{AppError, AppLint, Warning};
use angzarr_diagnostic::{Diagnostic, code};

const BLACKJACK: &[&str] = &[
    "io/angzarr/examples/v1/table.proto",
    "io/angzarr/examples/v1/player.proto",
    "io/angzarr/examples/v1/policies.proto",
];

/// The spec's protos, which hold blackjack's.
fn spec() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../angzarr-project/proto")
        .display()
        .to_string()
}

fn golden(path: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../angzarr-codegen/tests/golden/app/blackjack")
            .join(path),
    )
    .unwrap()
}

fn empty_dir() -> TestDir {
    TestDir::new(concat!(env!("CARGO_PKG_NAME"), "-app-test"))
}

fn angzarr(args: &[&str], root: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_angzarr"))
        .args(args)
        .args(["--format", "json", "-I", &spec()])
        .args(BLACKJACK)
        .current_dir(root)
        .output()
        .unwrap()
}

const SCAFFOLD: &[&str] = &[
    "scaffold",
    "app",
    "--src",
    ".",
    "--name",
    "blackjack",
    "--language",
    "player=python",
    "--in-memory",
];

/// Each file's action in a scaffold report, by path.
fn actions(out: &Output) -> Vec<(String, String)> {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let mut files: Vec<(String, String)> = report["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            let path = f["path"].as_str().unwrap().trim_start_matches("./");
            (path.to_string(), f["action"].as_str().unwrap().to_string())
        })
        .collect();
    files.sort();
    files
}

fn each(action: &str) -> Vec<(String, String)> {
    [
        "Cargo.toml",
        "angzarr.yaml",
        "build.rs",
        "src/driver.rs",
        "src/main.rs",
    ]
    .iter()
    .map(|p| (p.to_string(), action.to_string()))
    .collect()
}

#[test]
fn scaffold_app_writes_the_golden_config_and_main_then_keeps_them() {
    let root = empty_dir();
    assert_eq!(actions(&angzarr(SCAFFOLD, &root)), each("created"));
    // The golden build script names the include `proto`, as the user
    // would; this run named the spec's absolute path, and its manifest's
    // angzarr-build is this binary's commit: only the config, main and
    // driver are compared.
    for path in ["angzarr.yaml", "src/main.rs", "src/driver.rs"] {
        assert_eq!(
            std::fs::read_to_string(root.join(path)).unwrap(),
            golden(path),
            "{path}"
        );
    }
    assert_eq!(actions(&angzarr(SCAFFOLD, &root)), each("kept"));
}

/// The config and main are the user's, written once (item S Q7): a re-run
/// keeps both as they are, and rewrites only the CLI's build script, which
/// carries the generated marker.
#[test]
fn scaffold_app_keeps_the_users_config_and_main_and_rewrites_the_build_script() {
    let root = empty_dir();
    angzarr(SCAFFOLD, &root);
    let mine = format!(
        "{}\nretry:\n  max_race_runs: 0\n",
        std::fs::read_to_string(root.join("angzarr.yaml")).unwrap()
    );
    std::fs::write(root.join("angzarr.yaml"), &mine).unwrap();
    let main = format!(
        "{}\npub mod mine;\n",
        std::fs::read_to_string(root.join("src/main.rs")).unwrap()
    );
    std::fs::write(root.join("src/main.rs"), &main).unwrap();
    let build = std::fs::read_to_string(root.join("build.rs")).unwrap();
    std::fs::write(root.join("build.rs"), format!("{build}// edited\n")).unwrap();
    let mut again = SCAFFOLD.to_vec();
    again.extend(["--expose", "table=ip", "--max-retries", "1"]);
    let done = actions(&angzarr(&again, &root));
    for (path, action) in [
        ("angzarr.yaml", "kept"),
        ("src/main.rs", "kept"),
        ("build.rs", "regenerated"),
    ] {
        assert!(
            done.contains(&(path.to_string(), action.to_string())),
            "{path}: {done:?}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(root.join("angzarr.yaml")).unwrap(),
        mine
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/main.rs")).unwrap(),
        main
    );
    assert_eq!(
        std::fs::read_to_string(root.join("build.rs")).unwrap(),
        build
    );
}

/// A build script written by hand in the CLI's place, without the
/// generated marker, is refused with its remedy, never overwritten.
#[test]
fn scaffold_app_refuses_a_build_script_without_the_marker() {
    let root = empty_dir();
    angzarr(SCAFFOLD, &root);
    std::fs::write(root.join("build.rs"), "fn main() {}\n").unwrap();
    let out = angzarr(SCAFFOLD, &root);
    assert!(!out.status.success());
    let failure = String::from_utf8_lossy(&out.stderr);
    let unmarked = angzarr_build::BuildError::GeneratedUnmarked {
        path: String::new(),
    };
    assert!(failure.contains(unmarked.code()), "{failure}");
    assert!(
        failure.contains(unmarked.remedy().unwrap().code()),
        "the remedy: {failure}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("build.rs")).unwrap(),
        "fn main() {}\n"
    );
}

#[test]
fn scaffold_app_emits_expose_and_retry_from_its_flags() {
    let root = empty_dir();
    let mut args = SCAFFOLD.to_vec();
    args.extend([
        "--expose",
        "table=loopback",
        "--max-retries",
        "5",
        "--max-race-runs",
        "0",
    ]);
    actions(&angzarr(&args, &root));
    let config = std::fs::read_to_string(root.join("angzarr.yaml")).unwrap();
    assert!(
        config.contains(
            "  - name: table\n    kind: domain\n    language: rust\n    expose: loopback\n"
        ),
        "{config}"
    );
    assert!(
        config.contains("retry:\n  max_retries: 5\n  max_race_runs: 0\n"),
        "{config}"
    );
}

#[test]
fn scaffold_app_over_a_domain_crate_writes_the_golden_main_importing_it() {
    let root = empty_dir();
    let mut args = SCAFFOLD.to_vec();
    args.extend([
        "--domain-crate",
        "examples_proto",
        "--domain-wire",
        "examples_proto::v1",
    ]);
    actions(&angzarr(&args, &root));
    let over = |path: &str| {
        std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../angzarr-codegen/tests/golden/app/blackjack-domain-crate")
                .join(path),
        )
        .unwrap()
    };
    assert_eq!(
        std::fs::read_to_string(root.join("src/main.rs")).unwrap(),
        over("src/main.rs")
    );
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(
        manifest.contains(
            "[package.metadata.angzarr.domain]\ncrate = \"examples_proto\"\nwire = \"examples_proto::v1\"\n"
        ),
        "{manifest}"
    );
    // The manifest is the user's: a run without the flag follows it.
    actions(&angzarr(SCAFFOLD, &root));
    assert_eq!(
        std::fs::read_to_string(root.join("src/main.rs")).unwrap(),
        over("src/main.rs")
    );
}

#[test]
fn scaffold_app_refuses_a_wire_path_without_a_domain_crate() {
    let root = empty_dir();
    let mut args = SCAFFOLD.to_vec();
    args.extend(["--domain-wire", "examples_proto::v1"]);
    let out = angzarr(&args, &root);
    assert!(!out.status.success());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
}

#[test]
fn scaffold_app_refuses_exposing_a_policy_and_writes_nothing() {
    let root = empty_dir();
    let mut args = SCAFFOLD.to_vec();
    args.extend(["--expose", "policy-table-player=ip"]);
    let out = angzarr(&args, &root);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    let refused = AppError::Refused { lints: Vec::new() };
    assert!(stderr.contains(refused.code()), "{stderr}");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
}

#[test]
fn scaffold_app_refuses_a_malformed_expose() {
    let root = empty_dir();
    for value in ["table", "table=world", "=ip"] {
        let mut args = SCAFFOLD.to_vec();
        args.extend(["--expose", value]);
        let out = angzarr(&args, &root);
        assert!(!out.status.success(), "{value}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(code!("expose-malformed")),
            "{value}: {stderr}"
        );
    }
}

#[test]
fn lint_app_passes_the_generated_config() {
    let root = empty_dir();
    std::fs::write(root.join("angzarr.yaml"), golden("angzarr.yaml")).unwrap();
    let out = angzarr(&["lint", "app"], &root);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // A declaration warning of the protos is shown and fails nothing: the
    // table-to-wallet policy sends to a domain these protos do not own.
    let stderr = String::from_utf8_lossy(&out.stderr);
    let unowned = Warning::UnownedTarget {
        policy: String::new(),
        domain: String::new(),
    };
    assert!(
        stderr.contains(&format!("angzarr: warning[{}]: ", unowned.code())),
        "{stderr}"
    );
}

#[test]
fn lint_app_reports_each_lint_and_fails() {
    let root = empty_dir();
    std::fs::write(
        root.join("angzarr.yaml"),
        "schema_version: 1\
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
         \n",
    )
    .unwrap();
    let out = angzarr(&["lint", "app"], &root);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    let s = String::new;
    for code in [
        AppLint::EmptyLaunch { name: s() }.code(),
        AppLint::NotExposable {
            name: s(),
            kind: s(),
        }
        .code(),
        AppLint::UnknownComponent {
            name: s(),
            kind: s(),
        }
        .code(),
        AppLint::UnknownLanguage {
            name: s(),
            language: s(),
        }
        .code(),
        code!("lints"),
    ] {
        assert!(stderr.contains(code), "{code}:\n{stderr}");
    }
}

#[test]
fn scaffold_app_puts_a_policy_in_the_language_given() {
    let root = empty_dir();
    let mut args = SCAFFOLD.to_vec();
    args.extend(["--language", "policy-table-player=python"]);
    actions(&angzarr(&args, &root));
    let config = std::fs::read_to_string(root.join("angzarr.yaml")).unwrap();
    assert!(
        config.contains(
            "  - name: policy-table-player\n    kind: policy\n    language: python\n    \
             dir: policy-table-player\n    entry: [angzarr-policy-table-player]\n"
        ),
        "{config}"
    );
}

#[test]
fn scaffold_app_refuses_a_malformed_language_and_the_python_flag_is_gone() {
    let root = empty_dir();
    for value in ["player", "player=", "=python"] {
        let mut args = SCAFFOLD.to_vec();
        args.extend(["--language", value]);
        let out = angzarr(&args, &root);
        assert!(!out.status.success(), "{value}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(code!("language-malformed")),
            "{value}: {stderr}"
        );
    }
    let mut args = SCAFFOLD.to_vec();
    args.extend(["--python", "player"]);
    assert!(!angzarr(&args, &root).status.success());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
}
