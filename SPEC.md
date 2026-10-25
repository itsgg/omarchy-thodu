# Thodu: Specification

Created: 2026-10-21. Owner: GG. Derives from SEED.md at commit 23158cc, sections 1, 4, 5, 9 and 10. The spec's version is the git commit of this file.

Precedence: SEED.md, then this spec, then tasks, then the code. A conflict with SEED.md goes to GG. Agents never edit this file: they propose changes in `changes/` (format in `changes/README.md`), and the coordinator merges them.

## 1. Conventions

This spec fixes what parallel agents would otherwise build differently: the components and their boundaries, the target model, coordinates, the IPC, the key bindings inside a session, and how each behavior is checked. MUST and SHALL bind; SHOULD is the default unless a recorded reason says otherwise; MAY is optional. A value marked provisional was chosen without measurement and is replaced by the first measurement through a change.

Requirement IDs are REQ-<AREA>-<NN>, with these areas: SESSION (starting and ending a session), SRC (sources of targets), LABEL, SEARCH, ACT (actions), GRID, SHELL (the Omarchy shell source and overlay), CLI, INSTALL, PERF. Acceptance forms in use: conformance cases in `spec/cases/`, properties with a fixed seed, recorded accessibility trees replayed through a fake source, and measured thresholds.

## 2. Scope and non-goals

This spec covers SEED section 9's first coherent experience: click mode with search and the grid, plus keyboard scroll mode, across every visible window on the focused window's monitor, on GG's laptop. Targets come from the accessibility tree for GTK3, GTK4, Qt Widgets, Electron apps and Chromium's own controls, the Omarchy bar, and a bounded image supplement after primary targets. Every click action, scroll direction and Escape work.

Out of this milestone, and designed for so that no code assumes they never come: direct web page targets (SEED section 5, source 2), full screen-image coverage for foot and Zed, drag and hover, the agent interface beyond `thodu targets`, more than one monitor. REQ-SRC-06 adds bounded image targets to every visible-window collection; it does not promise full image coverage. Permanently out (SEED section 9): desktops other than Omarchy.

## 3. Terms

- Target: something a mouse could act on, with bounds, a role, text and a source (section 5).
- Label: the letters drawn over a target; typing it clicks the target, and typing its final letter with Shift selects it.
- Visible window: a mapped, non-hidden client on the focused window's monitor that is on its active workspace, pinned, or on a shown special workspace.
- Source: where targets come from: `atspi`, `shell`, a bounded `image` supplement for every visible-window collection, a window-sized fallback in scroll mode, and `grid` in this milestone; full `image` coverage and direct `web` targets later.
- Session: one press of the key until an action or Escape.
- Logical coordinates: Hyprland's layout coordinates, in which a window's `at` and `size` are given; one logical unit is `scale` physical pixels on that monitor (1.25 on eDP-1).
- Focused window: Hyprland's `activewindow`.
- Overlay: the omarchy-shell plugin that draws labels and takes the keyboard during a session.

## 4. Architecture

Four components, each with one job:

1. `thodud`, the service: collects targets from the sources, assigns labels, owns sessions, performs actions, and answers the IPC of section 7. It runs as a systemd user service started with the session.
2. The overlay, `io.github.itsgg.thodu`: an omarchy-shell plugin of kind overlay (QML). It draws one session's labels on the focused window's monitor, takes the keyboard exclusively, reports keys to `thodud`, and lists the shell's own items for the `shell` source. It holds no target logic. It is kept loaded and listens on `thodud`'s socket, so `thodud` pushes each session to it and summons it through omarchy-shell only when nothing listens. The same plugin is a bar widget too: `BarProbe.qml`, which draws nothing, finds the bar's clickable items once a second and sends them to `thodud` when they change.
3. `thodu`, the command line: starts sessions (`thodu hints`, `thodu grid`, `thodu scroll`), and reads and acts for agents (`thodu targets`). Hyprland keybindings call it.
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
| `source` | string | atspi, shell, image, window or grid |
| `actions` | list | of click, right, middle, double, focus or scroll |
| `window` | string | Hyprland window address, empty for shell targets |

A session holds its id, the window and its geometry at collection, the monitor's name, logical origin and scale, the targets, the labels, and the time each source took.

Invariants, checked as properties:

