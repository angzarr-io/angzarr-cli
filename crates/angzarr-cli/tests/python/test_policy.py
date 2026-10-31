"""policy-table-player's generated project under the real angzarr-client:
each TurnStarted becomes the player's DecideMove at its root, and a
hand-written dispatch on the client's public API takes its place."""

from angzarr_client import ComponentHost
from angzarr_client.proto.io.angzarr.v1 import types_pb2 as t
from angzarr_client.router import PolicyDispatch, pack

from angzarr_policy_table_player import domain, main
from angzarr_policy_table_player._descriptor import DESCRIPTOR
from angzarr_policy_table_player._gen.io.angzarr.examples.v1 import (
    player_pb2,
    table_pb2,
)
from angzarr_policy_table_player._gen.io.angzarr.examples.v1.table_player_policy_angzarr import (
    new_table_player_policy_dispatch,
)
from angzarr_policy_table_player.logic_impl import TablePlayerPolicyLogicImpl

TABLE = b"t" * 16


def turn(seat):
    b = t.EventBook(cover=t.Cover(domain="table"))
    b.cover.root.value = TABLE
    b.pages.add(sequence=3).payload.CopyFrom(
        pack(table_pb2.TurnStarted(seat=seat, player_root=b"p" * 16))
    )
    return b


class Logic(TablePlayerPolicyLogicImpl):
    def turn_started(self, event, source, ctx):
        assert ctx.sequence == 3
        command = domain.DecideMove(table_root=source, seat=event.seat)
        return [(event.player_root, command)]


def test_the_generated_dispatch_sends_the_targets_commands():
    response = new_table_player_policy_dispatch(Logic()).handle(turn(2))
    [reaction] = response.reactions
    assert reaction.trigger_sequence == 3
    [command] = reaction.commands
    assert command.cover.domain == "player"
    assert command.cover.root.value == b"p" * 16
    decided = player_pb2.DecideMove()
    decided.ParseFromString(command.page.command.payload.value)
    assert command.page.command.payload.type_url.endswith("DecideMove")
    assert decided.seat == 2 and decided.table_root == TABLE


def test_the_generated_host_registers_the_policy_by_its_name():
    assert main.host().descriptor is DESCRIPTOR
    assert DESCRIPTOR.name == "policy-table-player"


def test_a_hand_written_dispatch_takes_the_generated_ones_place():
    def started(event, ctx):
        book = t.CommandBook(cover=t.Cover(domain="player"))
        book.page.command.payload.CopyFrom(pack(player_pb2.DecideMove(seat=9)))
        return [book]

    hand = PolicyDispatch("policy-table-player", "table", "player")
    hand.on_event_with_context("io.angzarr.examples.v1.TurnStarted", started)
    host = ComponentHost(rules_version="0.1.0").add_policy(hand, DESCRIPTOR)
    assert host.descriptor is DESCRIPTOR
    [reaction] = hand.handle(turn(1)).reactions
    assert len(reaction.commands) == 1
