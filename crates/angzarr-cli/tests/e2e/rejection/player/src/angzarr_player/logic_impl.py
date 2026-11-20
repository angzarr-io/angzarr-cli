"""PlayerLogicImpl: a player that refuses every move with the generated
``Rejection`` (a reason key and its args), but for a total of 99, which it
fails on with the generated ``Bug``: a defect, its key and args the
cause."""

from __future__ import annotations

from ._gen.domain import (
    Bug,
    CommandContext,
    DecideMove,
    DecideMoveRefused,
    MoveDecided,
    MoveRefused,
    PlayerState,
    RecordMoveRefused,
    Rejection,
)
from ._gen.logic import PlayerLogic


class PlayerLogicImpl(PlayerLogic):
    def decide_move(
        self, cmd: DecideMove, state: PlayerState, ctx: CommandContext
    ) -> list[MoveDecided] | DecideMoveRefused:
        if cmd.total == 99:
            raise Bug("some-bug-key", amount=7)
        raise Rejection("some-reason-key", amount=13)

    def record_move_refused(
        self, fact: RecordMoveRefused, state: PlayerState, ctx: CommandContext
    ) -> list[MoveRefused]:
        return []

    def apply_move_decided(self, state: PlayerState, event: MoveDecided) -> PlayerState:
        return state

    def apply_move_refused(self, state: PlayerState, event: MoveRefused) -> PlayerState:
        return state
