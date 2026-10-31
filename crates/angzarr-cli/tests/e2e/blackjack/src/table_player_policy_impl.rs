//! TablePlayerPolicyImpl: table-player-policies' TablePlayer, over the
//! domain types the application and the policies share (examples-proto's).

use table_player_policies::{Policies, TablePlayer};

use crate::domain::*;
use crate::table_player_policy::TablePlayerPolicy;

#[derive(Clone, Debug, Default)]
pub struct TablePlayerPolicyImpl;

impl TablePlayerPolicy for TablePlayerPolicyImpl {
    fn turn_started(
        &self,
        source: &[u8],
        event: &TurnStarted,
        _ctx: &PolicyContext,
    ) -> Result<Vec<(Vec<u8>, PlayerAggregateCommand)>, Bug> {
        let ask = Policies.turn_started(source, event);
        Ok(vec![(
            ask.root,
            PlayerAggregateCommand::DecideMove(ask.command),
        )])
    }
}
