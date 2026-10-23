# 0011: Reach GTK rows and reveal overlapping hints

Status: review
Depends on: 0005, 0010
Spec: REQ-SRC-02, REQ-SESSION-01
Goal: Make GTK list-view rows, including Nautilus file rows, keyboard targets and make a focused hint chip visible above overlapping chips, with a cue that explains how to cycle focus.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SRC-02 REQ-SESSION-01`
- A sanitized GTK4 list-view recording replays to one target per row without duplicate targets for its grid-cell children.
- A QML test checks that a focused overlapping hint chip is drawn above the others and the cue explains Tab focus cycling.
- On GG's session: Nautilus file rows receive hints and Tab makes an overlapping hint visible.

## Log

- 2026-10-23: created after reproducing missing Nautilus file-row hints and overlapping labels on GG's screen.
- 2026-10-23: claimed by codex-hints-coverage; the GTK4 replay and overlap test fail before their fixes and pass after them.
- 2026-10-23: added GTK4 AT-SPI Cache traversal; the live Nautilus session showed 37 AT-SPI targets and 21 bar targets in 32 ms, including file rows and row buttons. Tab focused a file row in the overlay.
- 2026-10-23: `tests/acceptance/reqs.sh REQ-SRC-02 REQ-SESSION-01` and `tools/gate` passed; cargo-deny was skipped because it is not installed.