- REQ-LABEL-01 makes labels distinct and prefix-free.
- Every target's bounds lie inside its window's bounds at collection and have positive width and height; anything else is dropped before labelling.
- Converting a point from logical to physical coordinates and back moves it by at most one logical unit.

## 6. Behavior

### Sessions

### REQ-SESSION-01: Labels on visible windows
When `thodu hints` runs and a window is focused, the system shall show a label for each target of every visible window on the focused window's monitor and of the Omarchy bar on that monitor. A `scope` of `focused` limits a request to the active window, and an explicit `window` selects one window. While focus comes from text search or keyboard navigation, the overlay shall outline the target bounds. After a label focuses a target, it shall hide the target outline and emphasize the label chip; while a label prefix is incomplete, it shall not leave the previous target outlined as if it were still focused. The status toast shall wrap within the monitor width and a maximum width of 640 logical units. Its cue shall describe the current search-first or labels-only behavior accurately. The session view carries additive `label_selected` and `labels_only` fields for this presentation.
Acceptance:
- replay: each recorded tree in `spec/cases/atspi/*.json` through the fake source yields the targets listed beside it in `spec/cases/atspi/*.targets.jsonl`.
- measured: REQ-PERF-01.
- scenario: a QML test checks label focus, search focus, a mode-accurate cue and a wrapped, bounded status toast.
- scenario: a request without `scope` uses the visible windows, while `scope: focused` and an explicit `window` each restrict collection to one window.

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
When collecting from the accessibility tree, the system shall inspect the focused window's process and match its shown ACTIVE frame for that focused window. For a non-focused window, including an explicitly selected window that is not focused, it shall ignore ACTIVE frames and match only a unique shown frame from that process whose extents match that window's size. A collection shall assign an AT-SPI frame to at most one Hyprland window; if more than one window claims the same frame, it shall omit those windows. It shall never match by application name.
Acceptance:
- cases: `spec/cases/atspi/match.jsonl`, pairs of a Hyprland window and a registry snapshot with the expected frame, including two frames from one Chromium process and an app whose name differs from its class.
- scenario: a non-focused window cannot use an ACTIVE sibling frame, even when its extents happen to match; a frame claimed by multiple windows is omitted for all claimants.

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
While collecting, the system shall give each source a time budget (80 ms for atspi, 180 ms for image, and 40 ms for shell, provisional) and leave a source that misses it out of the session, naming it in the status line. The image source shall run only after a primary source requests fallback.
Acceptance:
- scenario: WHEN a fake source sleeps past its budget THEN the session shows the other sources' targets and the status line names the slow source, including a requested image fallback.

### REQ-SRC-06: Fill accessibility gaps from the screen image
For every visible-window collection, the system shall run the bounded image detector after primary sources, including when AT-SPI found targets, so visually apparent controls omitted from an application's accessibility tree can be added. It shall capture the focused monitor once, map a detection only when its click point belongs to a visible window, assign it to the topmost such window, and discard candidates substantially covered by AT-SPI targets for that window, shell targets, or the overlay's rendered label chips, focus ring and status toast. It shall convert detections to logical coordinates and clip them to that window and monitor. Image targets shall offer click only, have no invented text, and use a visibly different label style. If capture or detection fails, or exceeds the image source's 180 ms budget, it shall contribute no image targets and name the failure or timeout in status. In an interactive hints session, primary targets shall draw before image detection completes; the session shall then update to the image targets in a new generation. While image collection is pending or the overlay has not loaded the current target generation, hint keys shall not select labels and Escape shall remain available. Each key request shall include the generation shown by the overlay. The daemon shall ignore a stale-generation key other than Escape and return the current targets and view for the ignored key.
Acceptance:
- scenario: a synthetic monitor frame yields correctly addressed image targets with logical bounds and click-only actions; detections covered by AT-SPI targets or overlay geometry are discarded.
- scenario: an interactive hints session draws primary targets before image detection completes, then updates to image targets in a new generation; keys cannot select labels until that generation is ready, and Escape remains available.
- scenario: the overlay reports its rendered chip, focus-ring and toast rectangles on the first-draw event; the fallback waits for that event and uses those exact bounds, while clients without the optional bounds use the conservative estimate.
- scenario: empty and populated AT-SPI results request image supplementation; an unavailable tree or AT-SPI error also permits image collection.
- scenario: a detection outside every visible window is discarded; a detection in an overlap is assigned to the topmost visible window.
- scenario: a key request carrying an old generation returns current labels without changing focus or queuing an action.
- scenario: Escape with an old generation still closes the session.
- scenario: the overlay suppresses key input while fetching a new generation and includes the loaded generation in subsequent requests.
- scenario: an image update keeps every previously displayed primary target label and assigns added targets labels from a reserved prefix-free namespace.
- scenario: one image detection spanning several adjacent accessible controls is not added as a duplicate target.
- scenario: a QML test checks that an image target's label chip uses the image-source style.
- measured: the supplied Flea screenshot's visible file rows receive image labels within the 180 ms image-source budget, and `thodu bench --sessions 50` remains within REQ-PERF-01 on the GTK3 reference window and Slack.

