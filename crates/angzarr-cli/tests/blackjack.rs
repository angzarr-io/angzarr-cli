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
//! agg-table, examples-proto and table-player-policies are examples-rust's,
//! at [`EXAMPLES_RUST_REV`]: git dependencies pinned by hash, as the
//! scaffold pins angzarr-embedded and angzarr-client, so the outcome is
//! that commit's and not whatever a local checkout holds.
//!
//! Ignored: it needs the network (buf.build, GitHub) and uv.

mod common;

use common::{angzarr, copy_dir, here, overlay};

const PROTOS: &[&str] = &[
    "io/angzarr/examples/v1/table.proto",
    "io/angzarr/examples/v1/player.proto",
    "io/angzarr/examples/v1/policies.proto",
];

/// The examples-rust commit the table's crates are built from (its main).
const EXAMPLES_RUST_REV: &str = "9b569ae9162c1d918fcee1d1cc27aea9fa524fe5";
/// Where [`EXAMPLES_RUST_REV`] is fetched from.
const EXAMPLES_RUST_GIT: &str = "https://github.com/angzarr-io/angzarr-examples-rust";

#[test]
#[ignore = "needs the network (buf.build, GitHub) and uv"]
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
    let manifest = std::fs::read_to_string(app.join("Cargo.toml")).unwrap();
    let examples = format!("{{ git = \"{EXAMPLES_RUST_GIT}\", rev = \"{EXAMPLES_RUST_REV}\" }}");
    let crates = format!(
        "prost = \"0.14\"\nagg-table = {examples}\nexamples-proto = {examples}\n\
         table-player-policies = {examples}\n"
    );
    // A workspace of its own, wherever the temporary directory is.
    let manifest = manifest.replace("prost = \"0.14\"\n", &crates) + "\n[workspace]\n";
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

#[test]
fn the_e2e_players_decision_builds_its_event_with_from_command() {
    let logic = std::fs::read_to_string(
        here().join("tests/e2e/blackjack/player/src/angzarr_player/logic_impl.py"),
    )
    .unwrap();
    assert!(
        logic.contains("return MoveDecided.from_command(cmd, action=_move(cmd))"),
        "{logic}"
    );
}
