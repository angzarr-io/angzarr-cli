# Scaffolded ONCE by angzarr codegen python — this file is YOURS.
#
# Regeneration will NOT overwrite this file. It is your responsibility to
# keep the generated <Component>Handler interface implemented: when a
# command or event is added to the proto, the handler will be missing a
# method until you add it here.

from typing import Optional, Protocol

import angzarr_client.router as _az
from angzarr_client.proto.io.angzarr.v1 import types_pb2 as _t
from . import table_pb2 as _table

class TableAggregate:
    """Implements TableAggregateHandler."""

    def create_table(self, cmd: _table.CreateTable, state: _table.TableState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement TableAggregate.create_table")

    def request_seat(self, cmd: _table.RequestSeat, state: _table.TableState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement TableAggregate.request_seat")

    def leave_table(self, cmd: _table.LeaveTable, state: _table.TableState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement TableAggregate.leave_table")

    def place_bet(self, cmd: _table.PlaceBet, state: _table.TableState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement TableAggregate.place_bet")

    def deal_round(self, cmd: _table.DealRound, state: _table.TableState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement TableAggregate.deal_round")

    def hit(self, cmd: _table.Hit, state: _table.TableState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement TableAggregate.hit")

    def stand(self, cmd: _table.Stand, state: _table.TableState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement TableAggregate.stand")

    def double_down(self, cmd: _table.DoubleDown, state: _table.TableState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement TableAggregate.double_down")

    def confirm_seat(self, cmd: _table.ConfirmSeat, state: _table.TableState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement TableAggregate.confirm_seat")

    def release_seat(self, cmd: _table.ReleaseSeat, state: _table.TableState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement TableAggregate.release_seat")

    def add_chips(self, cmd: _table.AddChips, state: _table.TableState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement TableAggregate.add_chips")

    def apply_table_created(self, state: _table.TableState, event: _table.TableCreated, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement TableAggregate.apply_table_created")

    def apply_shoe_shuffled(self, state: _table.TableState, event: _table.ShoeShuffled, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement TableAggregate.apply_shoe_shuffled")

    def apply_seat_held(self, state: _table.TableState, event: _table.SeatHeld, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement TableAggregate.apply_seat_held")

    def apply_seat_released(self, state: _table.TableState, event: _table.SeatReleased, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement TableAggregate.apply_seat_released")

    def apply_player_seated(self, state: _table.TableState, event: _table.PlayerSeated, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement TableAggregate.apply_player_seated")

    def apply_chips_added(self, state: _table.TableState, event: _table.ChipsAdded, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement TableAggregate.apply_chips_added")

    def apply_player_cashed_out(self, state: _table.TableState, event: _table.PlayerCashedOut, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement TableAggregate.apply_player_cashed_out")

    def apply_bet_placed(self, state: _table.TableState, event: _table.BetPlaced, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement TableAggregate.apply_bet_placed")

    def apply_round_dealt(self, state: _table.TableState, event: _table.RoundDealt, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement TableAggregate.apply_round_dealt")

    def apply_card_dealt(self, state: _table.TableState, event: _table.CardDealt, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement TableAggregate.apply_card_dealt")

    def apply_hand_stood(self, state: _table.TableState, event: _table.HandStood, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement TableAggregate.apply_hand_stood")

    def apply_hand_doubled(self, state: _table.TableState, event: _table.HandDoubled, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement TableAggregate.apply_hand_doubled")

    def apply_dealer_played(self, state: _table.TableState, event: _table.DealerPlayed, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement TableAggregate.apply_dealer_played")

    def apply_round_settled(self, state: _table.TableState, event: _table.RoundSettled, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement TableAggregate.apply_round_settled")

