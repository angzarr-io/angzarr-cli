//! `angzarr scaffold app` and `angzarr lint app`: an application's startup
//! generated from its protos (the host config and driver the user's,
//! written once; build.rs and main.rs the CLI's, rewritten every run), and
//! the host config checked against the protos.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

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

fn empty_dir() -> PathBuf {
    static RUN: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "angzarr-cli-app-test-{}-{}",
        std::process::id(),
        RUN.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
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
    "--python",
    "player",
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
        "angzarr.toml",
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
    for path in ["angzarr.toml", "src/main.rs", "src/driver.rs"] {
        assert_eq!(
            std::fs::read_to_string(root.join(path)).unwrap(),
            golden(path),
            "{path}"
        );
    }
    assert_eq!(actions(&angzarr(SCAFFOLD, &root)), each("kept"));
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn scaffold_app_keeps_the_users_config_and_rewrites_a_hand_edited_main() {
    let root = empty_dir();
    angzarr(SCAFFOLD, &root);
    let mine = format!(
        "{}\n[retry]\nmax_race_runs = 0\n",
        std::fs::read_to_string(root.join("angzarr.toml")).unwrap()
    );
    std::fs::write(root.join("angzarr.toml"), &mine).unwrap();
    std::fs::write(root.join("src/main.rs"), "// edited\n").unwrap();
    let mut again = SCAFFOLD.to_vec();
    again.extend(["--expose", "table=ip", "--max-retries", "1"]);
    let done = actions(&angzarr(&again, &root));
    assert!(
        done.contains(&("angzarr.toml".to_string(), "kept".to_string())),
        "{done:?}"
    );
    assert!(
        done.contains(&("src/main.rs".to_string(), "regenerated".to_string())),
        "{done:?}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("angzarr.toml")).unwrap(),
        mine
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/main.rs")).unwrap(),
        golden("src/main.rs")
    );
    std::fs::remove_dir_all(&root).unwrap();
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
    let config = std::fs::read_to_string(root.join("angzarr.toml")).unwrap();
    assert!(
        config.contains(
            "name = \"table\"\nkind = \"domain\"\nlanguage = \"rust\"\nexpose = \"loopback\"\n"
        ),
        "{config}"
    );
    assert!(
        config.contains("[retry]\nmax_retries = 5\nmax_race_runs = 0\n"),
        "{config}"
    );
    std::fs::remove_dir_all(&root).unwrap();
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
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn scaffold_app_refuses_a_wire_path_without_a_domain_crate() {
    let root = empty_dir();
    let mut args = SCAFFOLD.to_vec();
    args.extend(["--domain-wire", "examples_proto::v1"]);
    let out = angzarr(&args, &root);
    assert!(!out.status.success());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn scaffold_app_refuses_exposing_a_policy_and_writes_nothing() {
    let root = empty_dir();
    let mut args = SCAFFOLD.to_vec();
    args.extend(["--expose", "policy-table-player=ip"]);
    let out = angzarr(&args, &root);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("angzarr-codegen-app-refused"), "{stderr}");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    std::fs::remove_dir_all(&root).unwrap();
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
            stderr.contains("angzarr-cli-expose-malformed"),
            "{value}: {stderr}"
        );
    }
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn lint_app_passes_the_generated_config() {
    let root = empty_dir();
    std::fs::write(root.join("angzarr.toml"), golden("angzarr.toml")).unwrap();
    let out = angzarr(&["lint", "app"], &root);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn lint_app_reports_each_lint_and_fails() {
    let root = empty_dir();
    std::fs::write(
        root.join("angzarr.toml"),
        "[[component]]\nname = \"table\"\nkind = \"domain\"\nlanguage = \"rust\"\n\n\
         [[component]]\nname = \"player\"\nkind = \"domain\"\nlanguage = \"python\"\n\
         launch = []\n\n\
         [[component]]\nname = \"policy-table-player\"\nkind = \"policy\"\n\
         language = \"python\"\nexpose = \"ip\"\n\n\
         [[component]]\nname = \"dealer\"\nkind = \"domain\"\nlanguage = \"cobol\"\n",
    )
    .unwrap();
    let out = angzarr(&["lint", "app"], &root);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    for code in [
        "angzarr-codegen-app-empty-launch",
        "angzarr-codegen-app-not-exposable",
        "angzarr-codegen-app-unknown-component",
        "angzarr-codegen-app-unknown-language",
        "angzarr-cli-lints",
    ] {
        assert!(stderr.contains(code), "{code}:\n{stderr}");
    }
    std::fs::remove_dir_all(&root).unwrap();
}
