//! TableAggregateImpl: the table's logic is agg-table's (`TableLogic` on
//! `Table`, over examples-proto's types); this converts the domain types
//! to those and back, one decision at a time, and agg-table's rejections
//! to the table's typed refusal, TableRefused. A decision of MANY events
//! takes the continuation steps itself, on a working copy.

use agg_table::{AfterAction, Table, TableLogic};
use examples_proto::v1;
use prost::Message;

use crate::domain::*;
use crate::table_aggregate::TableAggregate;

/// A domain value as examples-proto's message of the same proto type.
macro_rules! theirs {
    ($value:expr, $ty:ident) => {
        v1::$ty::decode(&*crate::wire::$ty::from($value.clone()).encode_to_vec())
            .expect("one proto")
    };
}

/// examples-proto's message as the domain value of the same proto type.
macro_rules! ours {
    ($value:expr, $ty:ident) => {
        $ty::try_from(crate::wire::$ty::decode(&*$value.encode_to_vec()).expect("one proto"))
            .expect("a domain value")
    };
}

#[derive(Clone, Debug, Default)]
pub struct TableAggregateImpl {
    pub table: Table,
}

/// agg-table's rejection as the table's typed refusal, TableRefused: its
/// reason code as the Reason, its args as the fields that name them. A
/// code TableRefused does not list is a bug.
fn rejected(rejection: agg_table::Rejection) -> Failed<TableRefused> {
    use TableRefusedReason as R;
    let reason = match rejection.code.reason() {
        "TABLE_ALREADY_EXISTS" => R::TableAlreadyExists,
        "INVALID_TABLE_CONFIG" => R::InvalidTableConfig,
        "TABLE_NOT_FOUND" => R::TableNotFound,
        "SEAT_TAKEN" => R::SeatTaken,
        "PLAYER_ALREADY_SEATED" => R::PlayerAlreadySeated,
        "NOT_SEATED" => R::NotSeated,
        "ROUND_IN_PROGRESS" => R::RoundInProgress,
        "ALREADY_BET" => R::AlreadyBet,
        "INSUFFICIENT_STACK" => R::InsufficientStack,
        "NO_BETS" => R::NoBets,
        "NO_ROUND_IN_PROGRESS" => R::NoRoundInProgress,
        "NOT_YOUR_TURN" => R::NotYourTurn,
        "DOUBLE_NOT_ALLOWED" => R::DoubleNotAllowed,
        "SEAT_OUT_OF_RANGE" => R::SeatOutOfRange,
        "BUY_IN_OUT_OF_RANGE" => R::BuyInOutOfRange,
        "BET_OUT_OF_RANGE" => R::BetOutOfRange,
        "BET_NOT_EVEN" => R::BetNotEven,
        "WAGER_IN_PLAY" => R::WagerInPlay,
        other => {
            return Failed::bug(&Bug::new("examples-table-unknown-reason").arg("reason", other));
        }
    };
    let int = |name: &str| {
        rejection.args.iter().find_map(|(n, v)| match v {
            agg_table::RejectionArg::Int(i) if *n == name => Some(*i),
            _ => None,
        })
    };
    let text = |name: &str| {
        rejection.args.iter().find_map(|(n, v)| match v {
            agg_table::RejectionArg::Str(s) if *n == name => Some(s.clone()),
            _ => None,
        })
    };
    Failed::rejected(TableRefused {
        reason,
        name: text("name"),
        rule: text("rule"),
        seat: int("seat").and_then(|n| i32::try_from(n).ok()),
        min_buy_in: int("min_buy_in"),
        max_buy_in: int("max_buy_in"),
        min_bet: int("min_bet"),
        max_bet: int("max_bet"),
    })
}

impl TableAggregateImpl {
    /// `events` folded into a copy of the table, and the step after an
    /// action decided on it; the dealer running the shoe out is a bug.
    fn after(&self, mut events: Vec<TableAggregateEvent>) -> Result<Vec<TableAggregateEvent>, Bug> {
        let mut working = self.clone();
        for event in &events {
            working.fold(event);
        }
        let after = working.table.after_action().map_err(|ran_out| {
            Bug::new("examples-table-shoe-ran-out").arg("round", ran_out.round)
        })?;
        match after {
            AfterAction::Turn(turn) => {
                events.push(TableAggregateEvent::TurnStarted(ours!(turn, TurnStarted)))
            }
            AfterAction::Settle(dealer, settled) => {
                events.push(TableAggregateEvent::DealerPlayed(ours!(
                    dealer,
                    DealerPlayed
                )));
                events.push(TableAggregateEvent::RoundSettled(ours!(
                    settled,
                    RoundSettled
                )));
            }
            AfterAction::Nothing => {}
        }
        Ok(events)
    }

