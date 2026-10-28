//! `angzarr scaffold rust`: each component's file written once into the
//! crate's `src`, never overwritten, and completed with `--missing`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const BUYIN: &[&str] = &[
    "buyin/v1/wallet.proto",
    "buyin/v1/buy_in.proto",
    "buyin/v1/policies.proto",
];

fn fixtures() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../angzarr-codegen/tests/fixtures")
        .display()
        .to_string()
}

fn empty_dir() -> PathBuf {
    static RUN: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "angzarr-cli-scaffold-test-{}-{}",
        std::process::id(),
        RUN.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `angzarr scaffold rust` over the buy-in protos into `src`, with `extra`.
fn scaffold(src: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_angzarr"))
        .args(["scaffold", "rust", "--format", "json", "-I", &fixtures()])
        .arg("--src")
        .arg(src)
        .args(extra)
        .args(BUYIN)
        .output()
        .unwrap()
}

/// `(path, action)` per file, sorted, the path relative to `src`.
fn actions(out: &Output, src: &Path) -> Vec<(String, String)> {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let mut actions: Vec<(String, String)> = value["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| {
            let path = PathBuf::from(f["path"].as_str().unwrap());
            (
                path.strip_prefix(src).unwrap().display().to_string(),
                f["action"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    actions.sort();
    actions
}

fn pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

#[test]
fn scaffold_rust_writes_each_component_once_and_never_overwrites() {
    let src = empty_dir();
    assert_eq!(
        actions(&scaffold(&src, &[]), &src),
        pairs(&[
            ("buy_in.rs", "created"),
            ("buy_in_wallet.rs", "created"),
            ("wallet.rs", "created"),
            ("wallet_buy_in.rs", "created"),
        ])
    );
    let mine = "//! Mine now.\n";
    std::fs::write(src.join("wallet.rs"), mine).unwrap();
    assert_eq!(
        actions(&scaffold(&src, &[]), &src),
        pairs(&[
            ("buy_in.rs", "kept"),
            ("buy_in_wallet.rs", "kept"),
            ("wallet.rs", "kept"),
            ("wallet_buy_in.rs", "kept"),
        ])
    );
    assert_eq!(
        std::fs::read_to_string(src.join("wallet.rs")).unwrap(),
        mine
    );
    std::fs::remove_dir_all(&src).unwrap();
}

#[test]
fn scaffold_rust_missing_with_translate_adds_the_policys_callback_stub() {
    let src = empty_dir();
    scaffold(&src, &[]);
    let out = scaffold(
        &src,
        &[
            "--missing",
            "--translate",
            "buyin.v1.BuyInWalletPolicy:buyin.v1.Hold=buyin.v1.HoldLost",
        ],
    );
    assert_eq!(
        actions(&out, &src),
        pairs(&[
            ("buy_in.rs", "kept"),
            ("buy_in_wallet.rs", "completed"),
            ("wallet.rs", "kept"),
            ("wallet_buy_in.rs", "kept"),
        ])
    );
    let policy = std::fs::read_to_string(src.join("buy_in_wallet.rs")).unwrap();
    assert!(policy.contains("#[on_refused(Hold)]"), "{policy}");
    std::fs::remove_dir_all(&src).unwrap();
}

#[test]
fn scaffold_rust_refuses_a_malformed_translate() {
    let src = empty_dir();
    let out = scaffold(&src, &["--translate", "buyin.v1.Hold"]);
    assert!(!out.status.success());
    assert!(
        std::fs::read_dir(&src).unwrap().next().is_none(),
        "nothing written"
    );
    std::fs::remove_dir_all(&src).unwrap();
}
