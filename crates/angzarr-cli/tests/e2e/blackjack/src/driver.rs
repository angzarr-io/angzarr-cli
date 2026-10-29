//! The blackjack driver: seed 21's hand (one deck), the table's own test's
//! (examples-rust remote_player.rs): Alice 4 6 and Bob A 5 against the
//! dealer's 9. The driver seats both and deals; the turn then crosses to
//! the Python player and back by the two policies alone, until the round
//! settles. It prints what happened, one line per fact the test checks.

use angzarr_embedded::next::v1::{Command, CommandBook, CommandPage, Cover, Uuid, command_page};
use angzarr_embedded::next::{Sampled, Timestamp, pack, type_name};
use angzarr_embedded_host::App;
use prost::{Message, Name};

use crate::wire;

const TABLE: u8 = 0x7a;
const ALICE: u8 = 0xa1;
const BOB: u8 = 0xb0;

fn cover(domain: &str, b: u8) -> Cover {
    Cover {
        domain: domain.to_string(),
        root: Some(Uuid::from_bytes([b; 16])),
        edition: None,
    }
}

fn to_table<M: Message + Name>(
    app: &App,
    n: u8,
    command: M,
) -> Result<(), Box<dyn std::error::Error>> {
    let book = CommandBook {
        cover: Some(cover("table", TABLE)),
        page: Some(CommandPage {
            body: Some(command_page::Body::Command(Command {
                payload: Some(pack(&command)),
            })),
            command_id: Some(Uuid::from_bytes([0xd0 | n; 16])),
            ..CommandPage::default()
        }),
        handling: None,
    };
    let sampled = Sampled {
        now: Timestamp {
            seconds: i64::from(n),
            nanos: 0,
        },
        command_id: Uuid::from_bytes([0xe0 | n; 16]),
    };
    app.handle(book, &sampled)
        .map(|_| ())
        .map_err(|failure| format!("command {n} ({}) failed: {failure:?}", M::full_name()).into())
}

/// The short type names of a stream's events, and the stream's events.
fn stream(app: &App, cover: &Cover) -> Result<Vec<(String, Vec<u8>)>, Box<dyn std::error::Error>> {
    Ok(app
        .events(cover)?
        .into_iter()
        .filter_map(|page| page.payload)
        .map(|any| {
            let name = type_name(&any.type_url)
                .rsplit('.')
                .next()
                .unwrap_or_default()
                .to_string();
            (name, any.value.to_vec())
        })
        .collect())
}

pub fn drive(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    to_table(
        app,
        1,
        wire::CreateTable {
            name: "Main".into(),
            min_bet: 10,
            max_bet: 100,
            min_buy_in: 100,
            max_buy_in: 1000,
            seats: 3,
            decks: 1,
            shoe_seed: 21,
        },
    )?;
    for (n, player, seat) in [(2, ALICE, 0), (4, BOB, 1)] {
        to_table(
            app,
            n,
            wire::RequestSeat {
                player_root: vec![player; 16],
                seat,
                amount: 500,
                request_id: vec![n; 16],
            },
        )?;
        to_table(
            app,
            n + 1,
            wire::ConfirmSeat {
                buy_in_id: vec![n; 16],
            },
        )?;
    }
    to_table(
        app,
        6,
        wire::PlaceBet {
            seat: 0,
            amount: 20,
        },
    )?;
    to_table(
        app,
        7,
        wire::PlaceBet {
            seat: 1,
            amount: 20,
        },
    )?;
    to_table(app, 8, wire::DealRound {})?;

    for (name, player) in [("alice", ALICE), ("bob", BOB)] {
        let moves: Vec<String> = stream(app, &cover("player", player))?
            .into_iter()
            .filter(|(n, _)| n == "MoveDecided")
            .map(|(_, bytes)| {
                let decided = wire::MoveDecided::decode(&*bytes).expect("decodes");
                format!(
                    "{:?}",
                    wire::MoveAction::try_from(decided.action).expect("an action")
                )
            })
            .collect();
        println!("{name}: {}", moves.join(" "));
    }
    let table = stream(app, &cover("table", TABLE))?;
    let from_deal: Vec<&str> = table
        .iter()
        .map(|(n, _)| n.as_str())
        .skip_while(|n| *n != "RoundDealt")
        .collect();
    println!("table: {}", from_deal.join(" "));
    if let Some((_, bytes)) = table.iter().find(|(n, _)| n == "RoundSettled") {
        let settled = wire::RoundSettled::decode(&**bytes)?;
        for o in &settled.outcomes {
            println!(
                "seat {}: wager {} returned {} stack {}",
                o.seat, o.wager, o.returned, o.stack_after
            );
        }
        println!("house {}", settled.house_delta);
    }
    Ok(())
}
