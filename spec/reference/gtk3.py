"""The GTK3 reference window (change 0001): known widgets, in a known order,
for recording trees and for checking clicks. Prints "clicked NAME" when a
button is clicked, so a live check can see a click land.

    python3 -I spec/reference/gtk3.py [APP_ID]
"""

import sys

import gi

gi.require_version("Gtk", "3.0")
from gi.repository import GLib, Gtk  # noqa: E402

GLib.set_prgname(sys.argv[1] if len(sys.argv) > 1 else "thodu-reference-gtk3")
window = Gtk.Window(title="Thodu GTK3 reference")
window.set_default_size(480, 360)
box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
for name in ("Open", "Save", "Close"):
    button = Gtk.Button(label=name)
    button.connect("clicked", lambda _w, n=name: print(f"clicked {n}", flush=True))
    box.pack_start(button, False, False, 0)
hidden = Gtk.Button(label="Hidden")
hidden.set_no_show_all(True)
box.pack_start(hidden, False, False, 0)
entry = Gtk.Entry()
entry.set_placeholder_text("Search")
box.pack_start(entry, False, False, 0)
box.pack_start(Gtk.CheckButton(label="Remember"), False, False, 0)
combo = Gtk.ComboBoxText()
for size in ("Small", "Medium", "Large"):
    combo.append_text(size)
combo.set_active(0)
box.pack_start(combo, False, False, 0)
box.pack_start(Gtk.LinkButton.new_with_label("https://example.org", "Docs"), False, False, 0)
window.add(box)
window.connect("destroy", Gtk.main_quit)
window.show_all()
Gtk.main()
