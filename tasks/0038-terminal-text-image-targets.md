# 0038: Label terminal text by line, with chips that leave the text readable

Status: open
Depends on: none
Spec: REQ-SRC-06, REQ-LABEL-02
Goal: On a Foot window every word becomes its own image target, and the labels hide much of the text GG is trying to read. Measured on GG's screen on 2026-10-25 with two Foot windows: the detector found 722 components, the woints `merge_text` step joins components only within its 4 logical unit gap while Foot's monospace space is about 7.7 logical units at scale 1.25, so each word stays a separate text line; the 500 `max_elements` cap then cut 624 to 704 candidates to 500, dropping the lowest-confidence lines at the bottom of the screen, and 477 image targets reached the overlay. 475 of them are shorter than the 18 unit chip, so REQ-LABEL-02 centres each chip on the target's top-left corner; with opaque three-letter chips the labels covered 20.7 percent of the windows and 35.4 percent of the text's ink. The same text-heavy frame also raises detection from 25 to 29 ms on a Chromium page to 68 to 96 ms, which pushes the image pass toward its budget (task 0036). Make terminal text useful as targets: join text detections on the same line across a gap measured from the frame (or from the window's monospace advance), so a line or a run of words is one target and the count stays well under the cap; draw image-text chips smaller and beside or translucent over the text rather than opaque across it; and propose the spec change this needs to REQ-SRC-06 (what an image text target is, and that the detector is bounded in count as well as time) and REQ-LABEL-02 (placement for small text targets). Keep the woints dependency unpatched; do the joining in crates/thodud/src/image.rs. Build test frames synthetically (monospace text at Foot's spacing), never from a capture of GG's screen, since the repository is public.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SRC-06 REQ-LABEL-02`
- scenario: a synthetic monospace frame at a 7.7 unit word gap yields one target per line or word run, not per word, and stays under the detector cap.
- scenario: the chips placed on that frame cover under a stated share of the text's ink, measured in the test, and controls on a Chromium-like frame keep their targets.

## Log

- 2026-10-27: created from the 2026-10-25 image investigation (queue item 11b). The counts, gaps and coverage above were measured on GG's screen with Thodu's own detector parameters, and the live daemon reported the same 477 targets.
