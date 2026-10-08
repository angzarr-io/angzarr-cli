# Scaffolded ONCE by angzarr codegen python — this file is YOURS.
#
# Regeneration will NOT overwrite this file. It is your responsibility to
# keep the generated <Component>Handler interface implemented: when a
# command or event is added to the proto, the handler will be missing a
# method until you add it here.

from typing import Optional, Protocol

import angzarr_client.router as _az
from angzarr_client.proto.io.angzarr.v1 import types_pb2 as _t
from . import counter_pb2 as _counter

class CounterProjector:
    """Implements CounterProjectorHandler."""

    def increased(self, projection: _counter.CounterProjectorState, event: _counter.Increased, ctx: _az.PageContext) -> None:
        raise NotImplementedError("TODO: implement CounterProjector.increased")

    def finish(self, projection: _counter.CounterProjectorState, events: _t.EventBook) -> _t.Projection:
        raise NotImplementedError("TODO: implement CounterProjector.finish")

