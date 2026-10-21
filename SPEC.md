# Thodu: Specification

Created: 2026-10-21. Owner: GG. Derives from SEED.md at commit 23158cc, sections 1, 4, 5, 9 and 10. The spec's version is the git commit of this file.

Precedence: SEED.md, then this spec, then tasks, then the code. A conflict with SEED.md goes to GG. Agents never edit this file: they propose changes in `changes/` (format in `changes/README.md`), and the coordinator merges them.

## 1. Conventions

This spec fixes what parallel agents would otherwise build differently: the components and their boundaries, the target model, coordinates, the IPC, the key bindings inside a session, and how each behavior is checked. MUST and SHALL bind; SHOULD is the default unless a recorded reason says otherwise; MAY is optional. A value marked provisional was chosen without measurement and is replaced by the first measurement through a change.

Requirement IDs are REQ-<AREA>-<NN>, with these areas: SESSION (starting and ending a session), SRC (sources of targets), LABEL, SEARCH, ACT (actions), GRID, SHELL (the Omarchy shell source and overlay), CLI, INSTALL, PERF. Acceptance forms in use: conformance cases in `spec/cases/`, properties with a fixed seed, recorded accessibility trees replayed through a fake source, and measured thresholds.

## 2. Scope and non-goals

This spec covers SEED section 9's first coherent experience: click mode with search and the grid, in the focused window, on GG's laptop. Targets come from the accessibility tree for GTK3, GTK4, Qt Widgets, Electron apps and Chromium's own controls, and from the Omarchy bar. Every click action and Escape work.

Out of this milestone, and designed for so that no code assumes they never come: scroll mode, web page targets (SEED section 5, source 2), the screen-image source, drag and hover, the agent interface beyond `thodu targets`, more than one monitor. Permanently out (SEED section 9): desktops other than Omarchy.

## 3. Terms

- Target: something a mouse could act on, with bounds, a role, text and a source (section 5).
- Label: the letters drawn over a target; typing it with Shift selects the target.
- Source: where targets come from: `atspi`, `shell`, `grid` in this milestone; `web` and `image` later.
- Session: one press of the key until an action or Escape.
- Logical coordinates: Hyprland's layout coordinates, in which a window's `at` and `size` are given; one logical unit is `scale` physical pixels on that monitor (1.25 on eDP-1).
- Focused window: Hyprland's `activewindow`.
- Overlay: the omarchy-shell plugin that draws labels and takes the keyboard during a session.

## 4. Architecture

Four components, each with one job:

1. `thodud`, the service: collects targets from the sources, assigns labels, owns sessions, performs actions, and answers the IPC of section 7. It runs as a systemd user service started with the session.
2. The overlay, `io.github.itsgg.thodu`: an omarchy-shell plugin of kind overlay (QML). It draws one session's labels on the focused window's monitor, takes the keyboard exclusively, reports keys to `thodud`, and lists the shell's own items for the `shell` source. It holds no target logic. It is kept loaded and listens on `thodud`'s socket, so `thodud` pushes each session to it and summons it through omarchy-shell only when nothing listens. The same plugin is a bar widget too: `BarProbe.qml`, which draws nothing, finds the bar's clickable items once a second and sends them to `thodud` when they change.
3. `thodu`, the command line: starts sessions (`thodu hints`, `thodu grid`), and reads and acts for agents (`thodu targets`). Hyprland keybindings call it.
4. Later, the Chromium extension (SEED section 5); not in this milestone.

Layout: `crates/thodu-core` holds the logic with no input or output (the target model, coordinates, labels, search, the grid, the stale check), `crates/thodud` the sources, actions and IPC server, and `crates/thodu` the command line. The overlay sits at the repository root as an Omarchy plugin: `manifest.json`, `Thodu.qml` and `components/`. Conformance cases live in `spec/cases/`, acceptance tests of record in `tests/acceptance/`, QML tests in `tests/qml/`.

Allowed dependencies: `thodu` and the overlay talk to `thodud` only through the IPC; `thodud` talks to Hyprland's request socket, the accessibility bus over D-Bus, the Wayland virtual-pointer protocol and `/dev/uinput`. Nothing else talks to those.

