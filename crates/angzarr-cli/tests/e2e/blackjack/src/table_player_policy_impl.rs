//! TablePlayerPolicyImpl: table-player-policies' TablePlayer, over the
//! domain types.

use examples_proto::v1;
use prost::Message;
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
        let event = v1::TurnStarted::decode(
            &*crate::wire::TurnStarted::from(event.clone()).encode_to_vec(),
        )
        .expect("one proto");
        let ask = Policies.turn_started(source, &event);
        let command = DecideMove::try_from(
            crate::wire::DecideMove::decode(&*ask.command.encode_to_vec()).expect("one proto"),
        )
        .expect("a domain value");
        Ok(vec![(ask.root, PlayerAggregateCommand::DecideMove(command))])
    }
}
