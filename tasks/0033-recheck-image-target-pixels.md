# 0033: Recheck an image target's pixels before clicking it

Status: review
Depends on: none
Spec: REQ-ACT-03, REQ-SRC-06
Goal: Image targets come from one screen capture, and nothing checks before the click that the screen there still shows what was detected. In Foot new output scrolls the text, so the label for one word can sit over another by the time GG types it. Keep a small fingerprint of each image target's pixels from the capture it was detected in, capture that region again just before the click, and refresh instead of clicking when the two differ markedly; a change as small as a blinking cursor stays below the threshold.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-ACT-03 REQ-SRC-06`
- scenario: an image target carries a fingerprint of its region from the detection capture; the same pixels pass, a blinking-cursor-sized change passes, and different content in the region fails.
- scenario: a click on an image target whose region changed refreshes the session instead of pressing, and a capture that fails or exceeds its time counts as changed.

## Log

- 2026-10-25: created from the review queue (item 6b) after task 0031 rechecked accessible elements; image targets had no recheck of their own.
- 2026-10-25: image targets keep a fingerprint of their region (a 16 by 4 grid of average brightness) from the capture they were detected in; before a click thodud captures the same monitor area at the same scale, crops it the same way, and refuses the click when the cells differ by more than ten levels on average. A first version recaptured only the region and measured up to 7.2 levels on unchanged content at scale 1.25, from rounding; the same-area recapture compares unchanged content at zero, measured live on GG's screen, and costs about 50 ms per image click. The image pass still takes 132 to 152 ms. Tests cover the fingerprint, its attachment, the recapture and the dispatch; five planted faults were each caught. Change 0028 states it in REQ-ACT-03. Installed live.