### REQ-SRC-07: Targets from every visible window
In default hints mode, the system shall collect targets from each visible window. It shall exclude windows on inactive workspaces and other monitors. One session shall assign labels across those windows and the bar. Every target shall retain its own Hyprland window address for stale checks and activation. In every hints scope, it shall omit a target whose click point belongs to a different topmost visible window. Before activation in any scope, it shall verify that the target's window is still topmost at that point and refresh if ownership changed. If multiple same-process frames cannot be matched to a non-focused window unambiguously, the system shall omit that window's AT-SPI targets rather than use another window's frame.
Acceptance:
- scenario: Hyprland client fixtures include active-workspace, pinned, shown-special, inactive-workspace, hidden, unmapped, and other-monitor windows; only the visible windows enter the session.
- scenario: two visible windows from different processes produce address-scoped targets in one session; acting on either target checks and uses its own window address.
- scenario: two same-process frames with distinct sizes match their corresponding windows; equal-size ambiguous background frames are omitted, while the focused window may use its ACTIVE frame.
- scenario: monitor image detections belong only to the topmost visible window at their click point; accessible coverage suppresses duplicates.
- scenario: a floating window owns overlap with a tiled window even when the tiled window has a more recent focus history; if another window moves over a target before activation, the session refreshes instead of clicking the occluder.
- scenario: in focused or explicit-window scope, AT-SPI targets covered by another visible window are omitted, and activation rechecks ownership before clicking.
- live: default hints on a monitor with two visible apps reports targets from both window addresses; `--window` remains scoped to that window.

### REQ-SRC-08: Scroll areas across visible windows
When scroll mode starts, the system shall collect visible AT-SPI scroll panes and web document viewports from each window in scope. It shall use one window-sized fallback area for a window with no semantic scroll area, clip areas to the window and monitor, keep the owning window address, and omit areas whose center is owned by another visible window. If there are no app windows, it shall return an empty area list and a status message without failing.
Acceptance:
- scenario: a replayed tree with nested scroll panes produces separate visible areas in window coordinates, while hidden and off-window panes are omitted.
- scenario: a Chromium document with no scroll-pane role is exposed as a scroll area; a window with no semantic area gets one window fallback.
- scenario: areas in overlapping windows retain only the topmost window's areas at overlapped points.
- scenario: zero visible app windows returns an empty result and status instead of a no-window error.

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
While a session is showing in search-first mode, lowercase letters typed without Shift and with no active label prefix shall keep only the targets whose text contains the typed letters, ignoring case and spaces, and focus the first in reading order. In labels-only mode, lowercase letters type labels until `/` starts a search. Shifted letters type a target label in either mode. Lowercase letters continue an active label prefix. Completing a label with a lowercase final character clicks its target according to REQ-ACT-01; completing it with an uppercase final character selects the target without clicking, for Return, Shift-Return, Control-Return or a double Return to act on. A label that is hidden by the current search shall not activate or take focus. An invalid label prefix shall leave the prior focus intact. Operating-system key repeats shall not complete or activate a label. A key typed with Control, Alt or Super held shall neither search, type a label, select a scroll area nor scroll; Escape keeps its meaning with any modifier, and Return with Control is the Control click of REQ-ACT-02. Tab and Shift-Tab move the focus through the kept targets. This follows Homerow's defaults (SEED R1).
Acceptance:
- cases: `spec/cases/search/filter.jsonl`, typed input against target texts with the expected kept targets and focus.
- scenario: a session test covers an uppercase prefix completed in lowercase in search-first mode, which clicks, and a label typed wholly with Shift, which selects.
- scenario: session tests cover hidden labels, invalid prefixes, prefix cancellation with `/`, and focus restoration on Backspace.
- scenario: a QML test covers a shifted key whose event text is lowercase, ignores Caps Lock text as a selection signal, and filters repeats while label input is active but preserves search repeats.
- scenario: QML and daemon tests send Control, Alt and Super chords on letters, digits, Space, Tab, `/`, Backspace and scroll keys; none searches, types a label, selects an area or scrolls, Escape with a modifier still ends the session, and Control-Return still clicks with Control held.

