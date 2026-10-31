//! `angzarr scaffold python` of every component kind, run under uv against
//! the real angzarr-client: the ledger projector (two sources, async), the
//! table-to-player policy, the buy-in process manager (with its fact) and
//! the table (typed refusals, emits cardinality). Each project gets the
//! pytest module of `tests/python` beside its own tests: the generated
//! dispatch answering a delivery, its generated descriptor the client's
//! `describe` of it, and a hand-written dispatch built on the client's
//! public API, described by `describe`, hosted in its place.
//!
//! Ignored: it needs the network (buf.build, GitHub) and uv.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The spec draft whose protos the projects generate from (decision 14.50):
/// the angzarr-project submodule's commit.
const SPEC_REV: &str = "2bd7cbde0ecb4b8070d87c8ba4d288c43aa1a6ad";

fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "angzarr-cli-scaffold-python-kinds-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Scaffolds `proto` as `domain` (a domain or a component name) into a
/// fresh project, adds `check` to its tests and runs them under uv.
fn runs(proto: &str, domain: &str, check: &str) {
    let root = scratch(domain);
    let spec = here().join("../../angzarr-project/proto");
    let out = Command::new(env!("CARGO_BIN_EXE_angzarr"))
        .args(["scaffold", "python", "--format", "json", "-I"])
        .arg(&spec)
        .args(["--domain", domain])
        .args([
            "--proto-git",
            "https://github.com/angzarr-io/angzarr-project",
            "--proto-ref",
            SPEC_REV,
        ])
        .arg("--src")
        .arg(&root)
        .arg(format!("io/angzarr/examples/v1/{proto}"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{domain}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::copy(
        here().join("tests/python").join(check),
        root.join("tests").join(check),
    )
    .unwrap();
    let test = Command::new("uv")
        .args(["run", "--locked", "pytest", "-q", "-p", "no:cacheprovider"])
        .arg(Path::new("tests").join(check))
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        test.status.success(),
        "{domain}:\n{}\n{}",
        String::from_utf8_lossy(&test.stdout),
        String::from_utf8_lossy(&test.stderr)
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
#[ignore = "needs the network (buf.build, GitHub) and uv"]
fn a_scaffolded_projector_runs_under_uv_and_a_hand_written_one_replaces_it() {
    runs(
        "ledger.proto",
        "projector-wallet-table-ledger",
        "test_ledger.py",
    );
}

#[test]
#[ignore = "needs the network (buf.build, GitHub) and uv"]
fn a_scaffolded_policy_runs_under_uv_and_a_hand_written_one_replaces_it() {
    runs("policies.proto", "policy-table-player", "test_policy.py");
}

/// Red against buf.build today: its remote python plugin v35.1 answers
/// buy_in.proto alone for the two targets buy_in.proto and wallet.proto
/// (wallet_pb2, which buy_in_pb2 imports, missing), where v34.1 and buf's
/// local protoc_builtin generate both. With wallet_pb2 there the module
/// passes.
#[test]
#[ignore = "needs the network (buf.build, GitHub) and uv"]
fn a_scaffolded_process_manager_runs_under_uv_and_a_hand_written_one_replaces_it() {
    runs("buy_in.proto", "buy-in", "test_buy_in.py");
}

#[test]
#[ignore = "needs the network (buf.build, GitHub) and uv"]
fn a_scaffolded_aggregate_refuses_with_its_typed_refusal_under_uv() {
    runs("table.proto", "table", "test_table.py");
}