    fn fold(&mut self, event: &TableAggregateEvent) {
        match event {
            TableAggregateEvent::TableCreated(e) => self.apply_table_created(e),
            TableAggregateEvent::ShoeShuffled(e) => self.apply_shoe_shuffled(e),
            TableAggregateEvent::SeatHeld(e) => self.apply_seat_held(e),
            TableAggregateEvent::SeatReleased(e) => self.apply_seat_released(e),
            TableAggregateEvent::PlayerSeated(e) => self.apply_player_seated(e),
            TableAggregateEvent::PlayerCashedOut(e) => self.apply_player_cashed_out(e),
            TableAggregateEvent::BetPlaced(e) => self.apply_bet_placed(e),
            TableAggregateEvent::RoundDealt(e) => self.apply_round_dealt(e),
            TableAggregateEvent::TurnStarted(e) => self.apply_turn_started(e),
            TableAggregateEvent::CardDealt(e) => self.apply_card_dealt(e),
            TableAggregateEvent::HandStood(e) => self.apply_hand_stood(e),
            TableAggregateEvent::HandDoubled(e) => self.apply_hand_doubled(e),
            TableAggregateEvent::DealerPlayed(e) => self.apply_dealer_played(e),
            TableAggregateEvent::RoundSettled(e) => self.apply_round_settled(e),
        }
    }
}

impl TableAggregate for TableAggregateImpl {
    fn create_table(
        &self,
        cmd: &CreateTable,
        _ctx: &Context,
    ) -> Result<(TableCreated, ShoeShuffled), Failed<TableRefused>> {
        let (created, shuffled) = self
            .table
            .create_table(&theirs!(cmd, CreateTable))
            .map_err(rejected)?;
        Ok((ours!(created, TableCreated), ours!(shuffled, ShoeShuffled)))
    }

    fn request_seat(&self, cmd: &RequestSeat, _ctx: &Context) -> Result<Option<SeatHeld>, Failed<TableRefused>> {
        let held = self
            .table
            .request_seat(&theirs!(cmd, RequestSeat))
            .map_err(rejected)?;
        Ok(held.map(|h| ours!(h, SeatHeld)))
    }

    fn leave_table(&self, cmd: &LeaveTable, ctx: &Context) -> Result<PlayerCashedOut, Failed<TableRefused>> {
        // agg-table's leave_table needs the table's stream root: the
        // context's cover names it.
        let root: [u8; 16] = ctx
            .command
            .cover
            .as_ref()
            .and_then(|cover| cover.root.as_ref())
            .and_then(|root| root.value[..].try_into().ok())
            .unwrap_or_default();
        let out = self
            .table
            .leave_table(&theirs!(cmd, LeaveTable), &root)
            .map_err(rejected)?;
        Ok(ours!(out, PlayerCashedOut))
    }

    fn place_bet(&self, cmd: &PlaceBet, _ctx: &Context) -> Result<BetPlaced, Failed<TableRefused>> {
        let bet = self
            .table
            .place_bet(&theirs!(cmd, PlaceBet))
            .map_err(rejected)?;
        Ok(ours!(bet, BetPlaced))
    }

    fn deal_round(&self, cmd: &DealRound, _ctx: &Context) -> Result<Vec<TableAggregateEvent>, Failed<TableRefused>> {
        let mut events = Vec::new();
        if let Some(shuffled) = self
            .table
            .deal_round(&theirs!(cmd, DealRound))
            .map_err(rejected)?
        {
            events.push(TableAggregateEvent::ShoeShuffled(ours!(
                shuffled,
                ShoeShuffled
            )));
        }
        let mut working = self.clone();
        for event in &events {
            working.fold(event);
        }
        if let Some(dealt) = working.table.deal().map_err(rejected)? {
            events.push(TableAggregateEvent::RoundDealt(ours!(dealt, RoundDealt)));
        }
        Ok(self.after(events)?)
    }

