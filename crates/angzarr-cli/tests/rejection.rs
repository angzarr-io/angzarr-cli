//! End to end: a Python decision's refusal, as the Rust host receives it.
//! `scaffold python` writes the player of a one-domain application, served
//! by a Python child; the user's files (`tests/e2e/rejection`) are the host
//! config, a build script generating the domain layer and `register()`, a
//! `main` running `register()` with the config, a player whose
//! `decide_move` raises the generated `domain.Rejection("some-reason-key",
//! amount=13)`, and a driver that sends one DecideMove through
//! `App::handle`, then the same command straight to the child's handler.
//!
//! A total of 99 makes the decision raise the generated `domain.Bug(
//! "some-bug-key", amount=7)` instead: it must arrive as a bug, the
//! router's INTERNAL ROUTER_REASON_UNHANDLED_HANDLER_ERROR with the key and
//! args as its cause (`cause`, `cause.amount`), recovery Bug.
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
        &here().join("../../angzarr-project/proto/io/angzarr"),
        &app.join("proto/io/angzarr"),
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
            "2bd7cbde0ecb4b8070d87c8ba4d288c43aa1a6ad",
            PLAYER,
        ],
    );
    overlay(&here().join("tests/e2e/rejection"), &app);
    common::manifest(&app, "rejection", "");

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
            "bug: Internal ROUTER_REASON_UNHANDLED_HANDLER_ERROR cause=some-bug-key cause.amount=7 recovery Bug",
            "player events: 0",
            "",
        ]
        .join("\n"),
        "stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::remove_dir_all(&app).unwrap();
}
