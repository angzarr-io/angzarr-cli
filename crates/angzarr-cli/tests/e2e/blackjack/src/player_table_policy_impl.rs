//! PlayerTablePolicyImpl: table-player-policies' PlayerTable, over the
//! domain types.

use examples_proto::v1;
use prost::Message;
use table_player_policies::{NoMove, PlayerTable, Policies, TableMove};

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
        let event = v1::MoveDecided::decode(
            &*crate::wire::MoveDecided::from(event.clone()).encode_to_vec(),
        )
        .expect("one proto");
        // A decision with no move the table knows is a player bug: the
        // policy's error, dead-lettered by its key and args.
        let to = Policies
            .move_decided(&event)
            .map_err(|NoMove::UnknownAction(action)| {
                Bug::new("policy-player-table-unknown-action").arg("action", action)
            })?;
        let command = match to.command {
            TableMove::Hit(c) => TableAggregateCommand::Hit(Hit { seat: c.seat }),
            TableMove::Stand(c) => TableAggregateCommand::Stand(Stand { seat: c.seat }),
            TableMove::DoubleDown(c) => {
                TableAggregateCommand::DoubleDown(DoubleDown { seat: c.seat })
            }
        };
        Ok(vec![(to.root, command)])
    }
}
