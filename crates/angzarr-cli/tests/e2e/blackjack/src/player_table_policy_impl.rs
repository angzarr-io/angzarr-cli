//! PlayerTablePolicyImpl: table-player-policies' PlayerTable, over the
//! domain types.

use examples_proto::v1;
use prost::Message;
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
    ) -> Vec<(Vec<u8>, TableAggregateCommand)> {
        let event = v1::MoveDecided::decode(
            &*crate::wire::MoveDecided::from(event.clone()).encode_to_vec(),
        )
        .expect("one proto");
        let to = Policies
            .move_decided(&event)
            .expect("the player decides a known move");
        let command = match to.command {
            TableMove::Hit(c) => TableAggregateCommand::Hit(Hit { seat: c.seat }),
            TableMove::Stand(c) => TableAggregateCommand::Stand(Stand { seat: c.seat }),
            TableMove::DoubleDown(c) => {
                TableAggregateCommand::DoubleDown(DoubleDown { seat: c.seat })
            }
        };
        vec![(to.root, command)]
    }
}
