# SPDX-License-Identifier: GPL-3.0-or-later
"""Socket discovery and the line-delimited JSON connection."""

from __future__ import annotations

import json
import os
import queue
import socket
import threading
from pathlib import Path
from typing import Any

__all__ = ["Connection", "PtyrcError", "live_sessions", "runtime_dir", "socket_path"]

PROTOCOL = 1


class PtyrcError(Exception):
    """An error reply of the driver, or a transport failure (`gone`)."""

    def __init__(self, code: str, message: str = "") -> None:
        super().__init__(f"{code}: {message}" if message else code)
        self.code = code
        self.message = message


def runtime_dir() -> Path:
    """Directory holding the session sockets."""
    base = os.environ.get("XDG_RUNTIME_DIR")
    if base:
        return Path(base) / "ptyrc"
    return Path(f"/tmp/ptyrc-{os.getuid()}")


def _alive(path: Path) -> bool:
    probe = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        probe.settimeout(0.5)
        probe.connect(str(path))
        return True
    except OSError:
        return False
    finally:
        probe.close()


def live_sessions() -> list[str]:
    """Names of the sessions that accept a connection."""
    try:
        socks = sorted(runtime_dir().glob("*.sock"))
    except OSError:
        return []
    return [p.stem for p in socks if _alive(p)]


def socket_path(session: str | None = None) -> Path:
    """Socket of `session`, else of `PTYRC_SOCKET` / `PTYRC_SESSION`, else of
    the only live session."""
    if session is None:
        direct = os.environ.get("PTYRC_SOCKET")
        if direct:
            return Path(direct)
        session = os.environ.get("PTYRC_SESSION")
    if session is not None:
        return runtime_dir() / f"{session}.sock"
    live = live_sessions()
    if len(live) == 1:
        return runtime_dir() / f"{live[0]}.sock"
    if not live:
        raise PtyrcError("gone", "no live session")
    raise PtyrcError("bad-request", "several sessions: " + ", ".join(live))


class Connection:
    """One client connection. Calls are thread-safe; a reader thread routes
    replies by id and queues events."""

    def __init__(self, path: Path | str, timeout: float = 5.0) -> None:
        self._sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._sock.settimeout(timeout)
        try:
            self._sock.connect(str(path))
        except OSError as exc:
            self._sock.close()
            raise PtyrcError("gone", f"{path}: {exc}") from exc
        self._sock.settimeout(None)
        self._send_lock = threading.Lock()
        self._lock = threading.Lock()
        self._next_id = 0
        self._pending: dict[int, queue.SimpleQueue[dict[str, Any] | None]] = {}
        self._closed = False
        self.events: queue.SimpleQueue[dict[str, Any] | None] = queue.SimpleQueue()
        self._hello: queue.SimpleQueue[dict[str, Any] | None] = queue.SimpleQueue()
        self._reader = threading.Thread(target=self._read_loop, daemon=True)
        self._reader.start()
        try:
            hello = self._hello.get(timeout=timeout)
        except queue.Empty:
            self.close()
            raise PtyrcError("timeout", "no hello") from None
        if hello is None:
            raise PtyrcError("gone", "closed before hello")
        if hello.get("protocol") != PROTOCOL:
            self.close()
            raise PtyrcError("bad-request", f"protocol {hello.get('protocol')}")
        self.hello = hello

    @property
    def closed(self) -> bool:
        return self._closed

    def _read_loop(self) -> None:
        buf = b""
        seen_hello = False
        try:
            while True:
                chunk = self._sock.recv(65536)
                if not chunk:
                    break
                buf += chunk
                *lines, buf = buf.split(b"\n")
                for line in lines:
                    if not line.strip():
                        continue
                    msg = json.loads(line)
                    if "event" in msg:
                        if not seen_hello and msg["event"] == "hello":
                            seen_hello = True
                            self._hello.put(msg.get("data") or {})
                        else:
                            self.events.put(msg)
                    elif "id" in msg:
                        with self._lock:
                            waiter = self._pending.pop(msg["id"], None)
                        if waiter is not None:
                            waiter.put(msg)
        except (OSError, ValueError):
            pass
        finally:
            self._closed = True
            with self._lock:
                waiters = list(self._pending.values())
                self._pending.clear()
            for waiter in waiters:
                waiter.put(None)
            self._hello.put(None)
            self.events.put(None)

    def call(
        self, method: str, params: dict[str, Any] | None = None, timeout: float = 30.0
    ) -> Any:
        """Send a request, return its result. Raises `PtyrcError`."""
        waiter: queue.SimpleQueue[dict[str, Any] | None] = queue.SimpleQueue()
        with self._lock:
            if self._closed:
                raise PtyrcError("gone", "connection closed")
            self._next_id += 1
            rid = self._next_id
            self._pending[rid] = waiter
        line = json.dumps(
            {"id": rid, "method": method, "params": params or {}},
            ensure_ascii=False,
            separators=(",", ":"),
        )
        try:
            with self._send_lock:
                self._sock.sendall(line.encode() + b"\n")
        except OSError as exc:
            with self._lock:
                self._pending.pop(rid, None)
            raise PtyrcError("gone", str(exc)) from exc
        try:
            msg = waiter.get(timeout=timeout)
        except queue.Empty:
            with self._lock:
                self._pending.pop(rid, None)
            raise PtyrcError("timeout", f"no reply to {method}") from None
        if msg is None:
            raise PtyrcError("gone", "connection closed")
        if "error" in msg:
            err = msg["error"] or {}
            raise PtyrcError(err.get("code", "internal"), err.get("message", ""))
        return msg.get("result")

    def close(self) -> None:
        self._closed = True
        try:
            self._sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self._sock.close()
