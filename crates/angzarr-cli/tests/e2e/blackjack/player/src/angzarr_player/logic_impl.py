"""PlayerLogicImpl: a simplified basic strategy, and a count of the
decisions made and of those made on a two-card hand."""

from __future__ import annotations

import dataclasses

from angzarr_player.domain import (
    CommandContext,
    DecideMove,
    MoveAction,
    MoveDecided,
    PlayerState,
)
from angzarr_player.logic import PlayerLogic


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
    ) -> MoveDecided:
        return MoveDecided(
            table_root=cmd.table_root,
            round=cmd.round,
            seat=cmd.seat,
            action=_move(cmd),
            cards=cmd.cards,
            total=cmd.total,
            soft=cmd.soft,
            dealer_up=cmd.dealer_up,
        )

    def apply_move_decided(self, state: PlayerState, event: MoveDecided) -> PlayerState:
        return dataclasses.replace(
            state,
            decisions=state.decisions + 1,
            hands_seen=state.hands_seen + (1 if len(event.cards) == 2 else 0),
        )
