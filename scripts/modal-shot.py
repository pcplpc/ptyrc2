# SPDX-License-Identifier: GPL-3.0-or-later
"""A modal `alert` widget over a full 80x24 screen: what the host shows."""
import sys
import time

from ptyrc import Pilot

fill = "seq 1 2000 | tr '\\n' ' ' | head -c 1919; sleep 60"
pilot = Pilot.spawn(["sh", "-c", fill], size=(80, 24))
pilot.wait_for("1 2 3")
time.sleep(0.5)
sizes = [(34, 9), (48, 13), (64, 18), (78, 22)]
menu = pilot.widget("manage", sizes, modal=True, border="alert", prefer="center", fill=1.0)
while not menu.refresh().placed:
    time.sleep(0.05)
menu.set(["", "  ▸ ● bricks -a           running   2m", "    ‖ fishes              paused   10m",
          "    + start an app…"])
print(menu.info, file=sys.stderr)
for line in pilot.shown():
    print(line)
if len(sys.argv) > 1:
    for i in range(3):
        cells = pilot.call("screen.cells", sprites=True, rect=[4, 2, 72, 1])
        print([ (r["t"][:6], r.get("fg")) for r in cells["rows"][0]][:6], file=sys.stderr)
        time.sleep(0.06)
pilot.close()
