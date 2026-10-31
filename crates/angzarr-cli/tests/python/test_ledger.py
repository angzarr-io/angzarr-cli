"""The ledger projector's generated project under the real angzarr-client:
the generated dispatch folds both sources through its coroutines, and a
hand-written dispatch on the client's public API takes its place."""

import asyncio
import dataclasses

from angzarr_client import ComponentHost, describe
from angzarr_client.proto.io.angzarr.v1 import types_pb2 as t
from angzarr_client.router import ProjectorDispatch, pack

from angzarr_projector_wallet_table_ledger import domain, main
from angzarr_projector_wallet_table_ledger._descriptor import DESCRIPTOR
from angzarr_projector_wallet_table_ledger._gen.io.angzarr.examples.v1 import (
    ledger_pb2,
    table_pb2,
    wallet_pb2,
)
from angzarr_projector_wallet_table_ledger._gen.io.angzarr.examples.v1.ledger_projector_angzarr import (
    new_ledger_projector_dispatch,
)
from angzarr_projector_wallet_table_ledger.logic_impl import LedgerProjectorLogicImpl


def book(domain_name, *events):
    b = t.EventBook(cover=t.Cover(domain=domain_name))
    for i, event in enumerate(events):
        b.pages.add(sequence=i).payload.CopyFrom(pack(event))
    return b


def projection(answer):
    p = ledger_pb2.LedgerProjection()
    p.ParseFromString(answer.projection.value)
    return p


class Logic(LedgerProjectorLogicImpl):
    def __init__(self):
        self.loops = set()

    async def project_table_created(self, projection, event, ctx):
        await asyncio.sleep(0)
        self.loops.add(asyncio.get_running_loop())
        assert ctx.cover.domain == "table"
        row = domain.TableLedgerRow(name=event.name)
        tables = {**projection.tables, "t1": row}
        return dataclasses.replace(projection, tables=tables)

    async def project_funds_deposited(self, projection, event, ctx):
        self.loops.add(asyncio.get_running_loop())
        assert ctx.cover.domain == "wallet"
        totals = domain.LedgerTotals(deposits=event.amount)
        return dataclasses.replace(projection, totals=totals)


def test_the_generated_dispatch_projects_both_sources():
    logic = Logic()
    dispatch = new_ledger_projector_dispatch(logic)
    table = projection(dispatch.handle(book("table", table_pb2.TableCreated(name="t"))))
    assert table.tables["t1"].name == "t"
    wallet = projection(
        dispatch.handle(book("wallet", wallet_pb2.FundsDeposited(amount=7)))
    )
    assert wallet.totals.deposits == 7
    # A domain it does not read is not folded.
    other = projection(dispatch.handle(book("player", table_pb2.TableCreated(name="x"))))
    assert not other.tables
    # Every coroutine ran on the dispatch's one loop.
    assert len(logic.loops) == 1


def test_the_generated_host_registers_the_projector_by_its_name():
    host = main.host()
    assert host.descriptor is DESCRIPTOR
    assert DESCRIPTOR.name == "projector-wallet-table-ledger"
    generated = new_ledger_projector_dispatch(Logic())
    assert describe(generated, rules_version="0.1.0") == DESCRIPTOR


def test_a_hand_written_dispatch_takes_the_generated_ones_place():
    def created(projection, event):
        row = ledger_pb2.TableLedgerRow(name="hand")
        projection.tables["t1"].CopyFrom(row)

    hand = ProjectorDispatch(
        "projector-wallet-table-ledger", ledger_pb2.LedgerProjection
    ).for_domains("table", "wallet")
    hand.on_event("io.angzarr.examples.v1.TableCreated", created)
    described = describe(hand, rules_version="0.1.0")
    assert described == DESCRIPTOR
    replaced = ComponentHost(rules_version="0.1.0").add_projector(hand, described)
    assert replaced.descriptor is described
    table = projection(hand.handle(book("table", table_pb2.TableCreated(name="t"))))
    assert table.tables["t1"].name == "hand"
