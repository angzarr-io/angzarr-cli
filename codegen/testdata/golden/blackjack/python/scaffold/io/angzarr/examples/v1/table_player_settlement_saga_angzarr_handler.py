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

class TablePlayerSettlementSagaImpl:
    """Implements TablePlayerSettlementSagaHandler."""

    def chips_added(self, event: _table.ChipsAdded, dests: _az.Destinations, source: _az.PageContext) -> tuple[list[_t.CommandBook], list[_t.EventBook]]:
        raise NotImplementedError("TODO: implement TablePlayerSettlementSagaImpl.chips_added")

    def player_cashed_out(self, event: _table.PlayerCashedOut, dests: _az.Destinations, source: _az.PageContext) -> tuple[list[_t.CommandBook], list[_t.EventBook]]:
        raise NotImplementedError("TODO: implement TablePlayerSettlementSagaImpl.player_cashed_out")

