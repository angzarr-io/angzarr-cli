# Scaffolded ONCE by angzarr codegen python — this file is YOURS.
#
# Regeneration will NOT overwrite this file. It is your responsibility to
# keep the generated <Component>Handler interface implemented: when a
# command or event is added to the proto, the handler will be missing a
# method until you add it here.

from typing import Optional, Protocol

import angzarr_client.router as _az
from angzarr_client.proto.io.angzarr.v1 import types_pb2 as _t
from angzarr_client.proto.io.angzarr.v1 import command_handler_pb2 as _ch
from . import player_pb2 as _player

class PlayerAggregate:
    """Implements PlayerAggregateHandler."""

    def register_player(self, cmd: _player.RegisterPlayer, state: _player.PlayerState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement PlayerAggregate.register_player")

    def import_player(self, cmd: _player.ImportPlayer, state: _player.PlayerState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement PlayerAggregate.import_player")

    def update_profile(self, cmd: _player.UpdateProfile, state: _player.PlayerState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement PlayerAggregate.update_profile")

    def deposit_funds(self, cmd: _player.DepositFunds, state: _player.PlayerState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement PlayerAggregate.deposit_funds")

    def withdraw_funds(self, cmd: _player.WithdrawFunds, state: _player.PlayerState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement PlayerAggregate.withdraw_funds")

    def request_top_up(self, cmd: _player.RequestTopUp, state: _player.PlayerState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement PlayerAggregate.request_top_up")

    def enroll_loyalty(self, cmd: _player.EnrollLoyalty, state: _player.PlayerState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement PlayerAggregate.enroll_loyalty")

    def hold_funds(self, cmd: _player.HoldFunds, state: _player.PlayerState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement PlayerAggregate.hold_funds")

    def capture_funds(self, cmd: _player.CaptureFunds, state: _player.PlayerState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement PlayerAggregate.capture_funds")

    def release_hold(self, cmd: _player.ReleaseHold, state: _player.PlayerState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement PlayerAggregate.release_hold")

    def record_round_result(self, cmd: _player.RecordRoundResult, state: _player.PlayerState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement PlayerAggregate.record_round_result")

    def award_loyalty_points(self, cmd: _player.AwardLoyaltyPoints, state: _player.PlayerState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement PlayerAggregate.award_loyalty_points")

    def apply_player_registered(self, state: _player.PlayerState, event: _player.PlayerRegistered, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement PlayerAggregate.apply_player_registered")

    def apply_player_imported(self, state: _player.PlayerState, event: _player.PlayerImported, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement PlayerAggregate.apply_player_imported")

    def apply_profile_updated(self, state: _player.PlayerState, event: _player.ProfileUpdated, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement PlayerAggregate.apply_profile_updated")

    def apply_funds_deposited(self, state: _player.PlayerState, event: _player.FundsDeposited, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement PlayerAggregate.apply_funds_deposited")

    def apply_funds_withdrawn(self, state: _player.PlayerState, event: _player.FundsWithdrawn, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement PlayerAggregate.apply_funds_withdrawn")

    def apply_funds_held(self, state: _player.PlayerState, event: _player.FundsHeld, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement PlayerAggregate.apply_funds_held")

    def apply_funds_captured(self, state: _player.PlayerState, event: _player.FundsCaptured, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement PlayerAggregate.apply_funds_captured")

    def apply_hold_released(self, state: _player.PlayerState, event: _player.HoldReleased, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement PlayerAggregate.apply_hold_released")

    def apply_top_up_requested(self, state: _player.PlayerState, event: _player.TopUpRequested, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement PlayerAggregate.apply_top_up_requested")

    def apply_top_up_refused(self, state: _player.PlayerState, event: _player.TopUpRefused, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement PlayerAggregate.apply_top_up_refused")

    def apply_top_up_settled(self, state: _player.PlayerState, event: _player.TopUpSettled, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement PlayerAggregate.apply_top_up_settled")

    def apply_cash_out_credited(self, state: _player.PlayerState, event: _player.CashOutCredited, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement PlayerAggregate.apply_cash_out_credited")

    def apply_loyalty_enrolled(self, state: _player.PlayerState, event: _player.LoyaltyEnrolled, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement PlayerAggregate.apply_loyalty_enrolled")

    def apply_round_result_recorded(self, state: _player.PlayerState, event: _player.RoundResultRecorded, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement PlayerAggregate.apply_round_result_recorded")

    def apply_round_result_retracted(self, state: _player.PlayerState, event: _player.RoundResultRetracted, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement PlayerAggregate.apply_round_result_retracted")

    def apply_loyalty_points_awarded(self, state: _player.PlayerState, event: _player.LoyaltyPointsAwarded, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement PlayerAggregate.apply_loyalty_points_awarded")

    def on_add_chips_rejected(self, n: _t.Notification, rejection: _t.RejectionNotification, state: _player.PlayerState, cctx: _az.CommandContext) -> Optional[_ch.BusinessResponse]:
        raise NotImplementedError("TODO: implement PlayerAggregate.on_add_chips_rejected")

    def on_top_up_settled_fact(self, fact: _player.TopUpSettled, state: _player.PlayerState) -> Optional[_az.FactRecord]:
        raise NotImplementedError("TODO: implement PlayerAggregate.on_top_up_settled_fact")

    def on_cash_out_credited_fact(self, fact: _player.CashOutCredited, state: _player.PlayerState) -> Optional[_az.FactRecord]:
        raise NotImplementedError("TODO: implement PlayerAggregate.on_cash_out_credited_fact")

    def on_record_round_result_undo(self, n: _t.Notification, compensate: _t.Compensate, state: _player.PlayerState, cctx: _az.CommandContext) -> Optional[_ch.BusinessResponse]:
        raise NotImplementedError("TODO: implement PlayerAggregate.on_record_round_result_undo")

