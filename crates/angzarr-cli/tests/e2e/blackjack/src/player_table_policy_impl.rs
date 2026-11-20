//! PlayerTablePolicyImpl: table-player-policies' PlayerTable, over the
//! domain types the application and the policies share (examples-proto's).
//! A MoveDecided with no move the table knows is no domain value: the
//! generated adapter reports it as the player's bug before this runs. A
//! refused move is the player's RecordMoveRefused (angzarr decision
//! 14.76); a refusal of a table command it never sends is a bug.

use angzarr_embedded::angzarr_diagnostic::code;
use table_player_policies::{PlayerTable, Policies, TableMove};

use crate::domain::*;

type Fact = Result<PlayerAggregateFact, Bug>;

/// The bug of a refusal of `command`, which this policy never sends.
fn never_sent<C>(_command: &C) -> Bug {
    Bug::new(code!("never-sent")).arg("command", std::any::type_name::<C>())
}

/// The player's fact of the refusal of a move it decided.
fn refused(command: TableMove, refusal: Option<TableRefused>) -> Fact {
    Ok(Policies.move_refused(&command, refusal).into())
}

#[derive(Clone, Debug, Default)]
pub struct PlayerTablePolicyImpl;

impl PlayerTablePolicy for PlayerTablePolicyImpl {
    fn move_decided(
        &self,
        _source: &[u8],
        event: &MoveDecided,
        _ctx: &PolicyContext,
    ) -> Result<Vec<(Vec<u8>, TableAggregateOutput)>, Bug> {
        let to = Policies.move_decided(event);
        let command = match to.command {
            TableMove::Hit(c) => TableAggregateOutput::Hit(c),
            TableMove::Stand(c) => TableAggregateOutput::Stand(c),
            TableMove::DoubleDown(c) => TableAggregateOutput::DoubleDown(c),
        };
        Ok(vec![(to.root, command)])
    }

    fn create_table_refused(
        &self,
        command: &CreateTable,
        _refusal: Option<TableRefused>,
        _ctx: &CommandContext,
    ) -> Fact {
        Err(never_sent(command))
    }

    fn request_seat_refused(
        &self,
        command: &RequestSeat,
        _refusal: Option<TableRefused>,
        _ctx: &CommandContext,
    ) -> Fact {
        Err(never_sent(command))
    }

    fn leave_table_refused(
        &self,
        command: &LeaveTable,
        _refusal: Option<TableRefused>,
        _ctx: &CommandContext,
    ) -> Fact {
        Err(never_sent(command))
    }

    fn place_bet_refused(
        &self,
        command: &PlaceBet,
        _refusal: Option<TableRefused>,
        _ctx: &CommandContext,
    ) -> Fact {
        Err(never_sent(command))
    }

    fn deal_round_refused(
        &self,
        command: &DealRound,
        _refusal: Option<TableRefused>,
        _ctx: &CommandContext,
    ) -> Fact {
        Err(never_sent(command))
    }

    fn hit_refused(
        &self,
        command: &Hit,
        refusal: Option<TableRefused>,
        _ctx: &CommandContext,
    ) -> Fact {
        refused(TableMove::Hit(command.clone()), refusal)
    }

    fn stand_refused(
        &self,
        command: &Stand,
        refusal: Option<TableRefused>,
        _ctx: &CommandContext,
    ) -> Fact {
        refused(TableMove::Stand(command.clone()), refusal)
    }

    fn double_down_refused(
        &self,
        command: &DoubleDown,
        refusal: Option<TableRefused>,
        _ctx: &CommandContext,
    ) -> Fact {
        refused(TableMove::DoubleDown(command.clone()), refusal)
    }

    fn confirm_seat_refused(
        &self,
        command: &ConfirmSeat,
        _refusal: Option<SeatNotConfirmable>,
        _ctx: &CommandContext,
    ) -> Fact {
        Err(never_sent(command))
    }
}
