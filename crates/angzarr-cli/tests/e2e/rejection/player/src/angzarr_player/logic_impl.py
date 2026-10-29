"""PlayerLogicImpl: a player that refuses every move with the generated
``Rejection`` (a reason key and its args), but for a total of 99, which it
fails on with the generated ``Bug``: a defect, its key and args the
cause."""

from __future__ import annotations

from angzarr_player.domain import (
    Bug,
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
        if cmd.total == 99:
            raise Bug("some-bug-key", amount=7)
        raise Rejection("some-reason-key", amount=13)

    def apply_move_decided(self, state: PlayerState, event: MoveDecided) -> PlayerState:
        return state
