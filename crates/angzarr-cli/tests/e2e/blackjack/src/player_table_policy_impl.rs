//! PlayerTablePolicyImpl: table-player-policies' PlayerTable, over the
//! domain types the application and the policies share (examples-proto's).
//! A MoveDecided with no move the table knows is no domain value: the
//! generated adapter reports it as the player's bug before this runs.

use table_player_policies::{PlayerTable, Policies, TableMove};

use crate::domain::*;
use crate::player_table_policy::PlayerTablePolicy;

#[derive(Clone, Debug, Default)]
pub struct PlayerTablePolicyImpl;

impl PlayerTablePolicy for PlayerTablePolicyImpl {
    fn move_decided(
        &self,
        _source: &[u8],
        event: &MoveDecided,
        _ctx: &PolicyContext,
    ) -> Result<Vec<(Vec<u8>, TableAggregateCommand)>, Bug> {
        let to = Policies.move_decided(event);
        let command = match to.command {
            TableMove::Hit(c) => TableAggregateCommand::Hit(c),
            TableMove::Stand(c) => TableAggregateCommand::Stand(c),
            TableMove::DoubleDown(c) => TableAggregateCommand::DoubleDown(c),
        };
        Ok(vec![(to.root, command)])
    }
}