Language: `thodud` and `thodu` are Rust, provisional (SEED's interim direction), confirmed or changed by the latency measured in task 0001 (REQ-PERF-01). The overlay is QML, as omarchy-shell requires.

External interfaces this depends on, each a risk SEED section 11 names: Hyprland 0.56 with the Lua configuration (dispatch syntax detected at start, never assumed); AT-SPI 2.62; omarchy-shell's plugin contract in Omarchy 4.0.x; `zwlr_virtual_pointer_v1`.

## 5. Data model and invariants

A target is:

| Field | Type | Meaning |
|---|---|---|
| `id` | string | unique within its session |
| `bounds` | `{x, y, w, h}` integers | logical coordinates, clipped to the window and the monitor |
| `role` | string | one of button, link, entry, menu-item, tab, list-item, check, toggle, combo, slider, scroll-area, text, unknown |
| `text` | string | name, description and value joined by one space, for search |
| `source` | string | atspi, shell or grid |
| `actions` | list | of click, right, middle, double, focus |
| `window` | string | Hyprland window address, empty for shell targets |

A session holds its id, the window and its geometry at collection, the monitor's name, logical origin and scale, the targets, the labels, and the time each source took.

Invariants, checked as properties:

- REQ-LABEL-01 makes labels distinct and prefix-free.
- Every target's bounds lie inside its window's bounds at collection and have positive width and height; anything else is dropped before labelling.
- Converting a point from logical to physical coordinates and back moves it by at most one logical unit.

## 6. Behavior

### Sessions

### REQ-SESSION-01: Labels on the focused window
When `thodu hints` runs and a window is focused, the system shall show a label for each target of that window and of the Omarchy bar on the same monitor.
Acceptance:
- replay: each recorded tree in `spec/cases/atspi/*.json` through the fake source yields the targets listed beside it in `spec/cases/atspi/*.targets.jsonl`.
- measured: REQ-PERF-01.

### REQ-SESSION-02: Escape gives the keyboard back
When Escape is pressed during a session, the system shall end the session and release the keyboard; no key after Escape reaches the overlay.
Acceptance:
- scenario: WHEN a session is showing and the overlay receives Escape THEN `thodud` reports the session closed and the overlay's layer surface no longer has keyboard focus (QML test on the overlay with a fake IPC).

### REQ-SESSION-03: A crash gives the keyboard back
If `thodud` stops answering during a session, then the overlay shall close its surface and release the keyboard within one second.
Acceptance:
- scenario: WHEN the fake IPC stops answering mid-session THEN the overlay closes within 1000 ms (QML test).

### Sources

### REQ-SRC-01: Match by process, never by name
When collecting from the accessibility tree, the system shall find the focused window's frame by the process id Hyprland reports and the frame's ACTIVE state, falling back to the frame whose extents match the window's geometry; it shall never match by application name.
Acceptance:
- cases: `spec/cases/atspi/match.jsonl`, pairs of a Hyprland window and a registry snapshot with the expected frame, including two frames from one Chromium process and an app whose name differs from its class.

### REQ-SRC-02: The toolkits of this milestone
The system shall produce targets from GTK3, GTK4, Qt Widgets and Electron windows and from Chromium's own controls, converting window-relative extents (GTK4) and screen extents alike to logical coordinates.
Acceptance:
- replay: recorded trees of the reference windows in spec/reference/ (gtk3.py, gtk4.py, qt.py), an Electron app (Obsidian) and Chromium, each with the targets a person marked, at least 90 percent found (provisional) and none outside its window.

### REQ-SRC-03: Waking Chromium and Electron
When a Chromium or Electron window's tree is dormant, the system shall wake it with one `GetAttributes` call on the frame, at most once per process, and collect after it.
Acceptance:
- scenario: WHEN the fake bus reports a frame whose only child is the null object THEN `thodud` calls GetAttributes once and collects the children it then reports.

### REQ-SRC-04: Only what can be clicked now
The system shall drop elements that are not SHOWING and VISIBLE, that lie outside the window, that are covered by another element of the same window, or that belong to a closed popup.
Acceptance:
- replay: the recorded trees include hidden subtrees and a closed combo box; none of their elements become targets.

### REQ-SRC-05: Budgets per source
While collecting, the system shall give each source a time budget (80 ms for atspi and 40 ms for shell, provisional) and leave a source that misses it out of the session, naming it in the status line.
Acceptance:
- scenario: WHEN a fake source sleeps past its budget THEN the session shows the other sources' targets and the status line names the slow source.

### Labels

### REQ-LABEL-01: Prefix-free labels from the home row
The system shall assign each target a distinct label of at most three letters from the alphabet `asdfghjkl` then `qwertyuiopzxcvbnm`, none a prefix of another, the shortest labels to the targets nearest the window's top-left.
Acceptance:
- property: for any n from 1 to 2000 targets, the labels are distinct, prefix-free and at most three letters (fixed seed).
- cases: `spec/cases/labels/assign.jsonl`.

### REQ-LABEL-02: Labels sit on their targets
The system shall place each label inside its target's bounds when the target is at least as large as the label, and otherwise centered on the target's top-left corner, clipped to the monitor.
Acceptance:
- property: for any target and label size, the placement follows this rule.

### Search

### REQ-SEARCH-01: Typing searches; Shift types a label
While a session is showing, when letters are typed without Shift, the system shall keep only the targets whose text contains the typed letters, ignoring case and spaces, and focus the first in reading order; when letters are typed with Shift, the system shall focus the target with that label. Tab and Shift-Tab move the focus through the kept targets. This follows Homerow's defaults (SEED R1).
Acceptance:
- cases: `spec/cases/search/filter.jsonl`, typed input against target texts with the expected kept targets and focus.

### REQ-SEARCH-02: Labels-only mode
Where the setting `labels_only` is true, letters without Shift type labels and `/` starts a search.
Acceptance:
- cases: `spec/cases/search/labels-only.jsonl`.

### Actions

### REQ-ACT-01: Return clicks
When Return is pressed with a focused target, the system shall move the pointer to the target's center, click the left button there through the virtual pointer, end the session and return the pointer to where it was, unless Hyprland's focus follows the mouse and that point is not over the clicked window, where returning it would take the keyboard from that window; then the pointer stays on the target. A session started with `test` always returns it.
Acceptance:
- scenario: WHEN a session on a recorded tree focuses a button at bounds (100, 200, 80, 30) on a monitor at scale 1.25 THEN the fake pointer receives a move to (140, 215) logical and one left click.
- scenario: WHEN focus follows the mouse and the pointer was over another window, or over none while the click was on a window, THEN it is not returned; over the same window, over none after a click on none, or with focus on click, it is.

### REQ-ACT-02: The other clicks
When Shift-Return is pressed the system shall right-click, when Return is pressed twice within 300 ms it shall double-click, and when Control-Return is pressed it shall click with Control held. After a plain Return on a target that is not an entry, the overlay keeps the keyboard for 300 ms, drawing nothing and letting the pointer through, so a second Return can come; any other key ends the session. After an entry, a right click or a Control click it gives the keyboard back at once.
Acceptance:
- cases: `spec/cases/act/keys.jsonl`, key sequences with the expected pointer and keyboard events and whether the overlay keeps the keyboard.

### REQ-ACT-03: Never act on a stale target
If the target's window has moved, resized, closed or left the active workspace since collection, then the system shall not act; it shall collect again and redraw the labels.
Acceptance:
- scenario: WHEN Hyprland reports the window 10 units to the right after collection THEN no pointer event is sent and a new collection runs.

### The grid

### REQ-GRID-01: Every point is reachable
When `thodu grid` runs, or Space is pressed in a session with no focused target, the system shall cover the monitor with a labelled grid of at most 26 by 26 cells; choosing a cell refines it the same way until a cell is at most 10 logical units, and Return then clicks its center as REQ-ACT-01 and REQ-ACT-02 describe.
Acceptance:
- property: for any monitor size and any point on it, some sequence of at most four choices ends on a cell containing the point.

### The Omarchy shell

### REQ-SHELL-01: The bar is a source
When a session starts, the overlay shall report the bar's clickable items on the focused window's monitor, with their bounds in logical coordinates, as `shell` targets.
Acceptance:
- scenario: a QML test with a fake bar of three items reports three targets with their bounds.

### Command line

### REQ-CLI-01: Targets for agents
When `thodu targets --json` runs, the system shall print the focused window's targets as one JSON object per line, in the shape of section 5, without showing labels.
Acceptance:
- cases: the replayed trees of REQ-SRC-02 printed through the command match their `.targets.jsonl` files.

### Installing

### REQ-INSTALL-01: Works on Omarchy as installed
When `thodu install` runs, the system shall set `ACCESSIBILITY_ENABLED=1` and `QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1` in the Hyprland session environment, install a udev rule giving the logged-in user `/dev/uinput` through `uaccess`, install and enable the overlay plugin and the `thodud` user service, and bind the keys GG chose in his Lua configuration; and `thodu doctor` shall report each of these as present or missing.
Acceptance:
- scenario: on a fake home directory, `thodu install` then `thodu doctor` reports every item present, and `thodu doctor` alone reports them missing.

## 7. Interfaces

The IPC is JSON lines over the Unix socket `$XDG_RUNTIME_DIR/thodu.sock`, one request and one reply per line, each with a `v` field equal to 1.

- `{"v":1,"op":"start","mode":"hints"|"grid","scope":"focused"}` returns `{"v":1,"ok":true,"session":ID,"status":TEXT}` and summons the overlay with the session. An optional `window` (a Hyprland address) replaces the focused window, `overlay` false runs the session over the IPC alone, and `test` true summons the overlay without the keyboard, for live checks.
- `{"v":1,"op":"session","session":ID}` returns the session's monitor, targets and labels and its view for the overlay; with `"brief":true`, the view alone, for the overlay's heartbeat (REQ-SESSION-03).
- `{"v":1,"op":"key","session":ID,"key":NAME,"shift":B,"ctrl":B}` returns the session's new view: its state (showing, acting or closed), the targets still kept, the focused target, the search and the typed label.
- `{"v":1,"op":"hidden","session":ID}`: the overlay's surface is gone, so a pending action may run (REQ-ACT-01); `thodud` acts anyway 400 ms after the action if none comes.
- `{"v":1,"op":"targets"}` returns the focused window's targets without a session, or those of an optional `window`.
- `{"v":1,"op":"close","session":ID}`.
- `{"v":1,"op":"listen"}`: the overlay's standing connection. After its reply, `thodud` writes `{"v":1,"event":"open","session":ID,"test":B,"reply":R}` on it for each new session, R being the session's full `session` reply.
- `{"v":1,"op":"drawn","session":ID}`: the overlay drew the session's labels for the first time. `session` replies carry `timing`: `collect_ms`, and `drawn_at_us` once drawn, in microseconds since the Unix epoch.

A `window` is a Hyprland address, or `class:NAME` for the first window of that class. `thodu bench [--sessions N] [--window W]` times N sessions from `thodu hints` starting to `drawn`, with the overlay summoned without the keyboard, prints the median and the 95th percentile, keeps the samples under `~/.local/share/thodu/bench/`, and exits 1 above REQ-PERF-01's thresholds.

`start` with mode grid needs no focused window and covers the focused monitor; `thodu grid [--window W] [--test] [--no-overlay]` starts one. The view carries `generation`, bumped when the targets change, and `linger`, true while the overlay keeps the keyboard after a click (REQ-ACT-02); a `key` reply carries the new targets when the key changed them. Below the monitor's level the overlay draws the grid's cells again, magnified, beside the outlined area.

`{"v":1,"op":"shell-items","screen":NAME,"items":[{"x","y","w","h","text"}]}` carries the bar's items on the monitor NAME, relative to the bar's window; the shell source places them at the bar's layer (namespace `omarchy-bar`) as Hyprland reports it. Labels go to the window's targets before the bar's, so the window keeps the shortest ones.

`{"v":1,"op":"record","window":W}` returns every frame of the window's process, walked in full and woken if dormant, as `snapshot` in the shape of `spec/cases/atspi/*.json`; `thodu record-tree --out FILE [--window W]` writes it. A recorded node carries `action`, the name of its first action; an element whose first action is "click" is a target, with the role button when its own role is not a clickable one.

Errors are `{"v":1,"ok":false,"error":KIND,"message":TEXT}` with KIND one of no-window, no-tree, busy, stale, internal, bad-request. The command line prints an ERROR line with the message and exits 1 on any error, 2 on bad arguments.

Keys inside a session: letters search (REQ-SEARCH-01), Shift with letters types a label, Tab and Shift-Tab move the focus, Return, Shift-Return, Control-Return and a double Return act (REQ-ACT-01, REQ-ACT-02), Space opens the grid, Backspace deletes the last letter, Escape ends the session.

## 8. Unwanted behavior

### REQ-SESSION-04: Nothing to label
If no source finds a target, then the system shall say so in the status line and open the grid.
Acceptance:
- scenario: WHEN every fake source returns nothing THEN the grid opens and the status line says that no targets were found.

### REQ-SESSION-05: No accessibility for the app
If the focused window's process has not registered on the accessibility bus, then the status line shall say which variable or flag the app needs, and the session shall continue with the other sources.
Acceptance:
- cases: `spec/cases/atspi/unregistered.jsonl`, an unregistered Chromium and an unregistered GTK3 app with the expected message.

### REQ-CLI-02: No Hyprland
If Hyprland's request socket cannot be reached, then `thodu` shall print an ERROR line saying so and exit 1, without starting a session.
Acceptance:
- scenario: WHEN `HYPRLAND_INSTANCE_SIGNATURE` names no socket THEN `thodu hints` exits 1 with ERROR.

## 9. Quality attributes

### REQ-PERF-01: Labels fast enough to use
The time from `thodu hints` starting to the overlay drawing labels shall be at most 100 ms at the median and 250 ms at the 95th percentile over 50 sessions on the GTK3 reference window (spec/reference/gtk3.py) and on Slack's app window, on GG's laptop (provisional; SEED P4).
Acceptance:
- measured: `thodu bench --sessions 50` prints both percentiles and exits 1 above the thresholds.

Determinism: the same targets in the same order get the same labels. `thodud` uses no CPU while idle and under 50 MB of memory (provisional).

## 10. Verification and the gate

Unit tests and properties cover labels, search, the grid, coordinates and the stale check. Conformance cases in `spec/cases/` are language-independent and run by one runner. Accessibility trees are recorded once from real apps on GG's machine by `tools/record-tree` (a task delivers it) and replayed through a fake source, so tests need no display. The overlay's QML tests use a fake IPC and run offscreen. Live checks against real apps on GG's session are a nightly step, not a gate step, until the open question on a private display with an accessibility bus is answered.

`tools/gate --turn` runs the unit tests, properties and QML tests; `tools/gate` adds the replayed conformance suite; `tools/gate --nightly` adds the live checks and `thodu bench`. A task is done when its acceptance commands pass and `tools/gate` passes.

## 11. Protected checks

`spec/cases/**` and the recorded trees in it, the thresholds in REQ-SRC-02 and REQ-PERF-01, and `tests/acceptance/**`, beyond what `tools/protected.txt` lists.

## 12. Open questions

- O1. Rust or another language for `thodud`: consequence, every later task; interim, Rust, measured on the GTK3 reference window at 61.2 ms median and 72.6 ms at the 95th percentile (release build, 50 sessions, 2026-10-21); next action, Slack's window after task 0005; owner, the coordinator, GG if it changes SEED.
- O2. An accessibility bus inside a private headless display, so live checks can run without GG's session: interim, recorded trees and nightly live checks on his session; next action, a task after 0004.
- O3. Labels-only or search-first as the default: interim, search-first as Homerow does; next action, GG's use after the first slice.
- O4. The keys that start a session: interim, none bound by default; next action, GG chooses from his free bindings at install.

## 13. Changes and retired requirements

- 2026-10-21: created.
- 2026-10-21: change 0001, reference windows replace the widget-factory demos in the toolkit and latency requirements.
- 2026-10-21: change 0002, the requests the overlay and live checks need, in section 7.
- 2026-10-21: change 0003, sessions pushed to a listening overlay, the bench, and O1's first measurement.
- 2026-10-21: change 0004, where the pointer goes back to, the wait for a second Return, and the grid's interface.
- 2026-10-21: change 0005, recording trees, and the action that marks a clickable element.
- 2026-10-21: change 0007, the bar's items come from a bar widget of the overlay plugin.

### Retired

None yet.
