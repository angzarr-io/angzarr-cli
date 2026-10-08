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
from . import counter_pb2 as _counter

class AuditProcessManager:
    """Implements AuditProcessManagerHandler."""

    def increased(self, event: _counter.Increased, state: _counter.AuditProcessManagerState, dests: _az.Destinations, trigger_cover: Optional[_t.Cover]) -> _pm.ProcessManagerHandleResponse:
        raise NotImplementedError("TODO: implement AuditProcessManager.increased")

    def apply_increased(self, state: _counter.AuditProcessManagerState, event: _counter.Increased, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement AuditProcessManager.apply_increased")

    def on_reserve_rejected(self, n: _t.Notification, rejection: _t.RejectionNotification, state: _counter.AuditProcessManagerState) -> Optional[_pm.ProcessManagerHandleResponse]:
        raise NotImplementedError("TODO: implement AuditProcessManager.on_reserve_rejected")

