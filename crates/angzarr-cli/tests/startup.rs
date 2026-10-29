//! `angzarr scaffold startup` and `angzarr lint startup`: the host config,
//! build.rs and main.rs written once, and the config checked against the
//! protos.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const FILES: &[&str] = &[
    "io/angzarr/examples/v1/table.proto",
    "io/angzarr/examples/v1/player.proto",
    "io/angzarr/examples/v1/policies.proto",
];

fn fixtures() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../angzarr-codegen/tests/fixtures")
        .display()
        .to_string()
}

fn golden(path: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../angzarr-codegen/tests/golden/startup/blackjack")
            .join(path),
    )
    .unwrap()
}

fn empty_dir() -> PathBuf {
    static RUN: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "angzarr-cli-startup-test-{}-{}",
        std::process::id(),
        RUN.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn angzarr(args: &[&str], root: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_angzarr"))
        .args(args)
        .args(["--format", "json", "-I", &fixtures()])
        .args(FILES)
        .current_dir(root)
        .output()
        .unwrap()
}

#[test]
fn scaffold_startup_writes_the_config_build_script_and_main_once() {
    let root = empty_dir();
    let args = [
        "scaffold",
        "startup",
        "--src",
        ".",
        "--name",
        "blackjack",
        "--python",
        "player",
    ];
    let out = angzarr(&args, &root);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The golden build script names the include as the user would, `proto`;
    // this run named the fixtures' absolute path.
    for path in ["angzarr.toml", "src/main.rs"] {
        assert_eq!(
            std::fs::read_to_string(root.join(path)).unwrap(),
            golden(path),
            "{path}"
        );
    }
    let mine = "# mine now\n";
    std::fs::write(root.join("angzarr.toml"), mine).unwrap();
    let again = angzarr(&args, &root);
    let report: serde_json::Value = serde_json::from_slice(&again.stdout).unwrap();
    assert!(
        report["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["action"] == "kept"),
        "{report}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("angzarr.toml")).unwrap(),
        mine
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn lint_startup_passes_the_generated_config() {
    let root = empty_dir();
    std::fs::write(root.join("angzarr.toml"), golden("angzarr.toml")).unwrap();
    let out = angzarr(&["lint", "startup"], &root);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn lint_startup_reports_each_lint_and_fails() {
    let root = empty_dir();
    std::fs::write(
        root.join("angzarr.toml"),
        "[[domain]]\nname = \"table\"\nlanguage = \"rust\"\n\n\
         [[domain]]\nname = \"player\"\nlanguage = \"python\"\nlaunch = []\n\n\
         [[domain]]\nname = \"dealer\"\nlanguage = \"cobol\"\n",
    )
    .unwrap();
    let out = angzarr(&["lint", "startup"], &root);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    for code in [
        "angzarr-codegen-startup-empty-launch",
        "angzarr-codegen-startup-unknown-domain",
        "angzarr-codegen-startup-unknown-language",
        "angzarr-cli-lints",
    ] {
        assert!(stderr.contains(code), "{code}:\n{stderr}");
    }
    std::fs::remove_dir_all(&root).unwrap();
}
