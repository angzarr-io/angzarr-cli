//! End to end: a blackjack application the CLI scaffolds, run. `scaffold
//! app` writes its startup from the spec's protos (the host config, the
//! table in process and the player a Python child, the store in memory; the
//! build script; the `main` running the generated `register()` with the
//! config), `scaffold rust` the Rust table and its two policies, `scaffold
//! python` the player at `player/`; the user's files (`tests/e2e/blackjack`)
//! are the table's logic, agg-table's `TableLogic`, and the policies',
//! table-player-policies', each behind the generated interface, a
//! basic-strategy player, and a driver that seats two players and deals
//! seed 21. From the deal the turn crosses to the
//! Python child and back by the policies alone, until the round settles;
//! the outcome is the table's own test's (examples-rust
//! `table/embedded/tests/remote_player.rs`).
//!
//! agg-table, examples-proto and table-player-policies are examples-rust's,
//! at [`EXAMPLES_RUST_REV`]: git dependencies pinned by hash, as
//! angzarr-embedded and angzarr-client are, so the outcome is that commit's
//! and not whatever a local checkout holds.
//!
//! Ignored: it needs the network (buf.build, GitHub) and uv.

mod common;

use common::{angzarr, copy_dir, here, overlay};

const PROTOS: &[&str] = &[
    "io/angzarr/examples/v1/table.proto",
    "io/angzarr/examples/v1/player.proto",
    "io/angzarr/examples/v1/policies.proto",
];

/// The examples-rust commit the table's crates are built from: on
/// angzarr-embedded at [`common`]'s rev, so the app has one angzarr-embedded
/// and one angzarr-proto.
const EXAMPLES_RUST_REV: &str = "320707759c44851434797165c9050c387ba92585";
/// Where [`EXAMPLES_RUST_REV`] is fetched from.
const EXAMPLES_RUST_GIT: &str = "https://github.com/angzarr-io/angzarr-examples-rust";

#[test]
#[ignore = "needs the network (buf.build, GitHub) and uv"]
fn a_scaffolded_blackjack_app_plays_seed_21_to_settlement_with_the_python_player() {
    let app = std::env::temp_dir().join(format!("angzarr-cli-blackjack-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app);
    copy_dir(
        &here().join("../../angzarr-project/proto/io/angzarr/examples"),
        &app.join("proto/io/angzarr/examples"),
    );
    let mut startup = vec![
        "scaffold",
        "app",
        "-I",
        "proto",
        "--src",
        ".",
        "--name",
        "blackjack",
        "--python",
        "player",
        "--in-memory",
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
            "ef975bcf6fd6a0207476667d18ddc7bb29a10ab4",
            PROTOS[1],
        ],
    );

    // The user's part: the logic, the driver (over the scaffold's stub),
    // and the table's crates.
    overlay(&here().join("tests/e2e/blackjack"), &app);
    let examples = format!("{{ git = \"{EXAMPLES_RUST_GIT}\", rev = \"{EXAMPLES_RUST_REV}\" }}");
    common::manifest(
        &app,
        "blackjack",
        &format!(
            "agg-table = {examples}\nexamples-proto = {examples}\n\
             table-player-policies = {examples}\n"
        ),
    );

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
