# 0010: Clear hint selection and activation

Status: review
Depends on: none
Spec: REQ-SEARCH-01, REQ-SEARCH-02, REQ-ACT-01, REQ-ACT-02, REQ-SESSION-01
Goal: Make hint selection visually clear, keep status feedback compact, and let an uppercase final label character activate a target while lowercase completion only focuses it.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SEARCH-01 REQ-SEARCH-02 REQ-ACT-01 REQ-ACT-02 REQ-SESSION-01`
- A QML test checks the selected label treatment, target outline behavior and bounded wrapped status line.
- Shifted uppercase input activates a one-letter D hint, lowercase completion focuses, and OS-generated repeats cannot activate a label or count as a deliberate second Return.

## Log

- 2026-10-22: created from GG's request to improve overlay appearance and research uppercase hint activation.
- 2026-10-22: Added case-aware label activation, selected-chip feedback and a bounded wrapping status toast. Acceptance passed: 48 Rust tests and 24 QML tests.
- 2026-10-22: Reproduced the live failure. Qt's shifted D event carried lowercase text with Shift set, so the overlay sent lowercase d and only focused the target. Hyprland repeats keys after 250 ms while Thodu accepts a second Return for 300 ms. Updated key translation to honor Shift and ignore auto-repeated Return. A live backend check confirmed the d target is workspace 2; the overlay event tests failed on both causes before the fix.
- 2026-10-22: Independent review found the new uppercase path had not been applied to conformance input, could let a held shifted key complete a label, and started a double-click wait after uppercase activation. Updated the requirement tests to use the protocol's `Upper` key, filtered shifted-letter repeats, made uppercase activation one click, preserved all existing click requirements in the proposal, and fixed the label-focus outline for new prefixes and fine grid cells. The SPEC proposal and acceptance now describe these behaviors.
