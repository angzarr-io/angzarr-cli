//! End to end: a blackjack application the CLI scaffolds, run. `scaffold
//! startup` and `scaffold rust` write the Rust table and its two policies
//! from the spec's protos, `scaffold python` the player at `player/`; the
//! user's files (`tests/e2e/blackjack`) are the table's logic, agg-table's
//! `TableLogic`, and the policies', table-player-policies', each behind the
//! generated interface, a basic-strategy player, and a driver that seats
//! two players and deals seed 21. From the deal the turn crosses to the
//! Python child and back by the policies alone, until the round settles;
//! the outcome is the table's own test's (examples-rust
//! `table/embedded/tests/remote_player.rs`).
//!
//! Ignored: it needs the network (buf.build, GitHub), uv and the
//! examples-rust checkout holding agg-table and table-player-policies,
//! `$ANGZARR_EXAMPLES_RUST` (by default
//! `~/workspace/worktrees/examples-rust--blackjack-embedded`).

use std::path::{Path, PathBuf};
use std::process::Command;

const PROTOS: &[&str] = &[
    "io/angzarr/examples/v1/table.proto",
    "io/angzarr/examples/v1/player.proto",
    "io/angzarr/examples/v1/policies.proto",
];

fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn examples_rust() -> PathBuf {
    match std::env::var_os("ANGZARR_EXAMPLES_RUST") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var_os("HOME").expect("HOME is set"))
            .join("workspace/worktrees/examples-rust--blackjack-embedded"),
    }
}

/// `angzarr <args>` in `dir`, which must succeed.
fn angzarr(dir: &Path, args: &[&str]) {
    let out = Command::new(env!("CARGO_BIN_EXE_angzarr"))
        .args(args)
        .args(["--format", "text"])
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "angzarr {args:?}:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The user's files over the scaffold's: every file under `from`, at the
/// same path under `to`.
fn overlay(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            overlay(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

#[test]
#[ignore = "needs the network (buf.build, GitHub), uv, and the examples-rust checkout"]
fn a_scaffolded_blackjack_app_plays_seed_21_to_settlement_with_the_python_player() {
    let app = std::env::temp_dir().join(format!("angzarr-cli-blackjack-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app);
    copy_dir(
        &here().join("../angzarr-codegen/tests/fixtures/io"),
        &app.join("proto/io"),
    );
    let build = format!(
        "{{ path = {:?} }}",
        here().join("../angzarr-build").display().to_string()
    );
    let mut startup = vec![
        "scaffold",
        "startup",
        "-I",
        "proto",
        "--src",
        ".",
        "--name",
        "blackjack",
        "--python",
        "player",
        "--angzarr-build",
        &build,
    ];
    startup.extend(PROTOS);
    angzarr(&app, &startup);
    angzarr(
        &app,
        &[
            "scaffold", "rust", "-I", "proto", "--src", "src", PROTOS[0], PROTOS[2],
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
            PROTOS[1],
        ],
    );

    // The user's part: the logic, the driver, the table's crates, and an
    // in-memory store.
    overlay(&here().join("tests/e2e/blackjack"), &app);
    let examples = examples_rust();
    let manifest = std::fs::read_to_string(app.join("Cargo.toml")).unwrap();
    let crates = format!(
        "prost = \"0.14\"\nagg-table = {{ path = {:?} }}\nexamples-proto = {{ path = {:?} }}\n\
         table-player-policies = {{ path = {:?} }}\n",
        examples.join("table/agg").display().to_string(),
        examples.join("proto").display().to_string(),
        examples.join("table/policies").display().to_string(),
    );
    // A workspace of its own, wherever the temporary directory is.
    let manifest = manifest.replace("prost = \"0.14\"\n", &crates) + "\n[workspace]\n";
    std::fs::write(app.join("Cargo.toml"), manifest).unwrap();
    let config = std::fs::read_to_string(app.join("angzarr.toml"))
        .unwrap()
        .replace("path = \"angzarr.db\"", "in_memory = true");
    std::fs::write(app.join("angzarr.toml"), config).unwrap();

    let out = Command::new(env!("CARGO"))
        .args(["run", "--quiet"])
        .current_dir(&app)
        .env("CARGO_TARGET_DIR", here().join("../../target/e2e"))
        .output()
        .expect("cargo runs");
    assert!(
        out.status.success(),
        "the app runs:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        [
            "alice: DoubleDown",
            "bob: Hit Hit Stand",
            "table: RoundDealt TurnStarted HandDoubled TurnStarted CardDealt TurnStarted \
             CardDealt TurnStarted HandStood DealerPlayed RoundSettled",
            "seat 0: wager 40 returned 80 stack 540",
            "seat 1: wager 20 returned 40 stack 520",
            "house -60",
            "",
        ]
        .join("\n")
    );
    std::fs::remove_dir_all(&app).unwrap();
}
