//! `angzarr scaffold rust`: each component's implementation written once
//! into the crate's `src`, never overwritten, `--missing` adding the
//! required methods it lacks, and what it lacks, has beyond and may define
//! reported; the interface is the generated domain layer's.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "../../../test-support/test_dir.rs"]
mod test_dir;
use test_dir::TestDir;

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
        .join("../angzarr-codegen/tests/fixtures")
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
        "buy_in_impl.rs",
        "buy_in_wallet_impl.rs",
        "wallet_buy_in_impl.rs",
        "wallet_impl.rs",
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
    let mine = "//! Mine now.\n\nimpl Wallet for WalletImpl {}\n";
    std::fs::write(src.join("wallet_impl.rs"), mine).unwrap();
    let out = scaffold(&src, &[]);
    assert_eq!(actions(&out, &src), all("kept", &[]));
    assert_eq!(
        std::fs::read_to_string(src.join("wallet_impl.rs")).unwrap(),
        mine
    );
    // What it lacks is reported, never written.
    assert_eq!(
        entry(&out, &src, "wallet_impl.rs")["lacks"],
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
    std::fs::write(src.join("wallet.rs"), "pub trait Wallet {}\n").unwrap();
    let out = scaffold(&src, &[]);
    assert_eq!(entry(&out, &src, "wallet.rs")["action"], "obsolete");
    assert!(src.join("wallet.rs").exists());
}

/// Every refusal callback of a policy is required (decision 14.76): each
/// is written with the implementation, `--translate` or not.
#[test]
fn scaffold_rust_writes_every_policys_callback_stub() {
    let src = empty_dir();
    let out = scaffold(
        &src,
        &[
            "--translate",
            "buyin.v1.BuyInWalletPolicy:buyin.v1.Hold=buyin.v1.HoldLost",
        ],
    );
    assert_eq!(actions(&out, &src), all("created", &[]));
    let policy = std::fs::read_to_string(src.join("buy_in_wallet_impl.rs")).unwrap();
    for callback in ["fn hold_refused(", "fn deposit_refused("] {
        assert!(policy.contains(callback), "{callback}:\n{policy}");
    }
    let other = std::fs::read_to_string(src.join("wallet_buy_in_impl.rs")).unwrap();
    assert!(other.contains("fn record_held_refused("), "{other}");
}

/// `--missing` adds a refusal callback the implementation lacks, as any
/// required method; a Rust implementation has nothing optional to report.
#[test]
fn scaffold_rust_missing_adds_an_absent_callback_stub() {
    let src = empty_dir();
    scaffold(&src, &[]);
    let path = src.join("buy_in_wallet_impl.rs");
    let fresh = std::fs::read_to_string(&path).unwrap();
    let start = fresh.find("    fn hold_refused(").unwrap();
    let end = start + fresh[start..].find("    }\n").unwrap() + "    }\n".len();
    std::fs::write(&path, format!("{}{}", &fresh[..start], &fresh[end..])).unwrap();
    let out = scaffold(&src, &[]);
    assert_eq!(
        entry(&out, &src, "buy_in_wallet_impl.rs")["lacks"],
        serde_json::json!(["hold_refused"])
    );
    let out = scaffold(&src, &["--missing"]);
    assert_eq!(actions(&out, &src), all("kept", &["buy_in_wallet_impl.rs"]));
    assert!(entry(&out, &src, "buy_in_wallet_impl.rs")["optional"].is_null());
    let policy = std::fs::read_to_string(&path).unwrap();
    assert!(policy.contains("fn hold_refused("), "{policy}");
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
}

#[test]
fn scaffold_rust_missing_refuses_an_input_the_proto_contradicts() {
    let src = empty_dir();
    scaffold(&src, &[]);
    let path = src.join("buy_in_impl.rs");
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
