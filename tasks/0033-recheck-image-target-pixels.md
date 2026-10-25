# 0033: Recheck an image target's pixels before clicking it

Status: open
Depends on: none
Spec: REQ-ACT-03, REQ-SRC-06
Goal: Image targets come from one screen capture, and nothing checks before the click that the screen there still shows what was detected. In Foot new output scrolls the text, so the label for one word can sit over another by the time GG types it. Keep a small fingerprint of each image target's pixels from the capture it was detected in, capture that region again just before the click, and refresh instead of clicking when the two differ markedly; a change as small as a blinking cursor stays below the threshold.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-ACT-03 REQ-SRC-06`
- scenario: an image target carries a fingerprint of its region from the detection capture; the same pixels pass, a blinking-cursor-sized change passes, and different content in the region fails.
- scenario: a click on an image target whose region changed refreshes the session instead of pressing, and a capture that fails or exceeds its time counts as changed.

## Log

- 2026-10-25: created from the review queue (item 6b) after task 0031 rechecked accessible elements; image targets had no recheck of their own.
