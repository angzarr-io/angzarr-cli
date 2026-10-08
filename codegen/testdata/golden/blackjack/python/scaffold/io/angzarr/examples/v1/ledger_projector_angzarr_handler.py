# Scaffolded ONCE by angzarr codegen python — this file is YOURS.
#
# Regeneration will NOT overwrite this file. It is your responsibility to
# keep the generated <Component>Handler interface implemented: when a
# command or event is added to the proto, the handler will be missing a
# method until you add it here.

from typing import Optional, Protocol

import angzarr_client.router as _az
from angzarr_client.proto.io.angzarr.v1 import types_pb2 as _t
from . import ledger_pb2 as _ledger
from . import player_pb2 as _player
from . import table_pb2 as _table

class LedgerProjector:
    """Implements LedgerProjectorHandler."""

    def player_registered(self, projection: _ledger.LedgerProjection, event: _player.PlayerRegistered, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.player_registered")

    def player_imported(self, projection: _ledger.LedgerProjection, event: _player.PlayerImported, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.player_imported")

    def profile_updated(self, projection: _ledger.LedgerProjection, event: _player.ProfileUpdated, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.profile_updated")

    def funds_deposited(self, projection: _ledger.LedgerProjection, event: _player.FundsDeposited, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.funds_deposited")

    def funds_withdrawn(self, projection: _ledger.LedgerProjection, event: _player.FundsWithdrawn, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.funds_withdrawn")

    def funds_held(self, projection: _ledger.LedgerProjection, event: _player.FundsHeld, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.funds_held")

    def funds_captured(self, projection: _ledger.LedgerProjection, event: _player.FundsCaptured, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.funds_captured")

    def hold_released(self, projection: _ledger.LedgerProjection, event: _player.HoldReleased, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.hold_released")

    def top_up_requested(self, projection: _ledger.LedgerProjection, event: _player.TopUpRequested, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.top_up_requested")

    def top_up_refused(self, projection: _ledger.LedgerProjection, event: _player.TopUpRefused, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.top_up_refused")

    def top_up_settled(self, projection: _ledger.LedgerProjection, event: _player.TopUpSettled, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.top_up_settled")

    def cash_out_credited(self, projection: _ledger.LedgerProjection, event: _player.CashOutCredited, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.cash_out_credited")

    def loyalty_enrolled(self, projection: _ledger.LedgerProjection, event: _player.LoyaltyEnrolled, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.loyalty_enrolled")

    def round_result_recorded(self, projection: _ledger.LedgerProjection, event: _player.RoundResultRecorded, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.round_result_recorded")

    def round_result_retracted(self, projection: _ledger.LedgerProjection, event: _player.RoundResultRetracted, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.round_result_retracted")

    def loyalty_points_awarded(self, projection: _ledger.LedgerProjection, event: _player.LoyaltyPointsAwarded, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.loyalty_points_awarded")

    def table_created(self, projection: _ledger.LedgerProjection, event: _table.TableCreated, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.table_created")

    def player_seated(self, projection: _ledger.LedgerProjection, event: _table.PlayerSeated, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.player_seated")

    def chips_added(self, projection: _ledger.LedgerProjection, event: _table.ChipsAdded, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.chips_added")

    def player_cashed_out(self, projection: _ledger.LedgerProjection, event: _table.PlayerCashedOut, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.player_cashed_out")

    def bet_placed(self, projection: _ledger.LedgerProjection, event: _table.BetPlaced, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.bet_placed")

    def hand_doubled(self, projection: _ledger.LedgerProjection, event: _table.HandDoubled, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.hand_doubled")

    def round_settled(self, projection: _ledger.LedgerProjection, event: _table.RoundSettled, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement LedgerProjector.round_settled")

    def finish(self, projection: _ledger.LedgerProjection, events: _t.EventBook) -> _t.Projection:
        raise NotImplementedError("TODO: implement LedgerProjector.finish")

