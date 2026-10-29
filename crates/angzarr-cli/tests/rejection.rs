//! End to end: a Python decision's refusal, as the Rust host receives it.
//! `scaffold startup` and `scaffold python` write a one-domain application,
//! the player served by a Python child; the user's files
//! (`tests/e2e/rejection`) are a player whose `decide_move` raises the
//! generated `domain.Rejection("some-reason-key", amount=13)`, and a driver
//! that sends one DecideMove through `App::handle`, then the same command
//! straight to the child's handler.
//!
//! The refusal must arrive as a business refusal: the kernel's
//! `Failure::Rejected` (recovery Business, neither retryable nor a bug,
//! never REMOTE_HANDLER_UNAVAILABLE or an internal error), FAILED_PRECONDITION
//! on the wire, with the key and args raised, and nothing written to the
//! player's stream.
//!
//! Ignored: it needs the network (buf.build, GitHub) and uv.

mod common;

use common::{angzarr, copy_dir, here, overlay};

const PLAYER: &str = "io/angzarr/examples/v1/player.proto";

#[test]
#[ignore = "needs the network (buf.build, GitHub) and uv"]
fn a_python_rejection_reaches_the_host_as_a_business_refusal_with_its_key_and_args() {
    let app = std::env::temp_dir().join(format!("angzarr-cli-rejection-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app);
    copy_dir(
        &here().join("../angzarr-codegen/tests/fixtures/io"),
        &app.join("proto/io"),
    );
    let build = format!(
        "{{ path = {:?} }}",
        here().join("../angzarr-build").display().to_string()
    );
    angzarr(
        &app,
        &[
            "scaffold",
            "startup",
            "-I",
            "proto",
            "--src",
            ".",
            "--name",
            "rejection",
            "--python",
            "player",
            "--angzarr-build",
            &build,
            PLAYER,
        ],
    );
    angzarr(
        &app,
        &[
            "scaffold",
            "python",
            "-I",
            "proto",
            "--src",
            "player",
            "--domain",
            "player",
            "--proto-git",
            "https://github.com/angzarr-io/angzarr-project",
            "--proto-ref",
            "b9b87bfc30acbb90b089515761186ceb5aa6d52d",
            PLAYER,
        ],
    );
    overlay(&here().join("tests/e2e/rejection"), &app);
    // A workspace of its own, wherever the temporary directory is.
    let manifest = std::fs::read_to_string(app.join("Cargo.toml")).unwrap() + "\n[workspace]\n";
    std::fs::write(app.join("Cargo.toml"), manifest).unwrap();

    let out = common::run(&app);
    assert!(
        out.status.success(),
        "the app runs:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        [
            "handle: rejected some-reason-key amount=13 recovery Business",
            "dispatch: FailedPrecondition some-reason-key amount=13 retryable false",
            "player events: 0",
            "",
        ]
        .join("\n"),
        "stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::remove_dir_all(&app).unwrap();
}
