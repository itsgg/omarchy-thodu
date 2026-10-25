# 0009: End a lingering click when it changes workspace

Status: merged

Merged on 2026-10-25 as part of change 0027, generalized from shell targets to any lingering click on the bar, grid cells included, after GG asked for workspace switching from the grid.
Why: Task 0009 live testing on Hyprland 0.56.2 showed a plain Return on the bar's workspace 2 target switched from workspace 1 to 2, then back to 1 when Thodu reclaimed Exclusive keyboard focus. A single Return reached QML; there was no second keypress. Keeping the layer OnDemand preserved workspace 2, but stopped a second Return from reaching Thodu. A workspace-changing bar click therefore needs to end its lingering session instead of reclaiming Exclusive focus.

## Modified

### REQ-ACT-02: The other clicks
When Shift-Return is pressed the system shall right-click, when Return is pressed twice within 300 ms it shall double-click, and when Control-Return is pressed it shall click with Control held. After a plain Return on a target that is not an entry, the overlay keeps the keyboard for 300 ms, drawing nothing and letting the pointer through, so a second Return can come; any other key ends the session. After an entry, a right click or a Control click it gives the keyboard back at once. If the first click on a shell target changes the active workspace, the overlay shall end the session after that click completes and shall not reclaim Exclusive keyboard focus.
Acceptance:
- cases: `spec/cases/act/keys.jsonl`, key sequences with the expected pointer and keyboard events and whether the overlay keeps the keyboard.
- scenario: WHEN one plain Return clicks a shell target that changes the monitor's active workspace from 1 to 2 THEN the session ends, workspace 2 stays active after 300 ms, and no second click is sent.
- scenario: WHEN two plain Returns within 300 ms click a target without changing the active workspace on the first click THEN the pointer receives two clicks.

## Tests

- `tests/qml/tst_session.qml`: a workspace-change reply ends the linger.
- `crates/thodud/src/hypr.rs`: active workspace lookup is scoped to the requested monitor.
- `crates/thodud/src/main.rs`: the lingering hidden reply reports a workspace change.
