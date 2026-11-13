# SPDX-License-Identifier: GPL-3.0-or-later
"""`ptyrc python`: a script or a console with `pilot` connected."""

from __future__ import annotations

import sys
from pathlib import Path

import pytest
from ptyrc.shell import main


def test_script(spawn, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    target = spawn(["cat"])
    monkeypatch.setenv("PTYRC_SESSION", target.session)
    script = tmp_path / "s.py"
    script.write_text("import sys\npilot.send(sys.argv[1] + '\\n')\n")
    assert main([str(script), "from-script"]) == 0
    target.wait_for("from-script", timeout=3)


def test_console(spawn) -> None:
    target = spawn(["cat"])
    console = spawn(
        [
            "env",
            "-u",
            "PTYRC_SOCKET",
            f"PTYRC_SESSION={target.session}",
            sys.executable,
            "-c",
            "import sys; from ptyrc.shell import main; sys.exit(main())",
        ]
    )
    console.wait_for('Connected to "cat"', timeout=5)
    console.send("pilot.send('from-console\\n')\n")
    target.wait_for("from-console", timeout=3)


def test_no_session(monkeypatch: pytest.MonkeyPatch, capsys) -> None:
    monkeypatch.setenv("PTYRC_SESSION", "none-such")
    assert main([]) == 3
    assert "no such session: none-such" in capsys.readouterr().err
