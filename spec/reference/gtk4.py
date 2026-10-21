"""The GTK4 reference window (change 0001): the widgets of gtk3.py in GTK4,
for recording trees and checking clicks. Prints "clicked NAME" when a
button is clicked.

    python3 -I spec/reference/gtk4.py [APP_ID]
"""

import sys

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import GLib, Gtk  # noqa: E402

GLib.set_prgname(sys.argv[1] if len(sys.argv) > 1 else "thodu-reference-gtk4")
Gtk.init()
window = Gtk.Window(title="Thodu GTK4 reference")
window.set_default_size(480, 360)
box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
for name in ("Open", "Save", "Close"):
    button = Gtk.Button(label=name)
    button.connect("clicked", lambda _w, n=name: print(f"clicked {n}", flush=True))
    box.append(button)
hidden = Gtk.Button(label="Hidden")
hidden.set_visible(False)
box.append(hidden)
entry = Gtk.Entry()
entry.set_placeholder_text("Search")
entry.connect("changed", lambda e: print(f"typed {e.get_text()}", flush=True))
box.append(entry)
box.append(Gtk.CheckButton(label="Remember"))
box.append(Gtk.DropDown.new_from_strings(["Small", "Medium", "Large"]))
box.append(Gtk.LinkButton.new_with_label("https://example.org", "Docs"))
window.set_child(box)
loop = GLib.MainLoop()
window.connect("close-request", lambda _w: loop.quit() or False)
window.present()
loop.run()
