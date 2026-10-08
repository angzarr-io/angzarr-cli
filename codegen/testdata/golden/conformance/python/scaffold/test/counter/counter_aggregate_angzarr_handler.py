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
from . import counter_pb2 as _counter

class CounterAggregate:
    """Implements CounterAggregateHandler."""

    def increase_by(self, cmd: _counter.IncreaseBy, state: _counter.CounterState, cctx: _az.CommandContext) -> list[_counter.Increased]:
        raise NotImplementedError("TODO: implement CounterAggregate.increase_by")

    def fail_hard(self, cmd: _counter.FailHard, state: _counter.CounterState, cctx: _az.CommandContext) -> Optional[_t.EventBook]:
        raise NotImplementedError("TODO: implement CounterAggregate.fail_hard")

    def apply_increased(self, state: _counter.CounterState, event: _counter.Increased, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement CounterAggregate.apply_increased")

    def on_reserve_rejected(self, n: _t.Notification, rejection: _t.RejectionNotification, state: _counter.CounterState, cctx: _az.CommandContext) -> Optional[_ch.BusinessResponse]:
        raise NotImplementedError("TODO: implement CounterAggregate.on_reserve_rejected")

