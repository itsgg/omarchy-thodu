# 0001: Reference windows instead of widget-factory demos

Status: merged
Why: gtk3-widget-factory and gtk4-widget-factory are not installed on GG's machine (2026-10-21), and installing them needs his approval. Small reference windows written with python-gobject (GTK3 and GTK4) and PySide6 (Qt Widgets), all installed, are deterministic and live in the repository, so recorded trees can be made again on any machine.

## Modified

### REQ-SRC-02: The toolkits of this milestone
Acceptance changes from recorded trees of gtk3-widget-factory, gtk4-widget-factory and a Qt Widgets demo to recorded trees of the reference windows in spec/reference/ (gtk3.py, gtk4.py, qt.py), an Electron app (Obsidian) and Chromium; the rest is unchanged.

### REQ-PERF-01: Labels fast enough to use
Measured over the GTK3 reference window instead of gtk3-widget-factory; the rest is unchanged.

## Tests

- Task 0001 and 0002's live checks name the GTK3 reference window.
