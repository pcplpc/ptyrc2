# SPDX-License-Identifier: GPL-3.0-or-later
"""The connection: framing of what is read, calls that cannot be answered."""

from __future__ import annotations

import json
import socket
import threading
from pathlib import Path

import pytest
from ptyrc import Pilot, PtyrcError
from ptyrc._wire import Connection


def line(msg: dict) -> bytes:
    return json.dumps(msg, separators=(",", ":")).encode() + b"\n"


def test_a_reply_in_many_pieces_among_events(tmp_path: Path) -> None:
    """A reply longer than one read, events before and after it in the
    same writes: all whole, in order."""
    path = tmp_path / "s.sock"
    server = socket.socket(socket.AF_UNIX)
    server.bind(str(path))
    server.listen(1)
    rows = [f"row {i} " + "x" * 90 for i in range(30000)]

    def serve() -> None:
        peer, _ = server.accept()
        peer.sendall(line({"event": "hello", "data": {"protocol": 1}}))
        requests = peer.makefile("rb")
        request = json.loads(requests.readline())
        reply = line({"id": request["id"], "result": {"rows": rows}})
        peer.sendall(line({"event": "a", "data": 1}) + reply[:100000])
        peer.sendall(reply[100000:] + line({"event": "b", "data": 2}) + b'{"eve')
        request = json.loads(requests.readline())
        peer.sendall(b'nt":"c"}\n\n' + line({"id": request["id"], "result": 3}))
        requests.readline()

    threading.Thread(target=serve, daemon=True).start()
    conn = Connection(path)
    try:
        assert conn.call("big")["rows"] == rows
        assert conn.call("small") == 3
        events = [conn.events.get(timeout=5) for _ in range(3)]
        assert events == [
            {"event": "a", "data": 1},
            {"event": "b", "data": 2},
            {"event": "c"},
        ]
    finally:
        conn.close()
        server.close()


def test_a_request_the_driver_cannot_read_fails_the_call(shell: Pilot) -> None:
    """The start of a request, as a write cut by an interrupt leaves it:
    the next one ends that line. The driver answers without an id; the
    call fails at once, and the connection goes on."""
    shell._conn._sock.sendall(b'{"id":0,"method":"draw.put","params":{"content"')
    with pytest.raises(PtyrcError) as err:
        shell._conn.call("session.info", timeout=5.0)
    assert err.value.code == "bad-request"
    assert shell.info()["session"] == shell.session
    assert not shell._conn._pending
