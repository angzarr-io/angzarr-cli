//! TableAggregate: the interface of `io.angzarr.examples.v1.TableState`, written once by
//! `angzarr scaffold rust` and yours from then on: `--missing` adds the
//! methods it lacks. It names only `crate::domain`'s types, its handlers'
//! `Context` among them; the generated adapter (`crate::adapters::TableAggregate`)
//! carries the markup and calls it.
//!
//! Edited: RequestSeat, ConfirmSeat and ReleaseSeat emit nothing when
//! repeated, so each returns an `Option`.

use crate::domain::*;

pub trait TableAggregate {
    fn create_table(&self, cmd: &CreateTable, ctx: &Context) -> Result<Vec<TableAggregateEvent>, Failed<Rejection>>;

    fn request_seat(&self, cmd: &RequestSeat, ctx: &Context) -> Result<Option<SeatHeld>, Failed<Rejection>>;

    fn leave_table(&self, cmd: &LeaveTable, ctx: &Context) -> Result<PlayerCashedOut, Failed<Rejection>>;

    fn place_bet(&self, cmd: &PlaceBet, ctx: &Context) -> Result<BetPlaced, Failed<Rejection>>;

    fn deal_round(&self, cmd: &DealRound, ctx: &Context) -> Result<Vec<TableAggregateEvent>, Failed<Rejection>>;

    fn hit(&self, cmd: &Hit, ctx: &Context) -> Result<Vec<TableAggregateEvent>, Failed<Rejection>>;

    fn stand(&self, cmd: &Stand, ctx: &Context) -> Result<Vec<TableAggregateEvent>, Failed<Rejection>>;

    fn double_down(&self, cmd: &DoubleDown, ctx: &Context) -> Result<Vec<TableAggregateEvent>, Failed<Rejection>>;

    fn confirm_seat(&self, cmd: &ConfirmSeat, ctx: &Context) -> Result<Option<PlayerSeated>, Failed<SeatNotConfirmable>>;

    fn release_seat(&self, cmd: &ReleaseSeat, ctx: &Context) -> Result<Option<SeatReleased>, Failed<Rejection>>;

    fn apply_table_created(&mut self, event: &TableCreated);

    fn apply_shoe_shuffled(&mut self, event: &ShoeShuffled);

    fn apply_seat_held(&mut self, event: &SeatHeld);

    fn apply_seat_released(&mut self, event: &SeatReleased);

    fn apply_player_seated(&mut self, event: &PlayerSeated);

    fn apply_player_cashed_out(&mut self, event: &PlayerCashedOut);

    fn apply_bet_placed(&mut self, event: &BetPlaced);

    fn apply_round_dealt(&mut self, event: &RoundDealt);

    fn apply_turn_started(&mut self, event: &TurnStarted);

    fn apply_card_dealt(&mut self, event: &CardDealt);

    fn apply_hand_stood(&mut self, event: &HandStood);

    fn apply_hand_doubled(&mut self, event: &HandDoubled);

    fn apply_dealer_played(&mut self, event: &DealerPlayed);

    fn apply_round_settled(&mut self, event: &RoundSettled);
}