### REQ-SEARCH-02: Labels-only mode
The setting `labels_only` defaults to true. Where it is true, letters without Shift type labels and `/` starts a search; where it is false, a session is search-first as REQ-SEARCH-01 describes.
Acceptance:
- cases: `spec/cases/search/labels-only.jsonl`.

### Actions

### REQ-ACT-01: Return clicks
When Return is pressed with a focused target, or a target label is completed with a lowercase final character, the system shall move the pointer to the target's center, click the left button there through the virtual pointer, end the session and return the pointer to where it was, unless Hyprland's focus follows the mouse and that point is not over the clicked window, where returning it would take the keyboard from that window; then the pointer stays on the target. A session started with `test` always returns it. Completing a label with an uppercase final character selects the target without acting. A label click performs one click and does not begin the double-Return wait. Since an uppercase final character never clicks, no click is sent while Shift is held and none waits for its release.
Acceptance:
- scenario: WHEN a session on a recorded tree focuses a button at bounds (100, 200, 80, 30) on a monitor at scale 1.25 THEN the fake pointer receives a move to (140, 215) logical and one left click.
- scenario: WHEN focus follows the mouse and the pointer was over another window, or over none while the click was on a window, THEN it is not returned; over the same window, over none after a click on none, or with focus on click, it is.
- scenario: session tests type a two-letter label with lowercase completion, which clicks once with no double-Return wait, and with uppercase completion, which selects without clicking, after which Return and Shift-Return act.
- scenario: a QML test sends a Shift-completed label at once, with no wait for Shift release and no release cue.

### REQ-ACT-02: The other clicks and deliberate double Return
When Shift-Return is pressed the system shall right-click, when Return is pressed twice within 300 ms it shall double-click, and when Control-Return is pressed it shall click with Control held. After a plain Return on a target that is not an entry, the overlay keeps the keyboard for 300 ms, drawing nothing and letting the pointer through, so a second Return can come; any other key ends the session. After an entry, a right click or a Control click it gives the keyboard back at once. A key event generated by operating-system key repeat shall not count as the second Return.
Acceptance:
- cases: `spec/cases/act/keys.jsonl`, key sequences with the expected pointer and keyboard events and whether the overlay keeps the keyboard.
- scenario: a QML key adapter ignores an auto-repeated Return while forwarding a distinct Return.
- scenario: a QML key adapter ignores auto-repeated shifted letters so they cannot complete an unintended click.
- scenario: two distinct Return presses within 300 ms make one double click; a later Return does not.

### REQ-ACT-03: Never act on a stale target
If the target's window has moved, resized, closed or left the active workspace since collection, then the system shall not act; it shall collect again and redraw the labels.
Acceptance:
- scenario: WHEN Hyprland reports the window 10 units to the right after collection THEN no pointer event is sent and a new collection runs.

### REQ-ACT-04: Scroll the selected area
In scroll mode, `j` and `k` shall send vertical wheel detents down and up, and `l` and `h` shall send horizontal wheel detents right and left, at the selected area's center. The number of detents is configured by REQ-ACT-05. Scrolling shall not click, shall keep the selected area and mode, and shall return the overlay to its normal input region after the wheel event. The input path shall support GTK, Qt, Chromium and Electron through the installed uinput device. If the owning window moved, closed or is no longer topmost before the wheel event, the system shall send no wheel event and shall report that the session must be restarted.
Acceptance:
- scenario: REQ-ACT-04 unit tests verify all four directions and configured detent amounts, with no button events.
- scenario: a daemon test waits for the overlay's pointer-pass-through-ready event, sends the wheel event to the selected point, and returns the same session to showing.
- scenario: a moved or occluded window produces no wheel event.
- live: scroll a GTK or Qt pane and a Chromium or Electron document, then confirm the overlay remains in scroll mode.

