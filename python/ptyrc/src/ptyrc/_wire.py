# SPDX-License-Identifier: GPL-3.0-or-later
"""Socket discovery and the line-delimited JSON connection."""

from __future__ import annotations

import json
import os
import queue
import re
import socket
import subprocess
import threading
from pathlib import Path
from typing import Any

__all__ = ["Connection", "PtyrcError", "live_sessions", "runtime_dir", "socket_path"]

PROTOCOL = 1


class PtyrcError(Exception):
    """An error reply of the driver, or a transport failure (`gone`).

    `code` and `message` are the driver's; `hint`, when there is one, says
    what to try: the valid values, the nearest one."""

    def __init__(self, code: str, message: str = "", hint: str | None = None) -> None:
        text = f"{code}: {message}" if message else code
        super().__init__(f"{text}\n  {hint}" if hint else text)
        self.code = code
        self.message = message
        self.hint = hint

    @classmethod
    def from_wire(cls, err: dict[str, Any] | None) -> PtyrcError:
        """The `error` member of a response."""
        err = err or {}
        return cls(err.get("code", "internal"), err.get("message", ""), err.get("hint"))


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


def _of_tmux() -> str | None:
    """The session the tmux session of this pane names in `@ptyrc`."""
    pane = os.environ.get("TMUX_PANE")
    if not os.environ.get("TMUX") or not pane:
        return None
    try:
        done = subprocess.run(
            ["tmux", "display-message", "-p", "-t", pane, "#{@ptyrc}"],
            check=False,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=5,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    name = done.stdout.strip()
    if done.returncode != 0 or not re.fullmatch(r"[A-Za-z0-9_-][A-Za-z0-9._-]*", name):
        return None
    return name


def socket_path(session: str | None = None) -> Path:
    """Socket of `session`, else of `PTYRC_SOCKET` / `PTYRC_SESSION`, else of
    the session over the tmux session of this pane, else of the only live
    session."""
    if session is None:
        direct = os.environ.get("PTYRC_SOCKET")
        if direct:
            return Path(direct)
        session = os.environ.get("PTYRC_SESSION")
    if session is not None:
        path = runtime_dir() / f"{session}.sock"
        if not _alive(path):
            live = live_sessions()
            raise PtyrcError(
                "gone",
                f"no such session: {session}",
                f"running: {', '.join(live)}"
                if live
                else f"start it: ptyrc run --name {session} -- CMD",
            )
        return path
    over = _of_tmux()
    if over is not None and _alive(runtime_dir() / f"{over}.sock"):
        return runtime_dir() / f"{over}.sock"
    live = live_sessions()
    if len(live) == 1:
        return runtime_dir() / f"{live[0]}.sock"
    if not live:
        raise PtyrcError("gone", "no session", "start one: ptyrc run -- CMD")
    raise PtyrcError(
        "bad-request",
        f"{len(live)} sessions, none chosen",
        f"pick one with session= or PTYRC_SESSION: {', '.join(live)}",
    )


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
            raise PtyrcError.from_wire(msg["error"])
        return msg.get("result")

    def close(self) -> None:
        self._closed = True
        try:
            self._sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self._sock.close()
