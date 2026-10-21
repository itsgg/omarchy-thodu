# 0007: The bar's items come from a bar widget of the overlay plugin

Status: merged
Why: task 0006 found that an overlay plugin cannot reach the bar: it is mounted by a Loader held as a property, with no visual parent, and a third-party plugin's shell facade offers no bar layout. A bar widget is mounted inside the bar's window, so it can walk the bar's items, and Omarchy's bar marks its own click targets (Bar.qml's moduleTargetClickable). No requirement changes.

## Modified

Section 4, item 2: the plugin `io.github.itsgg.thodu` is also of kind bar-widget. Its widget, `BarProbe.qml`, draws nothing; once a second it finds the bar's clickable items (`lib/BarScan.js`) and, when they change, sends them to `thodud`, which keeps the latest per monitor. `thodu install` adds the widget to the bar.

Section 7, Interfaces:

- `{"v":1,"op":"shell-items","screen":NAME,"items":[{"x","y","w","h","text"}]}`: the bar's items on the monitor NAME, relative to the bar's window. The shell source places them at the bar's layer (namespace `omarchy-bar`) as Hyprland reports it, within its 40 ms budget.
- Labels go to the window's targets before the bar's, so the window keeps the shortest ones (REQ-LABEL-01).

## Tests

- tests/qml/tst_barscan.qml: test_req_shell_01_a_fake_bar_reports_its_three_items.
- crates/thodud/src/sources.rs: req_shell_01_bar_items_become_shell_targets.