### REQ-ACT-05: Configure scroll distance
`~/.config/thodu/config.toml` shall accept `scroll_step` and `scroll_shift_step`, each an integer wheel detent count from 1 through 100. Their defaults shall be 1 and 5 respectively. A normal H/J/K/L press shall send `scroll_step` signed detents; a Shift-H/J/K/L press shall send `scroll_shift_step` signed detents. The Shift step may be smaller than the normal step. Invalid values shall make the configuration report an error and use defaults for the session.
Acceptance:
- scenario: config tests verify both defaults, custom values, invalid bounds and the ability for the Shift step to be smaller than the normal step.
- scenario: session tests verify each direction uses its configured signed detent count.

### REQ-ACT-06: Repeat scroll while a key is held
While scroll mode is active, pressing H/J/K/L shall send one wheel action immediately and repeat it at 80 ms intervals until the same key is released. The repeat shall use the step size selected at key-down. Operating-system key-repeat events shall not add actions. During each wheel action, the overlay shall remain mapped and retain exclusive keyboard focus while a small pointer input region over the selected area's center passes through, so wheel events reach the selected area and physical key releases do not reach the application. The pointer shall return to its prior safe position when the hold ends, not after each wheel action. Releasing the key shall stop further actions. Pressing a different non-modifier key shall stop the current repeat before processing that key. Escape shall end scroll mode and stop repetition.
Acceptance:
- scenario: QML tests verify held H/J/K/L repeat at the 80 ms interval and release stops further requests.
- scenario: a QML test verifies OS repeat events do not create extra actions and Escape stops the repeat.
- scenario: a QML test verifies the overlay retains keyboard focus while a small pointer region passes through for wheel events.
- scenario: the pointer remains over the selected area across repeat ticks and returns to its prior safe position on release.
- scenario: a stale or failed scroll action stops repetition and leaves the session dismissible.

### The grid

### REQ-GRID-01: Every point is reachable
When `thodu grid` runs, or Space is pressed in a session with no focused target, the system shall cover the monitor with a labelled grid of at most 26 by 26 cells; choosing a cell refines it the same way until a cell is at most 10 logical units, and Return then clicks its center as REQ-ACT-01 and REQ-ACT-02 describe.
Acceptance:
- property: for any monitor size and any point on it, some sequence of at most four choices ends on a cell containing the point.

### The Omarchy shell

### REQ-SHELL-01: The bar is a source
When a session starts, the overlay shall report the bar's clickable items on the focused window's monitor, or on Hyprland's focused monitor when no app window is focused, with their bounds in logical coordinates and a stable, opaque identity, as `shell` targets. Each identity shall be unique among the reported items and stable for the lifetime of its QML object, independent of its text and bounds; a replacement object shall receive a different identity. The daemon shall accept earlier v1 reports without identity and invalidate a legacy target before activation if its item index or text changes.
Acceptance:
- scenario: a QML test with a fake bar of three items reports three targets with their bounds.
- scenario: QML tests verify an item's identity stays stable when its text changes and across repeated scans, and a replacement object receives a different identity.
- scenario: with no active app window, the source collects cached bar items by monitor even when the app window list is empty.
- scenario: an earlier v1 bar report without identity is accepted; a legacy target remains valid while its item index, text, bounds, and bar origin match, and refreshes before activation if any changes.

### REQ-SHELL-02: The bar widget shows Thodu status and controls
When the Thodu bar widget is configured, it shall show a visible, themed control with a tooltip. The control shall indicate whether the `thodud` user service is ready. Activating it shall open a themed, keyboard-navigable menu with actions to start hints, grid, and scroll sessions; controls to change `labels_only`, `status_display`, `scroll_step`, and `scroll_shift_step`; and an action to open the Thodu configuration file in an Omarchy terminal editor. Configuration changes shall persist to `~/.config/thodu/config.toml`, preserve comments and all supported settings, use the existing value constraints, and apply to newly started sessions. If the file contains invalid or unsupported settings, the menu shall explain that it needs editing and disable direct setting changes while keeping the editor action available. Escape or an outside click shall close the menu. The bar widget shall continue to report other clickable bar items for hint sessions.
Acceptance:
- scenario: QML tests verify settings persist without dropping comments or supported settings, reject invalid TOML values, and honor the existing value constraints.
- scenario: a QML test instantiates the menu, verifies keyboard navigation, and verifies all three session mode actions close it.
- live: the visible control appears in the bar, indicates service readiness, opens and closes its menu, launches each mode, and persists setting changes.

