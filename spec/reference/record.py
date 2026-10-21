"""Record a window's accessibility tree as a Thodu snapshot (SPEC section 5),
the shape thodu-core replays: the window and monitor as Hyprland reports
them, and each application of that window's process with its frames.

    python3 -I spec/reference/record.py WINDOW_ADDRESS > snapshot.json
"""

import json
import subprocess
import sys

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402

STATES = {"active": 1, "checked": 4, "defunct": 6, "editable": 7, "enabled": 8, "focusable": 11,
          "focused": 12, "modal": 16, "sensitive": 24, "showing": 25, "visible": 30}


def hypr(cmd):
    return json.loads(subprocess.run(["hyprctl", "-j", cmd], capture_output=True, text=True, check=True).stdout)


def node(acc, depth=0):
    states = acc.get_state_set()
    out = {
        "role": acc.get_role_name(),
        "states": [n for n, v in STATES.items() if states.contains(Atspi.StateType(v))],
        "name": acc.get_name() or "",
        "description": acc.get_description() or "",
    }
    comp = acc.get_component_iface()
    if comp:
        e = comp.get_extents(Atspi.CoordType.WINDOW)
        out["extents"] = [e.x, e.y, e.width, e.height]
    if depth < 40:
        out["children"] = [node(acc.get_child_at_index(i), depth + 1) for i in range(acc.get_child_count())]
    return out


address = sys.argv[1]
window = next(c for c in hypr("clients") if c["address"] == address)
monitor = next(m for m in hypr("monitors") if m["id"] == window["monitor"])
desktop = Atspi.get_desktop(0)
apps = []
for i in range(desktop.get_child_count()):
    app = desktop.get_child_at_index(i)
    if app is not None and app.get_process_id() == window["pid"]:
        apps.append({"name": app.get_name(), "pid": window["pid"],
                     "frames": [node(app.get_child_at_index(j)) for j in range(app.get_child_count())]})
print(json.dumps({
    "window": {"address": window["address"], "class": window["class"], "title": window["title"],
               "pid": window["pid"], "at": window["at"], "size": window["size"],
               "workspace": window["workspace"]["id"], "monitor": window["monitor"]},
    "monitor": {"name": monitor["name"], "x": monitor["x"], "y": monitor["y"], "width": monitor["width"],
                "height": monitor["height"], "scale": monitor["scale"]},
    "apps": apps,
}, indent=1))
