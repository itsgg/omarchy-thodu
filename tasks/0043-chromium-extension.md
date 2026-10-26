# 0043: Read web pages through Thodu's own Chromium extension

Status: open
Depends on: 0034, 0035, 0036, 0037, 0042
Spec: REQ-SRC-02, REQ-SRC-07
Goal: SEED section 5 (source 2) makes the browser page a source of Thodu's own: a Chromium extension, added by Thodu's installer, finds the clickable elements in every page and frame and reports their text and positions to thodud, and it runs in Omarchy's web app windows as Vimium does in Slack's (SEED R10). Electron apps keep the woken accessibility tree, as does a Chromium window without the extension. Which route Chromium uses by default is settled by measuring both on large pages such as Slack's (SEED section 12), so this task builds the extension and the measurement before choosing. Propose the spec change first in changes/: the extension's protocol to thodud (a native messaging host or the existing socket), page to screen coordinates at the monitor's scale, frames, the stale check for page targets, and how install adds the extension. If a choice needs GG (for example how the extension is installed into his Chromium profile), set the task to blocked with the question.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SRC-02 REQ-SRC-07`
- scenario: a fixture page's clickable elements reach thodud as targets with correct screen bounds, including one inside an iframe; a page that scrolls after collection makes its targets stale.
- measured: the extension and the accessibility route on Slack, for coverage and time, recorded in the Log.

## Log

- 2026-10-27: created from the 2026-10-25 review (queue item 26); founder intent since 2026-10-21 (SEED section 12, its change log).