### Command line

### REQ-CLI-01: Targets for agents
When `thodu targets --json` runs, the system shall print the targets of every visible window as one JSON object per line, in the shape of section 5, without showing labels. A `scope` of `focused` or an explicit `window` limits the output to one window.
Acceptance:
- cases: the replayed trees of REQ-SRC-02 printed through the command match their `.targets.jsonl` files.
- scenario: two visible app windows produce targets with distinct `window` addresses; focused scope and an explicit window each return only their selected window's targets.

### Installing

### REQ-INSTALL-01: Works on Omarchy as installed
When `thodu install` runs, the system shall set `ACCESSIBILITY_ENABLED=1` and `QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1` in the Hyprland session environment, install a udev rule giving the logged-in user `/dev/uinput` through `uaccess`, install and enable the overlay plugin and the `thodud` user service, and bind keys in the user's Lua configuration; `thodu doctor` shall report each of these as present or missing. On a fresh configuration with no active Thodu bindings, an install without key options shall bind `SUPER + SHIFT + F12` to `thodu hints`, `SUPER + CTRL + F12` to `thodu grid`, and `SUPER + ALT + F12` to `thodu scroll`. Re-running without options shall preserve existing Thodu bindings and add the default scroll binding if it is missing. Supplying `--scroll-key` shall set only the scroll binding; supplying both `--hints-key` and `--grid-key` shall replace those bindings, with `--scroll-key` optional.
Acceptance:
- scenario: on a fake home directory, `thodu install` then `thodu doctor` reports every item present, and `thodu doctor` alone reports them missing.
- scenario: on a fresh fake home directory, non-interactive `thodu install` writes all three F12 defaults and `thodu doctor` reports the bindings present.
- scenario: on a fake home directory with custom hints and grid bindings, install without key options preserves them and adds the default scroll binding.
- scenario: `--scroll-key` changes only the scroll binding, and supplying all explicit options replaces all three bindings.

## 7. Interfaces

The IPC is JSON lines over the Unix socket `$XDG_RUNTIME_DIR/thodu.sock`, one request and one reply per line, each with a `v` field equal to 1.

- `{"v":1,"op":"start","mode":"hints"|"grid"|"scroll","scope":"visible"|"focused"}` returns `{"v":1,"ok":true,"session":ID,"status":TEXT}` and summons the overlay with the session. For hints and scroll, omitted `scope` defaults to `visible`; `focused` limits collection to the active window. An optional `window` (a Hyprland address) selects one window, `overlay` false runs the session over the IPC alone, and `test` true summons the overlay without the keyboard, for live checks.
- `{"v":1,"op":"session","session":ID}` returns the session's monitor, targets and labels and its view for the overlay; with `"brief":true`, the view alone, for the overlay's heartbeat (REQ-SESSION-03). The view's `labels_only`, `label_selected`, `status_display` and `status_persistent` fields are additive presentation fields.
- `{"v":1,"op":"key","session":ID,"generation":N,"key":NAME,"shift":B,"ctrl":B}` returns the session's new view: its state (showing, acting or closed), the targets still kept, the focused target, the search and the typed label. The daemon ignores a stale-generation request other than Escape and returns the current targets and view; Escape always ends the session.
- `{"v":1,"op":"scroll-ready","session":ID}` says that the overlay's pointer input region is passing events through at the selected scroll area's center while keyboard focus stays exclusive; `thodud` sends the pending wheel action and returns the session to showing.
- `{"v":1,"op":"scroll-stop","session":ID}` says that the held scroll key was released; `thodud` restores the pointer to its prior safe position when no wheel action remains pending.
- `{"v":1,"op":"hidden","session":ID}`: the overlay's surface is gone, so a pending action may run (REQ-ACT-01); `thodud` acts anyway 400 ms after the action if none comes.
- `{"v":1,"op":"targets","scope":"visible"|"focused"}` returns the visible windows' targets without a session; omitted `scope` defaults to `visible`. `focused` or an optional `window` limits collection to one window.
- `{"v":1,"op":"close","session":ID}`.
- `{"v":1,"op":"listen"}`: the overlay's standing connection. After its reply, `thodud` writes `{"v":1,"event":"open","session":ID,"test":B,"reply":R}` on it for each new session, R being the session's full `session` reply.
- `{"v":1,"op":"drawn","session":ID}`: the overlay drew the session's labels for the first time. `session` replies carry `timing`: `collect_ms`, and `drawn_at_us` once drawn, in microseconds since the Unix epoch.

