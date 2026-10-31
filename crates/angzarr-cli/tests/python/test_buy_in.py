"""pmg-buy-in's generated project under the real angzarr-client: a command
decided and folded, a fact recorded as received with its flag, and a
hand-written dispatch on the client's public API in its place."""

import dataclasses

from angzarr_client import ComponentHost, describe
from angzarr_client.proto.io.angzarr.v1 import types_pb2 as t
from angzarr_client.router import ProcessManagerDispatch, Rebuilder, pack

from angzarr_buy_in import domain, main
from angzarr_buy_in._descriptor import DESCRIPTOR
from angzarr_buy_in._gen.io.angzarr.examples.v1 import buy_in_pb2
from angzarr_buy_in._gen.io.angzarr.examples.v1.buy_in_process_manager_angzarr import (
    new_buy_in_process_manager_dispatch,
)
from angzarr_buy_in.logic_impl import BuyInLogicImpl


def request(arm, message):
    r = t.ContextualCommand()
    r.command.cover.domain = "buy-in"
    getattr(r.command.page, arm).payload.CopyFrom(pack(message))
    return r


class Logic(BuyInLogicImpl):
    def start_buy_in(self, cmd, state, ctx):
        return domain.BuyInStarted.from_command(cmd)

    def apply_buy_in_started(self, state, event):
        phase = domain.BuyInStatePhase.AWAITING_HOLD
        return dataclasses.replace(state, buy_in_id=event.buy_in_id, phase=phase)

    def seat_confirmation_refused(self, fact, state, ctx):
        return domain.BuyInSeatLost(buy_in_id=fact.buy_in_id, reason=fact.reason)

    def apply_buy_in_seat_lost(self, state, event):
        return dataclasses.replace(state, failure_reason=event.reason)


def test_the_generated_dispatch_decides_and_folds_a_command():
    dispatch = new_buy_in_process_manager_dispatch(Logic())
    resp = dispatch.handle(request("command", buy_in_pb2.StartBuyIn(buy_in_id=b"b")))
    [page] = resp.events.pages
    assert page.payload.type_url.endswith("BuyInStarted")
    state = buy_in_pb2.BuyInState()
    resp.events.snapshot.state.Unpack(state)
    assert state.buy_in_id == b"b"
    assert state.phase == buy_in_pb2.BuyInState.PHASE_AWAITING_HOLD


def test_the_generated_dispatch_records_a_fact_and_its_flag():
    dispatch = new_buy_in_process_manager_dispatch(Logic())
    fact = buy_in_pb2.SeatConfirmationRefused(buy_in_id=b"b", reason="GONE")
    resp = dispatch.handle(request("fact", fact))
    recorded, flag = resp.events.pages
    assert recorded.payload.type_url.endswith("SeatConfirmationRefused")
    assert flag.payload.type_url.endswith("BuyInSeatLost")


def test_the_generated_host_registers_the_process_manager_by_its_name():
    assert main.host().descriptor is DESCRIPTOR
    assert DESCRIPTOR.name == "pmg-buy-in"
    generated = new_buy_in_process_manager_dispatch(Logic())
    starting = [buy_in_pb2.StartBuyIn]
    assert describe(generated, rules_version="0.1.0", starting_types=starting) == (
        DESCRIPTOR
    )


def test_a_hand_written_dispatch_takes_the_generated_ones_place():
    def start(cmd, state, ctx):
        book = t.EventBook()
        book.pages.add().payload.CopyFrom(pack(buy_in_pb2.BuyInStarted()))
        return book

    hand = ProcessManagerDispatch(
        "pmg-buy-in", "buy-in", Rebuilder(buy_in_pb2.BuyInState)
    ).on_command("io.angzarr.examples.v1.StartBuyIn", start)
    described = describe(
        hand, rules_version="0.1.0", starting_types=[buy_in_pb2.StartBuyIn]
    )
    assert described == DESCRIPTOR
    host = ComponentHost(rules_version="0.1.0").add_process_manager(hand, described)
    assert host.descriptor is described
    resp = hand.handle(request("command", buy_in_pb2.StartBuyIn()))
    assert len(resp.events.pages) == 1
