//! `angzarr scaffold rust`: each component's implementation written once
//! into the crate's `src`, never overwritten, `--missing` adding the
//! required methods it lacks, and what it lacks, has beyond and may define
//! reported; the interface is the generated domain layer's.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use angzarr_test_dir::TestDir;

use angzarr_diagnostic::Diagnostic;

/// The generator's code for an input of the wrong kind, from its error.
fn input_mismatch() -> String {
    angzarr_codegen::UseError::InputMismatch {
        path: String::new(),
        message: String::new(),
        expected: String::new(),
        found: String::new(),
    }
    .code()
    .to_string()
}

const BUYIN: &[&str] = &[
    "buyin/v1/wallet.proto",
    "buyin/v1/buy_in.proto",
    "buyin/v1/policies.proto",
];

fn fixtures() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .display()
        .to_string()
}

fn empty_dir() -> TestDir {
    TestDir::new(concat!(env!("CARGO_PKG_NAME"), "-scaffold-test"))
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

/// Every component file with `action`, but those in `completed`, which
/// were completed.
fn all(action: &str, completed: &[&str]) -> Vec<(String, String)> {
    [
        "buy_in_state_impl.rs",
        "buy_in_wallet_policy_impl.rs",
        "wallet_buy_in_policy_impl.rs",
        "wallet_state_impl.rs",
    ]
    .iter()
    .map(|f| {
        let action = if completed.contains(f) {
            "completed"
        } else {
            action
        };
        (f.to_string(), action.to_string())
    })
    .collect()
}

/// The report's entry for `path` (relative to `src`).
fn entry(out: &Output, src: &Path, path: &str) -> serde_json::Value {
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    value["files"]
        .as_array()
        .expect("files")
        .iter()
        .find(|f| Path::new(f["path"].as_str().unwrap()) == src.join(path))
        .cloned()
        .unwrap_or_else(|| panic!("no {path}: {value}"))
}

#[test]
fn scaffold_rust_writes_each_component_once_and_never_overwrites() {
    let src = empty_dir();
    assert_eq!(actions(&scaffold(&src, &[]), &src), all("created", &[]));
    let mine = "//! Mine now.\n\nimpl WalletState for WalletStateImpl {}\n";
    std::fs::write(src.join("wallet_state_impl.rs"), mine).unwrap();
    let out = scaffold(&src, &[]);
    assert_eq!(actions(&out, &src), all("kept", &[]));
    assert_eq!(
        std::fs::read_to_string(src.join("wallet_state_impl.rs")).unwrap(),
        mine
    );
    // What it lacks is reported, never written.
    assert_eq!(
        entry(&out, &src, "wallet_state_impl.rs")["lacks"],
        serde_json::json!([
            "deposit",
            "hold",
            "record_buy_in_refused",
            "apply_buy_in_refusal_recorded",
            "apply_deposited",
            "apply_held"
        ])
    );
}

/// The interface an earlier CLI wrote beside the implementation is the
/// domain layer's now: reported `obsolete`, the user's to delete.
#[test]
fn scaffold_rust_reports_an_earlier_interface_file_as_obsolete() {
    let src = empty_dir();
    scaffold(&src, &[]);
    std::fs::write(src.join("wallet_state.rs"), "pub trait WalletState {}\n").unwrap();
    let out = scaffold(&src, &[]);
    assert_eq!(entry(&out, &src, "wallet_state.rs")["action"], "obsolete");
    assert!(src.join("wallet_state.rs").exists());
}

/// Every refusal callback of a policy is required (decision 14.76): each
/// is written with the implementation.
#[test]
fn scaffold_rust_writes_every_policys_callback_stub() {
    let src = empty_dir();
    let out = scaffold(&src, &[]);
    assert_eq!(actions(&out, &src), all("created", &[]));
    let policy = std::fs::read_to_string(src.join("buy_in_wallet_policy_impl.rs")).unwrap();
    assert!(policy.contains("fn refused("), "{policy}");
    let other = std::fs::read_to_string(src.join("wallet_buy_in_policy_impl.rs")).unwrap();
    assert!(other.contains("fn refused("), "{other}");
}

/// `--missing` adds a refusal callback the implementation lacks, as any
/// required method; a Rust implementation has nothing optional to report.
#[test]
fn scaffold_rust_missing_adds_an_absent_callback_stub() {
    let src = empty_dir();
    scaffold(&src, &[]);
    let path = src.join("buy_in_wallet_policy_impl.rs");
    let fresh = std::fs::read_to_string(&path).unwrap();
    let start = fresh.find("    fn refused(").unwrap();
    let end = start + fresh[start..].find("    }\n").unwrap() + "    }\n".len();
    std::fs::write(&path, format!("{}{}", &fresh[..start], &fresh[end..])).unwrap();
    let out = scaffold(&src, &[]);
    assert_eq!(
        entry(&out, &src, "buy_in_wallet_policy_impl.rs")["lacks"],
        serde_json::json!(["refused"])
    );
    let out = scaffold(&src, &["--missing"]);
    assert_eq!(
        actions(&out, &src),
        all("kept", &["buy_in_wallet_policy_impl.rs"])
    );
    assert!(entry(&out, &src, "buy_in_wallet_policy_impl.rs")["optional"].is_null());
    let policy = std::fs::read_to_string(&path).unwrap();
    assert!(policy.contains("fn refused("), "{policy}");
}

/// The refusal callback's fact is chosen in the protos (a refusal message's
/// `refusal_status`), not on the command line: `--translate` and
/// `--on-refused` are unknown options, refused by the parser (exit 2) before
/// anything is written.
#[test]
fn scaffold_rust_has_no_translate_or_on_refused_option() {
    for flag in [
        &["--translate", "buyin.v1.P:buyin.v1.Hold=buyin.v1.HoldLost"][..],
        &["--on-refused", "buyin.v1.Hold"][..],
    ] {
        let src = empty_dir();
        let out = scaffold(&src, flag);
        assert_eq!(out.status.code(), Some(2), "{flag:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("unexpected argument"), "{stderr}");
        assert!(
            std::fs::read_dir(&src).unwrap().next().is_none(),
            "nothing written"
        );
    }
}

#[test]
fn scaffold_rust_missing_refuses_an_input_the_proto_contradicts() {
    let src = empty_dir();
    scaffold(&src, &[]);
    let path = src.join("buy_in_state_impl.rs");
    let mine = std::fs::read_to_string(&path)
        .unwrap()
        .replace("_fact: &HoldLost", "_cmd: &HoldLost");
    std::fs::write(&path, &mine).unwrap();
    let out = scaffold(&src, &["--missing"]);
    assert!(!out.status.success());
    let failure: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert!(failure.to_string().contains(&input_mismatch()), "{failure}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), mine);
}
