"""PlayerLogicImpl: a simplified basic strategy, and a count of the
decisions made and of those made on a two-card hand."""

from __future__ import annotations

import dataclasses

from ._gen.domain import (
    CommandContext,
    DecideMove,
    DecideMoveRefused,
    MoveAction,
    MoveDecided,
    MoveRefused,
    PlayerState,
    RecordMoveRefused,
)
from ._gen.logic import PlayerLogic


def _move(cmd: DecideMove) -> MoveAction:
    up = cmd.dealer_up.rank if cmd.dealer_up is not None else 0
    if cmd.soft:
        return MoveAction.STAND if cmd.total >= 18 else MoveAction.HIT
    if cmd.total in (10, 11):
        return MoveAction.DOUBLE_DOWN if cmd.can_double else MoveAction.HIT
    if cmd.total <= 11:
        return MoveAction.HIT
    if cmd.total == 12:
        return MoveAction.STAND if 4 <= up <= 6 else MoveAction.HIT
    if cmd.total <= 16:
        return MoveAction.STAND if 2 <= up <= 6 else MoveAction.HIT
    return MoveAction.STAND


class PlayerLogicImpl(PlayerLogic):
    def decide_move(
        self, cmd: DecideMove, state: PlayerState, ctx: CommandContext
    ) -> list[MoveDecided] | DecideMoveRefused:
        return [MoveDecided.from_command(cmd, action=_move(cmd))]

    def record_move_refused(
        self, fact: RecordMoveRefused, state: PlayerState, ctx: CommandContext
    ) -> list[MoveRefused]:
        return [MoveRefused(seat=fact.seat, action=fact.action, reason=fact.reason)]

    def apply_move_refused(self, state: PlayerState, event: MoveRefused) -> PlayerState:
        return state

    def apply_move_decided(self, state: PlayerState, event: MoveDecided) -> PlayerState:
        return dataclasses.replace(
            state,
            decisions=state.decisions + 1,
            hands_seen=state.hands_seen + (1 if len(event.cards) == 2 else 0),
        )
