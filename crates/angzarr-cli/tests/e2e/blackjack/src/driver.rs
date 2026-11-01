//! The blackjack driver: seed 21's hand, agg-table's
//! `test_support::seed_21`, the one every driver of the table plays. The
//! driver seats both players, has them bet and deals; the turn then
//! crosses to the Python player and back by the two policies alone, until
//! the round settles. It checks each player's moves and the table's events
//! against seed 21's, and the settlement (`assert_settled`), then prints
//! the settlement.

use agg_table::test_support::seed_21::{self, ALICE, BOB};
use angzarr_embedded::v1::{CommandBook, Cover, Uuid};
use angzarr_embedded::{CommandBookExt, Sampled, Timestamp, pack, type_name};
use angzarr_embedded_host::App;
use prost::{Message, Name};

use crate::wire;

const TABLE: u8 = 0x7a;

fn cover(domain: &str, b: u8) -> Cover {
    Cover {
        domain: domain.to_string(),
        root: Some(Uuid::from_bytes([b; 16])),
        edition: None,
    }
}

async fn to_table<M: Message + Name>(
    app: &App,
    n: u8,
    command: M,
) -> Result<(), Box<dyn std::error::Error>> {
    let book = CommandBook::command(cover("table", TABLE), pack(&command))
        .with_id(Uuid::from_bytes([0xd0 | n; 16]));
    let sampled = Sampled {
        now: Timestamp {
            seconds: i64::from(n),
            nanos: 0,
        },
        command_id: Uuid::from_bytes([0xe0 | n; 16]),
    };
    app.handle(book, &sampled)
        .await
        .map(|_| ())
        .map_err(|failure| format!("command {n} ({}) failed: {failure:?}", M::full_name()).into())
}

/// The short type names of a stream's events, and the stream's events.
async fn stream(
    app: &App,
    cover: &Cover,
) -> Result<Vec<(String, Vec<u8>)>, Box<dyn std::error::Error>> {
    Ok(app
        .events(cover)
        .await?
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

pub async fn drive(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    to_table(app, 1, seed_21::create_table()).await?;
    let mut n = 1;
    for (request, confirm) in seed_21::seats() {
        to_table(app, n + 1, request).await?;
        to_table(app, n + 2, confirm).await?;
        n += 2;
    }
    for bet in seed_21::bets() {
        n += 1;
        to_table(app, n, bet).await?;
    }
    to_table(app, n + 1, wire::DealRound {}).await?;

    for player in [ALICE, BOB] {
        let moves: Vec<wire::MoveAction> = stream(app, &cover("player", player))
            .await?
            .into_iter()
            .filter(|(n, _)| n == "MoveDecided")
            .map(|(_, bytes)| {
                let decided = wire::MoveDecided::decode(&*bytes).expect("decodes");
                decided.action()
            })
            .collect();
        let expected: Vec<wire::MoveAction> = seed_21::moves_of(player)
            .into_iter()
            .map(|(_, action)| action)
            .collect();
        if moves != expected {
            return Err(format!("player {player:#x} moved {moves:?}, not {expected:?}").into());
        }
    }
    let table = stream(app, &cover("table", TABLE)).await?;
    let from_deal: Vec<&str> = table
        .iter()
        .map(|(n, _)| n.as_str())
        .skip_while(|n| *n != "RoundDealt")
        .collect();
    if from_deal != seed_21::FROM_DEAL {
        return Err(format!("the table recorded {from_deal:?} from the deal").into());
    }
    let (_, bytes) = table
        .iter()
        .find(|(n, _)| n == "RoundSettled")
        .ok_or("the round did not settle")?;
    let settled = wire::RoundSettled::decode(&**bytes)?;
    seed_21::assert_settled(&settled);
    for o in &settled.outcomes {
        println!(
            "seat {}: wager {} returned {} stack {}",
            o.seat, o.wager, o.returned, o.stack_after
        );
    }
    println!("house {}", settled.house_delta);
    Ok(())
}
