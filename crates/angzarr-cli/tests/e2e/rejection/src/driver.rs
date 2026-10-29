//! The rejection driver: one DecideMove to a player whose Python decision
//! raises the generated `Rejection("some-reason-key", amount=13)`. It
//! prints how the refusal reaches the host, one line per fact the test
//! checks: through `App::handle`, the kernel's failure and its recovery;
//! straight from the child's handler, the router's gRPC class, code, args
//! and whether it is retryable; then one DecideMove for a total of 99, on
//! which the decision raises the generated `Bug("some-bug-key",
//! amount=7)`, through `App::handle`; and what the player's stream holds
//! after.

use angzarr_embedded::next::angzarr_router::pb;
use angzarr_embedded::next::angzarr_router::router::CommandHandler;
use angzarr_embedded::next::v1::{Command, CommandBook, CommandPage, Cover, Uuid, command_page};
use angzarr_embedded::next::{Failure, KernelError, Sampled, Timestamp, pack};
use angzarr_embedded_diagnostic::{Arg, Diagnostic};
use angzarr_embedded_host::{App, is_retryable};
use prost::Message;

use crate::wire;

const PLAYER: u8 = 0xa1;

fn cover() -> Cover {
    Cover {
        domain: "player".to_string(),
        root: Some(Uuid::from_bytes([PLAYER; 16])),
        edition: None,
    }
}

fn decide_move(total: u32) -> wire::DecideMove {
    wire::DecideMove {
        table_root: vec![0x7a; 16],
        round: 1,
        seat: 0,
        total,
        ..wire::DecideMove::default()
    }
}

fn book() -> CommandBook {
    book_of(13, 0xd1)
}

fn book_of(total: u32, id: u8) -> CommandBook {
    CommandBook {
        cover: Some(cover()),
        page: Some(CommandPage {
            body: Some(command_page::Body::Command(Command {
                payload: Some(pack(&decide_move(total))),
            })),
            command_id: Some(Uuid::from_bytes([id; 16])),
            ..CommandPage::default()
        }),
        handling: None,
    }
}

fn args(args: Vec<(&'static str, Arg)>) -> String {
    args.into_iter()
        .map(|(name, arg)| match arg {
            Arg::Str(s) => format!("{name}={s}"),
            Arg::Int(n) => format!("{name}={n}"),
            Arg::Uint(n) => format!("{name}={n}"),
            Arg::Nested(cause) => format!("{name}={}", cause.code()),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn drive(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    let sampled = Sampled {
        now: Timestamp {
            seconds: 1,
            nanos: 0,
        },
        command_id: Uuid::from_bytes([0xe1; 16]),
    };
    match app.handle(book(), &sampled) {
        Ok(_) => println!("handle: accepted"),
        Err(Failure::Rejected(rejected)) => println!(
            "handle: rejected {} {} recovery {:?}",
            rejected.code(),
            args(rejected.args()),
            Failure::Rejected(rejected.clone()).recovery()
        ),
        Err(other) => println!("handle: failed {:?} {other:?}", other.recovery()),
    }

    // The same command straight to the child's handler: the gRPC status it
    // answered, in the router's terms.
    let handler = app.host().handler("player").ok_or("no player handler")?;
    let request = pb::ContextualCommand {
        events: Some(pb::EventBook {
            cover: Some(pb::Cover::decode(&*cover().encode_to_vec())?),
            next_sequence: 0,
            ..pb::EventBook::default()
        }),
        command: Some(pb::CommandBook::decode(&*book().encode_to_vec())?),
        ..pb::ContextualCommand::default()
    };
    match handler.dispatch(&request) {
        Ok(response) => println!("dispatch: accepted {response:?}"),
        Err(error) => println!(
            "dispatch: {:?} {} {} retryable {}",
            error.grpc,
            error.code,
            error
                .extras
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(" "),
            is_retryable(&error)
        ),
    }

    // A total of 99: the decision's Bug, a defect, never a refusal.
    let sampled = Sampled {
        command_id: Uuid::from_bytes([0xe2; 16]),
        ..sampled
    };
    match app.handle(book_of(99, 0xd2), &sampled) {
        Err(Failure::Kernel(error @ KernelError::Router(_))) => {
            let recovery = error.recovery();
            let KernelError::Router(error) = error else {
                unreachable!("matched above")
            };
            let extras: Vec<String> = error
                .extras
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            println!(
                "bug: {:?} {} {} recovery {recovery:?}",
                error.grpc,
                error.code,
                extras.join(" ")
            );
        }
        Ok(_) => println!("bug: accepted"),
        Err(other) => println!("bug: failed {:?} {other:?}", other.recovery()),
    }

    println!("player events: {}", app.events(&cover())?.len());
    Ok(())
}
