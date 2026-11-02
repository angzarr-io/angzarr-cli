//! TablePlayerPolicyImpl: table-player-policies' TablePlayer, over the
//! domain types the application and the policies share (examples-proto's).
//! A refused DecideMove is the table's RecordMoveUndecided (angzarr
//! decision 14.76).

use table_player_policies::{Policies, TablePlayer};

use crate::domain::*;

#[derive(Clone, Debug, Default)]
pub struct TablePlayerPolicyImpl;

impl TablePlayerPolicy for TablePlayerPolicyImpl {
    fn turn_started(
        &self,
        source: &[u8],
        event: &TurnStarted,
        _ctx: &PolicyContext,
    ) -> Result<Vec<(Vec<u8>, PlayerAggregateOutput)>, Bug> {
        let ask = Policies.turn_started(source, event);
        Ok(vec![(
            ask.root,
            PlayerAggregateOutput::DecideMove(ask.command),
        )])
    }

    fn decide_move_refused(
        &self,
        command: &DecideMove,
        refusal: Option<DecideMoveRefused>,
        _ctx: &CommandContext,
    ) -> Result<TableAggregateFact, Bug> {
        Ok(Policies.decide_move_refused(command, refusal).into())
    }
}
