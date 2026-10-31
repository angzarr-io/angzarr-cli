"""The table's generated project under the real angzarr-client: a decision
returning a typed refusal is answered as the client's Refused, the refusal
on the wire, its Reason the rejection code, under the code's status
(decision 14.52: FAILED_PRECONDITION unless its value marks another)."""

import pytest

from angzarr_client import describe
from angzarr_client.proto.io.angzarr.v1 import types_pb2 as t
from angzarr_client.router import Refused, pack

from angzarr_table import domain
from angzarr_table._descriptor import DESCRIPTOR
from angzarr_table._gen.io.angzarr.examples.v1 import table_pb2
from angzarr_table._gen.io.angzarr.examples.v1.table_aggregate_angzarr import (
    new_table_aggregate_dispatch,
)
from angzarr_table.logic_impl import TableLogicImpl


class Logic(TableLogicImpl):
    def place_bet(self, cmd, state, ctx):
        if cmd.amount % 2:
            reason = domain.TableRefusedReason.BET_NOT_EVEN
        else:
            reason = domain.TableRefusedReason.ALREADY_BET
        return domain.TableRefused(reason=reason, seat=cmd.seat)

    def release_seat(self, cmd, state, ctx):
        return None


def request(message):
    r = t.ContextualCommand()
    r.command.cover.domain = "table"
    r.command.page.command.payload.CopyFrom(pack(message))
    return r


def test_a_returned_refusal_is_refused_with_its_code():
    dispatch = new_table_aggregate_dispatch(Logic())
    with pytest.raises(Refused) as refused:
        dispatch.handle(request(table_pb2.PlaceBet(seat=2, amount=10)))
    assert refused.value.extras["refusal_type"] == "io.angzarr.examples.v1.TableRefused"
    wire = table_pb2.TableRefused()
    wire.ParseFromString(refused.value.refusal.value)
    assert wire.reason == table_pb2.TableRefused.REASON_ALREADY_BET
    assert wire.HasField("seat") and wire.seat == 2
    assert not wire.HasField("max_bet")
    plain = domain.TableRefused(reason=domain.TableRefusedReason.ALREADY_BET, seat=2)
    assert plain.key() == "io-angzarr-examples-v1-tablerefused-already-bet"
    assert plain.args() == {"reason": "ALREADY_BET", "seat": 2}
    assert refused.value.status().code == 9  # FAILED_PRECONDITION, unmarked


def test_a_refusal_goes_out_under_its_codes_marked_status():
    dispatch = new_table_aggregate_dispatch(Logic())
    with pytest.raises(Refused) as refused:
        dispatch.handle(request(table_pb2.PlaceBet(seat=2, amount=11)))
    # BET_NOT_EVEN is marked REFUSAL_STATUS_INVALID_ARGUMENT.
    assert refused.value.status().code == 3
    assert domain.TableRefused(reason=domain.TableRefusedReason.BET_NOT_EVEN).status() == 3


def test_an_optional_decision_may_emit_nothing():
    dispatch = new_table_aggregate_dispatch(Logic())
    resp = dispatch.handle(request(table_pb2.ReleaseSeat()))
    assert len(resp.events.pages) == 0


def test_the_descriptor_names_the_aggregate_by_its_domain():
    assert DESCRIPTOR.name == "table"
    generated = new_table_aggregate_dispatch(Logic())
    assert describe(generated, rules_version="0.1.0") == DESCRIPTOR