A `window` is a Hyprland address, or `class:NAME` for the first window of that class. `thodu bench [--sessions N] [--window W]` times N sessions from `thodu hints` starting to `drawn`, with the overlay summoned without the keyboard, prints the median and the 95th percentile, keeps the samples under `~/.local/share/thodu/bench/`, and exits 1 above REQ-PERF-01's thresholds.

`start` with mode grid needs no focused window and covers the focused monitor; `thodu grid [--window W] [--test] [--no-overlay]` starts one. The view carries `generation`, bumped when the targets change, and `linger`, true while the overlay keeps the keyboard after a click (REQ-ACT-02); a `key` reply carries the new targets when the key changed them. Below the monitor's level the overlay draws the grid's cells again, magnified, beside the outlined area.

`{"v":1,"op":"shell-items","screen":NAME,"items":[{"identity":ID,"x","y","w","h","text"}]}` carries the bar's items on the monitor NAME, relative to the bar's window; the shell source places them at the bar's layer (namespace `omarchy-bar`) as Hyprland reports it. New bar probes send an identity unique and stable for each QML object. Earlier v1 reports without identity are accepted with an index-and-text fallback, and a changed index or text makes that target stale before activation. Labels go to the window's targets before the bar's, so the window keeps the shortest ones.

`{"v":1,"op":"record","window":W}` returns every frame of the window's process, walked in full and woken if dormant, as `snapshot` in the shape of `spec/cases/atspi/*.json`; `thodu record-tree --out FILE [--window W]` writes it. A recorded node carries `action`, the name of its first action; an element whose first action is "click" is a target, with the role button when its own role is not a clickable one.

Errors are `{"v":1,"ok":false,"error":KIND,"message":TEXT}` with KIND one of no-window, no-tree, busy, stale, internal, bad-request. The command line prints an ERROR line with the message and exits 1 on any error, 2 on bad arguments.

Keys inside a hints session: in labels-only mode, the default, lowercase letters type labels until `/` starts a search, while in search-first mode they search (REQ-SEARCH-01, REQ-SEARCH-02); Shift with letters types a label; a lowercase final label character clicks its visible target and an uppercase one selects it (REQ-SEARCH-01, REQ-ACT-01); Tab and Shift-Tab move the focus, Return, Shift-Return, Control-Return and a double Return act (REQ-ACT-01, REQ-ACT-02), Space opens the grid, Backspace deletes the last letter, Escape ends the session. In scroll mode, number keys select an area, Tab and Shift-Tab cycle areas, holding H/J/K/L sends wheel actions immediately and every 80 ms until release, Shift+H/J/K/L use the configured Shift step, Return clears the number prefix, and Escape ends the session (REQ-SESSION-07, REQ-ACT-04, REQ-ACT-06).

## 8. Unwanted behavior

### REQ-SESSION-04: Nothing to label
If no source finds a target, then the system shall say so in the status line and open the grid.
Acceptance:
- scenario: WHEN every fake source returns nothing THEN the grid opens and the status line says that no targets were found.

### REQ-SESSION-05: No accessibility for the app
If the focused window's process has not registered on the accessibility bus, then the status line shall say which variable or flag the app needs, and the session shall continue with the other sources.
Acceptance:
- cases: `spec/cases/atspi/unregistered.jsonl`, an unregistered Chromium and an unregistered GTK3 app with the expected message.

### REQ-SESSION-06: Hints work without an active app window
When `thodu hints` runs and Hyprland reports no active app window, the system shall use Hyprland's focused monitor and continue collecting targets for the requested scope. If no source finds a target, it shall open the grid as REQ-SESSION-04 specifies.
Acceptance:
- scenario: with no active app window and a cached bar item, hints labels the bar item on the focused monitor.
- scenario: with no active app window and no source targets, hints opens the grid and reports that no targets were found.

