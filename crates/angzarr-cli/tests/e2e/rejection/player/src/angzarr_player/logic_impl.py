"""PlayerLogicImpl: a player that refuses every move, with the generated
``Rejection``: a reason key and its args."""

from __future__ import annotations

from angzarr_player.domain import (
    CommandContext,
    DecideMove,
    MoveDecided,
    PlayerState,
    Rejection,
)
from angzarr_player.logic import PlayerLogic


class PlayerLogicImpl(PlayerLogic):
    def decide_move(
        self, cmd: DecideMove, state: PlayerState, ctx: CommandContext
    ) -> MoveDecided:
        raise Rejection("some-reason-key", amount=13)

    def apply_move_decided(self, state: PlayerState, event: MoveDecided) -> PlayerState:
        return state
