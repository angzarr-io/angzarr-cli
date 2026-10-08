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

class OrderSagaImpl:
    """Implements OrderSagaHandler."""

    def increased(self, event: _counter.Increased, dests: _az.Destinations, source: _az.PageContext) -> tuple[list[_t.CommandBook], list[_t.EventBook]]:
        raise NotImplementedError("TODO: implement OrderSagaImpl.increased")

