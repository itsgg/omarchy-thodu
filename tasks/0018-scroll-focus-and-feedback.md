# 0018: Keep scroll focus and distinguish the active area

Status: claimed
Depends on: none
Spec: REQ-SESSION-07, REQ-ACT-06
Goal: Keep scroll keys inside Thodu while wheel input is passed to the selected application, and make the selected scroll area easy to distinguish from a click target.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SESSION-07 REQ-ACT-06`
- scenario: In a live Hyprland session with focus-following enabled, tap and hold H/J/K/L across multiple wheel actions; none of those keys appear in the target application's input, the mode remains open, and Escape closes it.
- scenario: The selected scroll area has a distinct, high-contrast outline while normal hint selection keeps its existing style; the visible key cue still names Escape as the exit.

## Log

- 2026-10-24: created after live feedback reported scroll keys reaching the underlying terminal; acceptance covers retained keyboard capture and scroll-area feedback.
