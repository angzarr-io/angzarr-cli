# Scaffolded ONCE by angzarr codegen python — this file is YOURS.
#
# Regeneration will NOT overwrite this file. It is your responsibility to
# keep the generated <Component>Handler interface implemented: when a
# command or event is added to the proto, the handler will be missing a
# method until you add it here.

from typing import Optional, Protocol

import angzarr_client.router as _az
from angzarr_client.proto.io.angzarr.v1 import types_pb2 as _t
from angzarr_client.proto.io.angzarr.v1 import process_manager_pb2 as _pm
from . import buy_in_pb2 as _buy_in
from . import player_pb2 as _player
from . import table_pb2 as _table

class BuyInProcessManager:
    """Implements BuyInProcessManagerHandler."""

    def funds_held(self, event: _player.FundsHeld, state: _buy_in.BuyInState, dests: _az.Destinations, trigger_cover: Optional[_t.Cover]) -> _pm.ProcessManagerHandleResponse:
        raise NotImplementedError("TODO: implement BuyInProcessManager.funds_held")

    def funds_captured(self, event: _player.FundsCaptured, state: _buy_in.BuyInState, dests: _az.Destinations, trigger_cover: Optional[_t.Cover]) -> _pm.ProcessManagerHandleResponse:
        raise NotImplementedError("TODO: implement BuyInProcessManager.funds_captured")

    def seat_held(self, event: _table.SeatHeld, state: _buy_in.BuyInState, dests: _az.Destinations, trigger_cover: Optional[_t.Cover]) -> _pm.ProcessManagerHandleResponse:
        raise NotImplementedError("TODO: implement BuyInProcessManager.seat_held")

    def player_seated(self, event: _table.PlayerSeated, state: _buy_in.BuyInState, dests: _az.Destinations, trigger_cover: Optional[_t.Cover]) -> _pm.ProcessManagerHandleResponse:
        raise NotImplementedError("TODO: implement BuyInProcessManager.player_seated")

    def apply_buy_in_started(self, state: _buy_in.BuyInState, event: _buy_in.BuyInStarted, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement BuyInProcessManager.apply_buy_in_started")

    def apply_buy_in_funds_held(self, state: _buy_in.BuyInState, event: _buy_in.BuyInFundsHeld, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement BuyInProcessManager.apply_buy_in_funds_held")

    def apply_buy_in_seated(self, state: _buy_in.BuyInState, event: _buy_in.BuyInSeated, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement BuyInProcessManager.apply_buy_in_seated")

    def apply_buy_in_completed(self, state: _buy_in.BuyInState, event: _buy_in.BuyInCompleted, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement BuyInProcessManager.apply_buy_in_completed")

    def apply_buy_in_failed(self, state: _buy_in.BuyInState, event: _buy_in.BuyInFailed, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement BuyInProcessManager.apply_buy_in_failed")

    def on_hold_funds_rejected(self, n: _t.Notification, rejection: _t.RejectionNotification, state: _buy_in.BuyInState) -> Optional[_pm.ProcessManagerHandleResponse]:
        raise NotImplementedError("TODO: implement BuyInProcessManager.on_hold_funds_rejected")

    def on_confirm_seat_rejected(self, n: _t.Notification, rejection: _t.RejectionNotification, state: _buy_in.BuyInState) -> Optional[_pm.ProcessManagerHandleResponse]:
        raise NotImplementedError("TODO: implement BuyInProcessManager.on_confirm_seat_rejected")