### REQ-SESSION-07: Select scroll areas from the keyboard
When `thodu scroll` runs, the system shall start on the focused monitor and collect areas in the requested scope, defaulting to every visible window. It shall number areas in reading order, initially focus the first area, outline the focused area, and show the available keys, including that holding H/J/K/L repeats scrolling. Tab and Shift-Tab cycle areas. Decimal digits select an area by its one-based number, with `0` selecting area 10; digits can be combined for areas 11 and above. Return clears the number prefix without scrolling. Escape ends the mode. With no areas, the overlay shall remain dismissible and report that no scroll areas were found, without opening the click grid.
Acceptance:
- scenario: a session test verifies reading-order focus, area cycling, one- and multi-digit selection, Backspace, Return and Escape.
- scenario: a QML test verifies numeric keys are sent only in scroll mode and the overlay identifies the scroll bindings and hold behavior.
- scenario: `thodu scroll --scope focused` scopes to one app; the default mode includes all visible windows on the monitor.
- scenario: when no app window is open, `thodu scroll` opens with no areas and remains dismissible.

### REQ-SESSION-08: Keep guidance while controlling status detail
The overlay shall keep its key cue visible throughout an active session. The `status_display` setting shall accept `auto`, `always` or `off`, defaulting to `auto`. In `auto`, routine collection details shall be visible for 2000 ms after the session first draws, then hidden. In `always`, routine details shall remain visible. In `off`, routine details shall be hidden. Warnings, missing-source or budget failures, no-target states, and active collection shall remain visible in every mode. Live search text and an incomplete label prefix shall remain visible in every mode. These rules shall not apply during the click-through linger state.
Acceptance:
- scenario: a QML test verifies the cue remains visible after routine details expire in auto mode.
- scenario: QML tests verify routine details persist in always mode and are hidden in off mode.
- scenario: QML tests verify warnings, empty-target status, active collection, live search and label prefixes remain visible in off mode.
- scenario: a daemon test verifies which source and no-target statuses are marked persistent.

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
- O3. Labels-only or search-first as the default: resolved 2026-10-25 by change 0024 after GG's use; labels-only is the default and search-first a setting.

## 13. Changes and retired requirements

- 2026-10-21: created.
- 2026-10-21: change 0001, reference windows replace the widget-factory demos in the toolkit and latency requirements.
- 2026-10-21: change 0002, the requests the overlay and live checks need, in section 7.
- 2026-10-21: change 0003, sessions pushed to a listening overlay, the bench, and O1's first measurement.
- 2026-10-21: change 0004, where the pointer goes back to, the wait for a second Return, and the grid's interface.
- 2026-10-21: change 0005, recording trees, and the action that marks a clickable element.
- 2026-10-21: change 0007, the bar's items come from a bar widget of the overlay plugin.
- 2026-10-22: change 0008, fresh installs use the F12 defaults, preserve existing bindings, and accept explicit overrides; O4 is resolved, with GG's local Ctrl bindings retained.
- 2026-10-22: change 0010, uppercase final hint characters activate once, lowercase completion focuses, and selected targets and feedback are visually distinct.
- 2026-10-23: change 0011, incomplete visible accessibility list rows fall back to bounded image targets.
- 2026-10-23: change 0012, hints and target queries cover every visible window on the focused window's monitor, with per-window frame matching and image fallback.
- 2026-10-24: change 0013, ACTIVE frames belong only to the focused window and a frame cannot be reused across windows.
- 2026-10-24: change 0014, hints can draw primary labels while the bounded image fallback continues, then update the target generation.
- 2026-10-24: change 0015, image supplementation applies to every visible-window collection and session keys are generation-checked.
- 2026-10-24: change 0016, sessions without an active app use Hyprland's focused monitor and can label bar items.
- 2026-10-24: change 0017, accessible scroll areas, keyboard scroll mode, uinput wheel events and an F12-family scroll binding.
- 2026-10-24: change 0018, configurable status detail and scroll distance, plus held H/J/K/L scrolling with retained keyboard capture.
- 2026-10-25: change 0020, a visible Thodu bar control with service status, mode launchers, settings and configuration editing.
- 2026-10-25: change 0021, stable per-object bar item identities, updated `shell-items` shape, and legacy report compatibility.
- 2026-10-25: change 0022, keys typed with Control, Alt or Super neither search, type labels, select scroll areas nor scroll.
- 2026-10-25: change 0023, a Shift-completed label activates once Shift is physically released, recognised whatever symbol the layout gives the release.
- 2026-10-25: change 0024, a lowercase final label character clicks and an uppercase one selects; labels-only is the default, O3 is resolved, and change 0023's wait for Shift release is retired.

### Retired

None yet.
