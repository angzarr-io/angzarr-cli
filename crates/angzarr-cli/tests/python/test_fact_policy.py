"""policy-table-buy-in's generated project under the real angzarr-client: a
SeatHeld becomes the buy-in's StartBuyIn, sent as a Fact because the buy-in
declares it one (angzarr decision 14.69), and never as a command."""

from angzarr_client.proto.io.angzarr.v1 import types_pb2 as t
from angzarr_client.router import pack

from angzarr_policy_table_buy_in import domain
from angzarr_policy_table_buy_in._gen.io.angzarr.examples.v1 import (
    buy_in_pb2,
    table_pb2,
)
from angzarr_policy_table_buy_in._gen.io.angzarr.examples.v1.table_buy_in_policy_angzarr import (
    new_table_buy_in_policy_dispatch,
)
from angzarr_policy_table_buy_in.logic_impl import TableBuyInPolicyLogicImpl

TABLE = b"t" * 16
BUY_IN = b"b" * 16


def held():
    b = t.EventBook(cover=t.Cover(domain="table"))
    b.cover.root.value = TABLE
    b.pages.add(sequence=1).payload.CopyFrom(
        pack(table_pb2.SeatHeld(buy_in_id=BUY_IN, seat=2, amount=500))
    )
    return b


class Logic(TableBuyInPolicyLogicImpl):
    def seat_held(self, event, source, ctx):
        start = domain.StartBuyIn(
            buy_in_id=event.buy_in_id, table_root=source, seat=event.seat
        )
        return [(BUY_IN, start)]


def test_the_generated_dispatch_sends_a_declared_fact_as_a_fact():
    response = new_table_buy_in_policy_dispatch(Logic()).handle(held())
    [reaction] = response.reactions
    [output] = reaction.outputs
    assert output.cover.domain == "buy-in"
    assert output.cover.root.value == BUY_IN
    assert output.page.WhichOneof("body") == "fact"
    start = buy_in_pb2.StartBuyIn()
    output.page.fact.payload.Unpack(start)
    assert start.seat == 2 and start.table_root == TABLE
