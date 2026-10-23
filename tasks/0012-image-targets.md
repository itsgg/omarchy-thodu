# 0012: Add bounded image targets

Status: review
Depends on: 0011
Spec: REQ-SRC-06
Goal: Fill gaps left by incomplete accessibility trees with fast image-detected text lines and box-like controls, so visible rows such as Flea's file entries can receive hints.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SRC-06`
- A synthetic frame produces image targets with clipped logical bounds, click actions, and distinct labels; AT-SPI-covered areas are not duplicated.
- Complete AT-SPI trees skip image capture; incomplete visible rows and unavailable/erroring AT-SPI request the fallback.
- The supplied Flea screenshot's visible file rows receive image labels within the 180 ms image-source budget.

## Log

- 2026-10-23: created after measuring Tesseract at about 2.7 to 3.0 seconds per screenshot; chose the bounded Woints classical detector to preserve the existing hint latency contract.
- 2026-10-23: measured the Woints detector on the supplied screenshot: 20 file-list text lines at about 112 ms in release at 1920 px; set a 180 ms image budget and run it only for incomplete AT-SPI coverage.
- 2026-10-23: SEED section 9 schedules general image coverage later; GG approved this narrower fallback for visible AT-SPI row gaps after the Flea report.
- 2026-10-23: made sparse-row detection work under generic accessibility containers and reused the target builder's row roles; `tests/acceptance/reqs.sh REQ-SRC-06` and the full gate passed.
- 2026-10-23: verified Agy's review: image targets were sorted after every AT-SPI target and skipped stale-window checks. Coordinate sorting now applies across both sources; window-scoped targets are stale-checked. Added REQ-LABEL-01 and REQ-ACT-03 regressions; all 40 core requirement tests and `tests/acceptance/reqs.sh REQ-SRC-06` pass.
