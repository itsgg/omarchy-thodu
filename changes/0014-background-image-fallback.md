# 0014: Draw hints before image fallback completes

Status: merged
Why: A live check with visible Foot and Zenity windows found that the image pass could add 127 ms to collection and delay first draw to 276 ms, so hints need to draw primary targets before collecting visual targets. The file-manager screenshot then showed 20 AT-SPI targets while visible controls remained unlabeled; because the tree appeared complete, the current fallback heuristic did not run. AT-SPI reports the semantics and actions applications expose, so image detection must also supplement populated trees to find visually apparent controls apps paint themselves.

## Added

None.

## Modified

### REQ-SRC-06: Fill accessibility gaps from the screen image
For every visible-window collection, the system shall run the bounded image detector after primary sources, including when AT-SPI found targets, so visually apparent controls omitted from an application's accessibility tree can be added. It shall capture the focused monitor once, map a detection only when its click point belongs to a visible window, assign it to the topmost such window, and discard candidates substantially covered by AT-SPI targets for that window, shell targets, or the overlay's label chips, focus ring and status toast. It shall convert detections to logical coordinates and clip them to that window and monitor. Image targets shall offer click only, have no invented text, and use a visibly different label style. If capture or detection fails, or exceeds the image source's 180 ms budget, it shall contribute no image targets and name the failure or timeout in status. In an interactive hints session, primary targets shall draw before image detection completes; the session shall then update to the image targets in a new generation. While the fallback is pending, hint keys shall not select labels and Escape shall remain available.
Acceptance:
- scenario: a synthetic monitor frame yields correctly addressed image targets with logical bounds and click-only actions; detections covered by AT-SPI targets or overlay geometry are discarded.
- scenario: an interactive hints session draws primary targets before a requested image fallback completes, then updates to the image targets in a new generation; keys cannot select labels until that generation is ready, and Escape remains available.
- scenario: the overlay reports its rendered chip, focus-ring and toast rectangles on the first-draw event; the fallback waits for that event and uses those exact bounds, while clients without the optional bounds use the conservative estimate.
- scenario: a complete AT-SPI tree still requests image supplementation; an incomplete visible row, unavailable tree, or AT-SPI error also requests the fallback across the visible-window session.
- scenario: an empty AT-SPI result still requests image supplementation; synthetic image detections remain scoped to their visible window.
- scenario: a detection outside every visible window is discarded; a detection in an overlap is assigned to the topmost window.
- scenario: a QML test checks that an image target's label chip uses the image-source style.
- measured: the supplied Flea screenshot's visible file rows receive image labels within the 180 ms image-source budget, and `thodu bench --sessions 50` remains within REQ-PERF-01 on the GTK3 reference window and Slack.

## Removed

None.

## Tests

- Add REQ-SRC-06 coverage that complete primary targets still request image supplementation and that overlay geometry is excluded from the detector.
- Add REQ-SESSION-01 coverage that the overlay reports its rendered label and toast rectangles on first draw, holds selection keys while collecting, accepts Escape and reloads the new target generation through its heartbeat.
- Add REQ-SESSION-01 coverage for draw timeout, closed-session cancellation, and a session without an overlay.
- Modifies test: `req_src_06_image_fallback_is_skipped_for_complete_primary_sources`: clarify the generic collector behavior after AT-SPI starts requesting visual supplementation for complete trees.
- Modifies test: `req_src_06_req_session_01_fallback_updates_visible_targets_after_primary_draw`: cover a fallback arriving after the session has started acting.
- Modifies test: `test_req_session_01_visual_collection_holds_keys_until_ready`: check heartbeat-driven generation reload instead of manually applying the completed reply.
- Add QML coverage that the drawn event uses the visible chip, focus-ring and toast geometry, including fractional positions.