    fn hit(&self, cmd: &Hit, _ctx: &Context) -> Result<Vec<TableAggregateEvent>, Failed<TableRefused>> {
        let card = self.table.hit(&theirs!(cmd, Hit)).map_err(rejected)?;
        Ok(self.after(vec![TableAggregateEvent::CardDealt(ours!(card, CardDealt))])?)
    }

    fn stand(&self, cmd: &Stand, _ctx: &Context) -> Result<Vec<TableAggregateEvent>, Failed<TableRefused>> {
        let stood = self.table.stand(&theirs!(cmd, Stand)).map_err(rejected)?;
        Ok(self.after(vec![TableAggregateEvent::HandStood(ours!(
            stood, HandStood
        ))])?)
    }

    fn double_down(&self, cmd: &DoubleDown, _ctx: &Context) -> Result<Vec<TableAggregateEvent>, Failed<TableRefused>> {
        let doubled = self
            .table
            .double_down(&theirs!(cmd, DoubleDown))
            .map_err(rejected)?;
        Ok(self.after(vec![TableAggregateEvent::HandDoubled(ours!(
            doubled,
            HandDoubled
        ))])?)
    }

    fn confirm_seat(&self, cmd: &ConfirmSeat, _ctx: &Context) -> Result<Option<PlayerSeated>, Failed<SeatNotConfirmable>> {
        match self.table.confirm_seat(&theirs!(cmd, ConfirmSeat)) {
            Ok(seated) => Ok(seated.map(|s| ours!(s, PlayerSeated))),
            Err(refusal) => Err(ours!(refusal, SeatNotConfirmable).into()),
        }
    }

    fn release_seat(&self, cmd: &ReleaseSeat, _ctx: &Context) -> Result<Option<SeatReleased>, Failed<Rejection>> {
        Ok(self
            .table
            .release_seat(&theirs!(cmd, ReleaseSeat))
            .map(|r| ours!(r, SeatReleased)))
    }

    fn apply_table_created(&mut self, event: &TableCreated) {
        self.table
            .apply_table_created(&theirs!(event, TableCreated));
    }

    fn apply_shoe_shuffled(&mut self, event: &ShoeShuffled) {
        self.table
            .apply_shoe_shuffled(&theirs!(event, ShoeShuffled));
    }

    fn apply_seat_held(&mut self, event: &SeatHeld) {
        self.table.apply_seat_held(&theirs!(event, SeatHeld));
    }

    fn apply_seat_released(&mut self, event: &SeatReleased) {
        self.table
            .apply_seat_released(&theirs!(event, SeatReleased));
    }

    fn apply_player_seated(&mut self, event: &PlayerSeated) {
        self.table
            .apply_player_seated(&theirs!(event, PlayerSeated));
    }

    fn apply_player_cashed_out(&mut self, event: &PlayerCashedOut) {
        self.table
            .apply_player_cashed_out(&theirs!(event, PlayerCashedOut));
    }

    fn apply_bet_placed(&mut self, event: &BetPlaced) {
        self.table.apply_bet_placed(&theirs!(event, BetPlaced));
    }

    fn apply_round_dealt(&mut self, event: &RoundDealt) {
        self.table.apply_round_dealt(&theirs!(event, RoundDealt));
    }

    fn apply_turn_started(&mut self, event: &TurnStarted) {
        self.table.apply_turn_started(&theirs!(event, TurnStarted));
    }

    fn apply_card_dealt(&mut self, event: &CardDealt) {
        self.table.apply_card_dealt(&theirs!(event, CardDealt));
    }

    fn apply_hand_stood(&mut self, event: &HandStood) {
        self.table.apply_hand_stood(&theirs!(event, HandStood));
    }

    fn apply_hand_doubled(&mut self, event: &HandDoubled) {
        self.table.apply_hand_doubled(&theirs!(event, HandDoubled));
    }

    fn apply_dealer_played(&mut self, event: &DealerPlayed) {
        self.table
            .apply_dealer_played(&theirs!(event, DealerPlayed));
    }

    fn apply_round_settled(&mut self, event: &RoundSettled) {
        self.table
            .apply_round_settled(&theirs!(event, RoundSettled));
    }
}
