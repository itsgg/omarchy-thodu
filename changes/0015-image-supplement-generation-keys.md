# 0015: Align image supplementation and reject stale session keys

Status: merged
Why: Change 0014 made the bounded image detector supplement every visible-window collection, but the scope summary and task 0012 acceptance still described an incomplete-list-only fallback. The asynchronous update also allowed a key typed against old labels to reach a session after its target generation changed. Align the text and make keys identify the generation the overlay displayed.

## Added

None

## Modified

### REQ-SRC-06: Fill accessibility gaps from the screen image
For every visible-window collection, the system shall run the bounded image detector after primary sources, including when AT-SPI found targets, so visually apparent controls omitted from an application's accessibility tree can be added. It shall capture the focused monitor once, map a detection only when its click point belongs to a visible window, assign it to the topmost such window, and discard candidates substantially covered by AT-SPI targets for that window, shell targets, or the overlay's rendered label chips, focus ring and status toast. It shall convert detections to logical coordinates and clip them to that window and monitor. Image targets shall offer click only, have no invented text, and use a visibly different label style. If capture or detection fails, or exceeds the image source's 180 ms budget, it shall contribute no image targets and name the failure or timeout in status. In an interactive hints session, primary targets shall draw before image detection completes; the session shall then update to the image targets in a new generation. While image collection is pending or the overlay has not loaded the current target generation, hint keys shall not select labels and Escape shall remain available. Each key request shall include the generation shown by the overlay. The daemon shall ignore a stale-generation key other than Escape and return the current targets and view for the ignored key.
Acceptance:
- scenario: empty and populated AT-SPI results request image supplementation; an unavailable tree or AT-SPI error also permits image collection.
- scenario: an interactive hints session draws primary targets before image detection completes, then updates to image targets in a new generation; keys cannot select labels until that generation is ready, and Escape remains available.
- scenario: a key request carrying an old generation returns current labels without changing focus or queuing an action.
- scenario: Escape with an old generation still closes the session.
- scenario: the overlay suppresses key input while fetching a new generation and includes the loaded generation in subsequent requests.
- scenario: an image update keeps every previously displayed primary target label and assigns added targets labels from a reserved prefix-free namespace.
- scenario: one image detection spanning several adjacent accessible controls is not added as a duplicate target.

## Removed

None

## Tests

- Remove the obsolete tree-shape heuristic and its core tests; the daemon no longer consults it after image supplementation became unconditional.
- Extend the production AT-SPI result-builder test to cover both empty and populated results.
- Use the full visible-window set to reject covered AT-SPI and image targets in focused or explicit-window scope, and recheck target ownership before activation in every scope.
- Add a daemon regression proving stale-generation activation returns current targets without queuing an action.
- Extend the QML test to hold labels while fetching a new generation and send the loaded generation with each key request.
- Preserve the labels shown before the image update and suppress detections covered by the union of accessible bounds.
- Update task 0012 acceptance to reflect the later REQ-SRC-06 policy.
