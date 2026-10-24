//! Image-derived targets found by the bounded classical detector (REQ-SRC-06).

use std::io::Cursor;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use thodu_core::model::{ActionKind, Monitor, Rect, Role, Source as From, Target, Window};
use woints_core::geom::{Global, Rect as VisionRect};
use woints_vision::{ClassicalDetector, DetectParams, DetectionClass, Detector, Frame};

use crate::sources::{Found, Source, SourceTargets};

/// A screen-image source, with detector scratch memory reused across sessions.
pub struct ImageSource {
    detector: Mutex<ClassicalDetector>,
    budget: Duration,
}

impl ImageSource {
    pub fn new(budget: Duration) -> Self {
        Self {
            detector: Mutex::new(ClassicalDetector::new()),
            budget,
        }
    }
}

impl Source for ImageSource {
    fn name(&self) -> &'static str {
        "image"
    }

    fn budget(&self) -> Duration {
        self.budget
    }

    fn is_fallback(&self) -> bool {
        true
    }

    fn collect(&self, window: &Window, monitor: &Monitor) -> Found {
        self.collect_windows(std::slice::from_ref(window), monitor, &[])
    }

    fn collect_many(&self, windows: &[Window], monitor: &Monitor) -> Found {
        self.collect_windows(windows, monitor, &[])
    }

    fn collect_many_excluding(
        &self,
        windows: &[Window],
        monitor: &Monitor,
        excluded: &[Target],
    ) -> Found {
        self.collect_windows(windows, monitor, excluded)
    }
}

