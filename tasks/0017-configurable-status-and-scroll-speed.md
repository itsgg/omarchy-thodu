# 0017: Configurable status detail and hold-to-scroll

Status: review
Depends on: none
Spec: REQ-SESSION-08, REQ-ACT-05, REQ-ACT-06
Goal: Keep keyboard guidance visible while making routine collection details temporary or configurable, and support holding scroll keys without leaking key events to the app.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SESSION-08 REQ-ACT-05 REQ-ACT-06`
- scenario: Set status detail to auto, always and off and confirm that warnings and live search or label input remain visible in every mode.
- scenario: Set normal and Shift wheel steps, including a Shift step smaller than the normal step, and confirm all directions use the configured signed detent counts.
- scenario: Hold and release each scroll direction while the overlay keeps keyboard capture and passes wheel input through to the selected app; confirm release stops repeats. Defaults remain one detent and five with Shift.

## Log

- 2026-10-24: created from GG's request to reduce persistent status noise and make scroll speed configurable.
- 2026-10-24: added auto/always/off status detail, configurable scroll steps, and held H/J/K/L scrolling with keyboard capture retained; acceptance passed (38 QML tests) and `tools/gate` passed in 170.5 s.
