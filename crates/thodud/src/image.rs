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
        let started = Instant::now();
        let active = crate::hypr::active_window()?.ok_or("no focused window")?;
        if active.address != window.address {
            return Ok(SourceTargets::new(
                Vec::new(),
                "image skipped for a non-focused window",
            ));
        }

        let visible = Rect::new(window.at.0, window.at.1, window.size.0, window.size.1)
            .intersect(&monitor.logical())
            .ok_or_else(|| "no visible window area to capture".to_string())?;
        let geometry = format!("{},{} {}x{}", visible.x, visible.y, visible.w, visible.h);
        let scale = monitor.scale.to_string();
        let captured = Command::new("grim")
            .args(["-g", &geometry, "-s", &scale, "-"])
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
        let params = DetectParams {
            downscale_width: 1920,
            max_elements: 500,
            deadline: Some(started + self.budget),
            ..DetectParams::default()
        };
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
        let targets = targets_from_detections(window, visible, monitor.scale, &boxes, &[]);
        let took = started.elapsed().as_millis();
        let count = targets.len();
        Ok(SourceTargets::new(
            targets,
            format!("{count} visual candidates from image in {took} ms"),
        ))
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

/// Convert frame-pixel candidates to logical target bounds. `visible` is both
/// the frame origin and the area to which candidates are clipped.
fn targets_from_detections(
    window: &Window,
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
            if bounds.is_empty() || covered_by_accessible(bounds, &window.address, accessible) {
                return None;
            }
            let role = match d.kind {
                DetectionKind::TextLine => Role::Text,
                DetectionKind::Link => Role::Link,
                DetectionKind::Box => Role::Unknown,
            };
            Some((bounds, role))
        })
        .enumerate()
        .map(|(i, (bounds, role))| Target {
            id: format!("i{i}"),
            bounds,
            role,
            text: String::new(),
            source: From::Image,
            actions: vec![ActionKind::Click],
            window: window.address.clone(),
        })
        .collect()
}

pub(super) fn covered_by_accessible(bounds: Rect, window: &str, accessible: &[Target]) -> bool {
    let area = i64::from(bounds.w) * i64::from(bounds.h);
    area > 0
        && accessible.iter().any(|target| {
            if target.source != From::Atspi || target.window != window {
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
    use super::{DetectionBox, DetectionKind, targets_from_detections};
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
        }
    }

    // REQ-SRC-06: image boxes are mapped from capture pixels, clipped to the
    // visible window, and omitted where a trusted AT-SPI target covers them.
    #[test]
    fn req_src_06_image_targets_are_clipped_and_deduplicated() {
        let accessible = Target {
            id: "a0".into(),
            bounds: Rect::new(160, 70, 50, 30),
            role: Role::ListItem,
            text: "Already accessible".into(),
            source: Source::Atspi,
            actions: vec![ActionKind::Click],
            window: "0x1".into(),
        };
        let detections = [
            DetectionBox {
                bounds: Rect::new(60, 25, 50, 20),
                kind: DetectionKind::TextLine,
                confidence: 0.8,
            },
            DetectionBox {
                bounds: Rect::new(-10, 10, 40, 20),
                kind: DetectionKind::TextLine,
                confidence: 0.8,
            },
        ];
        let found = targets_from_detections(
            &window(),
            Rect::new(110, 60, 180, 100),
            1.25,
            &detections,
            &[accessible],
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].bounds, Rect::new(110, 68, 24, 16));
        assert_eq!(found[0].source, Source::Image);
        assert_eq!(found[0].role, Role::Text);
        assert!(found[0].text.is_empty());
        assert_eq!(found[0].actions, [ActionKind::Click]);
        assert_eq!(found[0].window, "0x1");
    }

    // REQ-SRC-06: the classical detector can turn a synthetic line of glyph
    // strokes into a spatial target without a model or OCR process.
    #[test]
    fn req_src_06_classical_detector_finds_text_lines() {
        let (width, height) = (300, 60);
        let mut rgba = vec![255; width * height * 4];
        for (x, w) in [(20, 40), (63, 60), (126, 30)] {
            for y in 20..32 {
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
            &woints_vision::DetectParams::default(),
        );
        assert!(
            found
                .iter()
                .any(|d| d.class == woints_vision::DetectionClass::TextLine)
        );
        let targets = targets_from_detections(
            &window(),
            Rect::new(100, 50, 200, 60),
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
}
