# 0009: Let the first click pass through during the double-click wait

Status: done
Depends on: 0004
Spec: REQ-ACT-02
Goal: A plain Return on a focused target shall deliver its click through the overlay and keep keyboard focus for a possible second Return unless a shell click changes workspace.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-ACT-02`
- On GG's session: choose a different workspace with one label and press Return once; it switches immediately and remains there after the 300 ms linger ends.
- On GG's session: on a target whose first click leaves the active workspace unchanged, press Return and then a second Return within 300 ms; the second Return still reaches Thodu and double-clicks.

## Log

- 2026-10-22: created after reproducing a plain Return that dismissed Thodu without activating its selected workspace target.
- 2026-10-22: On Hyprland 0.56.2, an empty input mask on an exclusive layer still receives pointer fallback events. The overlay now switches to on-demand keyboard focus while the first click passes through, then restores exclusive focus on thodud's click-complete reply. `tests/acceptance/reqs.sh REQ-ACT-02` and `tools/gate` pass (153.4s). Live check: label d moved workspace 1 to 2 on the first Return; a second Return 105 ms later reached Thodu and ran the double click. The bar changed under the same coordinates after the workspace switch, so the second click returned to workspace 1. Review this observed bar behavior with the change.
- 2026-10-22: Tightened the live acceptance after a one-Return reproduction showed workspace 1 -> 2 -> 1 about 550 ms later. The QML event log recorded one D and one Return, followed by `finished no second Return`; this is not a second Return or a second click. Investigating focus release when the overlay closes.
- 2026-10-22: The rollback came from restoring Exclusive focus after a bar click switched workspace. thodud now reports that workspace change in the click-complete reply; the overlay ends that session without reacquiring Exclusive focus. The REQ-ACT-02 acceptance suite passes. Live check: one Return on label d switched from workspace 1 to empty workspace 2 and stayed there for over one second; two Returns on the active workspace target reached QML and kept workspace 1 active. `changes/0009-workspace-switch-click.md` proposes the matching REQ-ACT-02 exception.
- 2026-10-22: Coordinator checked a reported lost second Return, which did not reproduce in the recorded live acceptance on Hyprland 0.56.2: a same-workspace target received a second Return 105 ms after the first. The overlay's `visible: view.active` transition schedules the frame used to close the panel, and its own 300 ms linger ends before the 400 ms hidden wait can leave the layer mapped; the next `show` resets all focus session fields. Reduced the workspace guard's Hyprland queries from a 5 ms loop to 25 ms polling and added a REQ-ACT-02 test that bounds queries and confirms a workspace change is observed. The full gate passed.