impl ImageSource {
    fn collect_windows(&self, windows: &[Window], monitor: &Monitor, excluded: &[Target]) -> Found {
        let started = Instant::now();
        if windows.is_empty() {
            return Ok(SourceTargets::new(
                Vec::new(),
                "image skipped with no visible windows",
            ));
        }

        let visible = monitor.logical();
        let geometry = format!("{},{} {}x{}", visible.x, visible.y, visible.w, visible.h);
        let scale = monitor.scale.to_string();
        let captured = Command::new("grim")
            // Compression level 0 keeps the PNG stream lossless while
            // avoiding capture time that would consume the detector budget.
            .args(["-l", "0", "-g", &geometry, "-s", &scale, "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .map_err(|_| "grim capture failed".to_string())?;
        if !captured.status.success() {
            return Err(format!(
                "grim capture exited with {}",
                captured.status.code().unwrap_or(-1)
            ));
        }
        if started.elapsed() >= self.budget {
            return Ok(SourceTargets::new(
                Vec::new(),
                "image capture used its budget",
            ));
        }

        let (width, height, rgba) = decode_png(&captured.stdout)?;
        let frame = Frame {
            rgba,
            width,
            height,
            rect: VisionRect::<Global>::new(visible.x, visible.y, visible.w, visible.h),
            scale: monitor.scale as f32,
        };
        let params = detect_params(started, self.budget);
        let mut detector = self
            .detector
            .lock()
            .map_err(|_| "image detector lock".to_string())?;
        let detections = detector.detect(&frame, &params);
        let boxes: Vec<_> = detections
            .iter()
            .filter_map(|d| {
                let kind = match d.class {
                    DetectionClass::TextLine => DetectionKind::TextLine,
                    DetectionClass::Link => DetectionKind::Link,
                    DetectionClass::Box => DetectionKind::Box,
                    DetectionClass::Image | DetectionClass::Unknown => return None,
                };
                Some(DetectionBox {
                    bounds: Rect::new(d.rect_px.x, d.rect_px.y, d.rect_px.w, d.rect_px.h),
                    kind,
                    confidence: d.confidence,
                })
            })
            .collect();
        let targets =
            targets_from_monitor_detections(windows, visible, monitor.scale, &boxes, excluded);
        let took = started.elapsed().as_millis();
        let count = targets.len();
        Ok(SourceTargets::new(
            targets,
            format!("{count} visual candidates from image in {took} ms"),
        ))
    }
}

fn detect_params(started: Instant, budget: Duration) -> DetectParams {
    DetectParams {
        // Keep file-row text legible on common 1920-pixel outputs while
        // bounding the detector's work and mapping boxes to source pixels.
        downscale_width: 1280,
        max_elements: 500,
        deadline: Some(started + budget),
        ..DetectParams::default()
    }
}

/// A visual region emitted by the detector, in the captured frame's pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DetectionKind {
    TextLine,
    Link,
    Box,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DetectionBox {
    bounds: Rect,
    kind: DetectionKind,
    confidence: f32,
}

/// Convert monitor-image candidates and attach each one to the frontmost
/// visible window at its click point. A candidate whose click point shifts
/// when clipped to its window is dropped, keeping its eventual click safe.
fn targets_from_monitor_detections(
    windows: &[Window],
    visible: Rect,
    scale: f64,
    detections: &[DetectionBox],
    accessible: &[Target],
) -> Vec<Target> {
    if !scale.is_finite() || scale <= 0.0 {
        return Vec::new();
    }
    detections
        .iter()
        .filter(|d| d.confidence >= 0.4)
        .filter_map(|d| {
            let logical = |v: i32| (f64::from(v) / scale).round() as i32;
            let x = visible.x + logical(d.bounds.x);
            let y = visible.y + logical(d.bounds.y);
            let right = visible.x + logical(d.bounds.right());
            let bottom = visible.y + logical(d.bounds.bottom());
            let bounds = Rect::new(x, y, right - x, bottom - y).intersect(&visible)?;
            let center = bounds.center();
            let window = windows
                .iter()
                .filter(|window| window.rect().contains_point(center.0, center.1))
                .min_by_key(|window| window.stacking_key())?;
            let bounds = bounds.intersect(&window.rect())?;
            if bounds.center() != center
                || covered_by_accessible(bounds, &window.address, accessible)
            {
                return None;
            }
            let role = match d.kind {
                DetectionKind::TextLine => Role::Text,
                DetectionKind::Link => Role::Link,
                DetectionKind::Box => Role::Unknown,
            };
            Some((window.address.clone(), bounds, role))
        })
        .enumerate()
        .map(|(i, (window, bounds, role))| Target {
            id: format!("{window}:i{i}"),
            bounds,
            role,
            text: String::new(),
            source: From::Image,
            actions: vec![ActionKind::Click],
            window,
        })
        .collect()
}

pub(super) fn covered_by_accessible(bounds: Rect, window: &str, accessible: &[Target]) -> bool {
    let area = i64::from(bounds.w) * i64::from(bounds.h);
    area > 0
        && accessible.iter().any(|target| {
            if target.source == From::Image
                || (!target.window.is_empty() && target.window != window)
            {
                return false;
            }
            target
                .bounds
                .intersect(&bounds)
                .is_some_and(|overlap| i64::from(overlap.w) * i64::from(overlap.h) * 4 >= area * 3)
        })
}

fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|_| "grim returned an invalid PNG".to_string())?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut pixels)
        .map_err(|_| "grim PNG could not be decoded".to_string())?;
    pixels.truncate(info.buffer_size());
    let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
    match info.color_type {
        png::ColorType::Rgba => rgba = pixels,
        png::ColorType::Rgb => {
            let (pixels, remainder) = pixels.as_chunks::<3>();
            if !remainder.is_empty() {
                return Err("grim PNG has truncated RGB pixels".into());
            }
            for pixel in pixels {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
        }
        png::ColorType::Grayscale => {
            for pixel in pixels {
                rgba.extend_from_slice(&[pixel, pixel, pixel, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            let (pixels, remainder) = pixels.as_chunks::<2>();
            if !remainder.is_empty() {
                return Err("grim PNG has truncated grayscale pixels".into());
            }
            for pixel in pixels {
                rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
        }
        png::ColorType::Indexed => return Err("grim PNG retained an indexed palette".into()),
    }
    Ok((info.width, info.height, rgba))
}

#[cfg(test)]
mod tests {
    use super::{DetectionBox, DetectionKind, targets_from_monitor_detections};
    use std::time::{Duration, Instant};
    use thodu_core::model::{ActionKind, Rect, Role, Source, Target, Window};

    fn window() -> Window {
        Window {
            address: "0x1".into(),
            class: "file-manager".into(),
            title: String::new(),
            pid: 1,
            at: (100, 50),
            size: (200, 120),
            workspace: 1,
            monitor: 0,
            ..Window::default()
        }
    }

    // REQ-SRC-06: image boxes are mapped from capture pixels, clipped to the
    // visible window, and omitted where a trusted AT-SPI target covers them.
    #[test]
    fn req_src_06_image_targets_are_clipped_and_deduplicated() {
        let mut file_window = window();
        file_window.size = (80, 120);
        let accessible = Target {
            id: "a0".into(),
            bounds: Rect::new(150, 70, 30, 20),
            role: Role::ListItem,
            text: "Already accessible".into(),
            source: Source::Atspi,
            actions: vec![ActionKind::Click],
            window: "0x1".into(),
        };
        let detections = [
            DetectionBox {
                bounds: Rect::new(188, 88, 24, 12),
                kind: DetectionKind::TextLine,
                confidence: 0.8,
            },
            DetectionBox {
                bounds: Rect::new(100, 75, 150, 25),
                kind: DetectionKind::TextLine,
                confidence: 0.8,
            },
        ];
        let found = targets_from_monitor_detections(
            &[file_window],
            Rect::new(0, 0, 400, 300),
            1.25,
            &detections,
            &[accessible],
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].bounds, Rect::new(100, 60, 80, 20));
        assert_eq!(found[0].source, Source::Image);
        assert_eq!(found[0].role, Role::Text);
        assert!(found[0].text.is_empty());
        assert_eq!(found[0].actions, [ActionKind::Click]);
        assert_eq!(found[0].window, "0x1");
    }

    // REQ-SRC-06: detections inside overlay labels and the toast are discarded
    // by the same mapper used for live monitor screenshots.
    #[test]
    fn req_src_06_image_mapper_omits_overlay_detections() {
        let mut app = window();
        app.at = (0, 0);
        app.size = (400, 300);
        let overlay = |id: &str, bounds| Target {
            id: id.into(),
            bounds,
            role: Role::Unknown,
            text: String::new(),
            source: Source::Shell,
            actions: Vec::new(),
            window: String::new(),
        };
        let exclusions = [
            overlay("overlay:label", Rect::new(108, 58, 24, 18)),
            overlay("overlay:status", Rect::new(100, 245, 200, 45)),
        ];
        let detections = [
            DetectionBox {
                bounds: Rect::new(110, 60, 18, 12),
                kind: DetectionKind::TextLine,
                confidence: 0.8,
            },
            DetectionBox {
                bounds: Rect::new(170, 260, 30, 20),
                kind: DetectionKind::TextLine,
                confidence: 0.8,
            },
            DetectionBox {
                bounds: Rect::new(250, 100, 20, 12),
                kind: DetectionKind::TextLine,
                confidence: 0.8,
            },
        ];

        let found = targets_from_monitor_detections(
            &[app],
            Rect::new(0, 0, 400, 300),
            1.0,
            &detections,
            &exclusions,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].bounds, Rect::new(250, 100, 20, 12));
    }

    // REQ-SRC-06: the classical detector can turn a synthetic line of glyph
    // strokes into a spatial target without a model or OCR process.
    #[test]
    fn req_src_06_classical_detector_finds_text_lines() {
        let (width, height) = (1920, 120);
        let mut rgba = vec![255; width * height * 4];
        for (x, w) in [(120, 40), (163, 60), (226, 30)] {
            for y in 70..82 {
                for px in x..x + w {
                    let i = (y * width + px) * 4;
                    rgba[i..i + 3].copy_from_slice(&[0, 0, 0]);
                }
            }
        }
        let frame = woints_vision::Frame {
            rgba,
            width: width as u32,
            height: height as u32,
            rect: woints_core::geom::Rect::new(0, 0, width as i32, height as i32),
            scale: 1.0,
        };
        let mut detector = woints_vision::ClassicalDetector::new();
        let found = woints_vision::Detector::detect(
            &mut detector,
            &frame,
            &super::detect_params(Instant::now(), Duration::from_secs(1)),
        );
        assert!(
            found
                .iter()
                .any(|d| d.class == woints_vision::DetectionClass::TextLine)
        );
        let targets = targets_from_monitor_detections(
            &[window()],
            Rect::new(0, 0, 1920, 120),
            1.0,
            &found
                .iter()
                .map(|d| DetectionBox {
                    bounds: Rect::new(d.rect_px.x, d.rect_px.y, d.rect_px.w, d.rect_px.h),
                    kind: DetectionKind::TextLine,
                    confidence: d.confidence,
                })
                .collect::<Vec<_>>(),
            &[],
        );
        assert!(!targets.is_empty());
    }

    // REQ-SRC-06 and REQ-SRC-07: visual candidates from one monitor image
    // belong to the topmost window at their click point, and AT-SPI wins.
    #[test]
    fn req_src_07_monitor_detections_are_addressed_to_the_topmost_window() {
        let mut front = window();
        front.address = "0xfront".into();
        front.at = (0, 0);
        front.size = (100, 100);
        front.focus_history_id = 0;
        let mut back = front.clone();
        back.address = "0xback".into();
        back.size = (200, 100);
        back.floating = true;
        back.focus_history_id = 3;
        let detection = |x| DetectionBox {
            bounds: Rect::new(x, 20, 20, 10),
            kind: DetectionKind::TextLine,
            confidence: 0.9,
        };
        let accessible = Target {
            id: "a0".into(),
            bounds: Rect::new(130, 20, 20, 10),
            role: Role::Button,
            text: "accessible".into(),
            source: thodu_core::model::Source::Atspi,
            actions: vec![ActionKind::Click],
            window: "0xback".into(),
        };
        let targets = targets_from_monitor_detections(
            &[front, back],
            Rect::new(0, 0, 200, 100),
            1.0,
            &[detection(20), detection(130)],
            &[accessible],
        );
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].window, "0xback");
        assert_eq!(targets[0].bounds, Rect::new(20, 20, 20, 10));
    }
}
