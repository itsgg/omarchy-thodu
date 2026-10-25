//! Image-derived targets found by the bounded classical detector (REQ-SRC-06).

use std::io::{Cursor, Read};
use std::process::{Command, Stdio};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use thodu_core::model::{
    self, ActionKind, Layer, Monitor, PixelCheck, Rect, Role, Source as From, Target, TargetCheck,
    Window,
};
use thodu_core::pixels;
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
        let windows = std::slice::from_ref(window);
        self.collect_windows(windows, windows, monitor, &[])
    }

    fn collect_many(&self, windows: &[Window], monitor: &Monitor) -> Found {
        self.collect_windows(windows, windows, monitor, &[])
    }

    fn collect_many_excluding(
        &self,
        windows: &[Window],
        monitor: &Monitor,
        excluded: &[Target],
    ) -> Found {
        self.collect_windows(windows, windows, monitor, excluded)
    }

    fn collect_many_excluding_scoped(
        &self,
        windows: &[Window],
        visible_windows: &[Window],
        monitor: &Monitor,
        excluded: &[Target],
    ) -> Found {
        self.collect_windows(windows, visible_windows, monitor, excluded)
    }
}

impl ImageSource {
    fn collect_windows(
        &self,
        windows: &[Window],
        visible_windows: &[Window],
        monitor: &Monitor,
        excluded: &[Target],
    ) -> Found {
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
        let targets = with_pixel_checks(
            targets_from_monitor_detections(
                windows,
                visible_windows,
                &monitor.layers,
                visible,
                monitor.scale,
                &boxes,
                excluded,
            ),
            &frame.rgba,
            frame.width,
            frame.height,
            visible,
            monitor.scale,
        );
        let took = started.elapsed().as_millis();
        let count = targets.len();
        Ok(SourceTargets::new(
            targets,
            format!("{count} visual candidates from image in {took} ms"),
        ))
    }
}

/// Give each image target a fingerprint of its region in the capture it was
/// detected in, to compare with a fresh capture just before a click.
fn with_pixel_checks(
    mut targets: Vec<Target>,
    rgba: &[u8],
    width: u32,
    height: u32,
    visible: Rect,
    scale: f64,
) -> Vec<Target> {
    for target in &mut targets {
        let crop = crop_in(visible, target.bounds, scale);
        target.check = Some(TargetCheck::Pixels(PixelCheck {
            region: target.bounds,
            area: visible,
            scale_permille: (scale * 1000.0).round() as u32,
            fingerprint: pixels::fingerprint(rgba, width, height, crop),
        }));
    }
    targets
}

/// Where a logical `region` lies, in physical pixels, in a capture of the
/// logical `area` taken at `scale`.
fn crop_in(area: Rect, region: Rect, scale: f64) -> (u32, u32, u32, u32) {
    let physical = |v: i32| (f64::from(v) * scale).round().max(0.0) as u32;
    (
        physical(region.x - area.x),
        physical(region.y - area.y),
        physical(region.w),
        physical(region.h),
    )
}

/// Whether an image target's region now looks markedly different from when
/// it was detected (REQ-ACT-03). A capture that fails or takes too long
/// counts as changed, so the session refreshes instead of clicking unchecked.
pub fn pixels_changed(check: &PixelCheck) -> bool {
    pixels_changed_with(check, capture_region)
}

fn pixels_changed_with(
    check: &PixelCheck,
    capture: impl FnOnce(&PixelCheck) -> Result<(u32, u32, Vec<u8>), String>,
) -> bool {
    let scale = f64::from(check.scale_permille) / 1000.0;
    match capture(check) {
        Ok((width, height, rgba)) => pixels::differ(
            &check.fingerprint,
            &pixels::fingerprint(
                &rgba,
                width,
                height,
                crop_in(check.area, check.region, scale),
            ),
        ),
        Err(_) => true,
    }
}

/// How long the recapture before a click may take.
const RECAPTURE_LIMIT: Duration = Duration::from_millis(500);

/// The monitor area the target was detected in, at the same scale, through
/// grim, read on its own thread so the image cannot fill the pipe, within
/// RECAPTURE_LIMIT.
fn capture_region(check: &PixelCheck) -> Result<(u32, u32, Vec<u8>), String> {
    let r = check.area;
    let scale = f64::from(check.scale_permille) / 1000.0;
    let geometry = format!("{},{} {}x{}", r.x, r.y, r.w, r.h);
    let mut child = Command::new("grim")
        .args(["-l", "0", "-g", &geometry, "-s", &scale.to_string(), "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "grim did not start".to_string())?;
    let mut stdout = child.stdout.take().ok_or("grim gave no output")?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = tx.send(stdout.read_to_end(&mut bytes).map(|_| bytes));
    });
    match rx.recv_timeout(RECAPTURE_LIMIT) {
        Ok(Ok(bytes)) => {
            let _ = child.wait();
            decode_png(&bytes)
        }
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            Err("grim did not capture the region in time".into())
        }
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

/// Convert monitor-image candidates and attach each one to the window that
/// owns its click point; one under a layer, such as the bar or a
/// notification, is dropped. A candidate whose click point shifts when
/// clipped to its window is dropped too, keeping its eventual click safe.
fn targets_from_monitor_detections(
    selected_windows: &[Window],
    visible_windows: &[Window],
    layers: &[Layer],
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
            let owner = model::owner_among(visible_windows, layers, center.0, center.1);
            let window = visible_windows
                .iter()
                .find(|window| Some(window.address.as_str()) == owner.window())?;
            if !selected_windows
                .iter()
                .any(|selected| selected.address == window.address)
            {
                return None;
            }
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
            check: None,
        })
        .collect()
}

pub(super) fn covered_by_accessible(bounds: Rect, window: &str, accessible: &[Target]) -> bool {
    let area = i64::from(bounds.w) * i64::from(bounds.h);
    if area <= 0 {
        return false;
    }
    let covered: Vec<Rect> = accessible
        .iter()
        .filter(|target| {
            target.source != From::Image && (target.window.is_empty() || target.window == window)
        })
        .filter_map(|target| target.bounds.intersect(&bounds))
        .collect();
    if covered.is_empty() {
        return false;
    }
    let center = bounds.center();
    if covered
        .iter()
        .any(|rect| rect.contains_point(center.0, center.1))
    {
        return true;
    }

    let mut ys = vec![bounds.y, bounds.bottom()];
    for rect in &covered {
        ys.push(rect.y);
        ys.push(rect.bottom());
    }
    ys.sort_unstable();
    ys.dedup();
    let mut covered_area = 0_i64;
    for band in ys.windows(2) {
        let (top, bottom) = (band[0], band[1]);
        if bottom <= top {
            continue;
        }
        let mut intervals: Vec<(i32, i32)> = covered
            .iter()
            .filter(|rect| rect.y < bottom && rect.bottom() > top)
            .map(|rect| (rect.x, rect.right()))
            .collect();
        intervals.sort_unstable();
        let mut merged: Option<(i32, i32)> = None;
        let mut width = 0_i64;
        for (left, right) in intervals {
            match merged {
                Some((start, end)) if left <= end => merged = Some((start, end.max(right))),
                Some((start, end)) => {
                    width += i64::from(end - start);
                    merged = Some((left, right));
                }
                None => merged = Some((left, right)),
            }
        }
        if let Some((start, end)) = merged {
            width += i64::from(end - start);
        }
        covered_area += width * i64::from(bottom - top);
    }
    covered_area * 4 >= area * 3
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
    use super::{
        DetectionBox, DetectionKind, covered_by_accessible, pixels_changed_with,
        targets_from_monitor_detections, with_pixel_checks,
    };
    use std::time::{Duration, Instant};
    use thodu_core::model::{
        ActionKind, PixelCheck, Rect, Role, Source, Target, TargetCheck, Window,
    };
    use thodu_core::pixels;

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
            check: None,
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
            std::slice::from_ref(&file_window),
            std::slice::from_ref(&file_window),
            &[],
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

    // REQ-SRC-06: accessible controls collectively covering a detection
    // suppress the duplicate even when no single control covers most of it.
    #[test]
    fn req_src_06_combined_accessible_controls_cover_image_detection() {
        let detection = Rect::new(0, 0, 120, 20);
        let accessible: Vec<Target> = (0..3)
            .map(|i| Target {
                id: format!("a{i}"),
                bounds: Rect::new(i * 40, 0, 40, 20),
                role: Role::Button,
                text: format!("item {i}"),
                source: Source::Atspi,
                actions: vec![ActionKind::Click],
                window: "0x1".into(),
                check: None,
            })
            .collect();

        assert!(covered_by_accessible(detection, "0x1", &accessible));
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
            check: None,
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
            std::slice::from_ref(&app),
            std::slice::from_ref(&app),
            &[],
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
        let app = window();
        let targets = targets_from_monitor_detections(
            std::slice::from_ref(&app),
            std::slice::from_ref(&app),
            &[],
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
    // REQ-SRC-06 and REQ-ACT-03: an image target carries a fingerprint of its
    // own region in the capture it was detected in, at the capture's scale.
    #[test]
    fn req_src_06_image_targets_carry_pixel_fingerprints() {
        let (width, height) = (250u32, 125u32);
        let mut rgba = vec![255u8; (width * height * 4) as usize];
        for y in 25..50 {
            for x in 25..75 {
                let i = ((y * width + x) * 4) as usize;
                rgba[i..i + 3].copy_from_slice(&[0, 0, 0]);
            }
        }
        let target = Target {
            id: "0x1:i0".into(),
            bounds: Rect::new(20, 20, 40, 20),
            role: Role::Text,
            text: String::new(),
            source: thodu_core::model::Source::Image,
            actions: vec![ActionKind::Click],
            window: "0x1".into(),
            check: None,
        };
        let targets = with_pixel_checks(
            vec![target],
            &rgba,
            width,
            height,
            Rect::new(0, 0, 200, 100),
            1.25,
        );
        let Some(TargetCheck::Pixels(check)) = &targets[0].check else {
            panic!("an image target carries a pixel check");
        };
        assert_eq!(check.region, Rect::new(20, 20, 40, 20));
        assert_eq!(check.area, Rect::new(0, 0, 200, 100));
        assert_eq!(check.scale_permille, 1250);
        assert_eq!(
            check.fingerprint,
            pixels::fingerprint(&rgba, width, height, (25, 25, 50, 25))
        );
    }

    // REQ-ACT-03: before a click the region is captured again; the same
    // pixels pass, different ones and a failed capture count as changed.
    #[test]
    fn req_act_03_image_pixels_are_captured_again_before_a_click() {
        let (w, h) = (40u32, 20u32);
        let dark = vec![0u8; (w * h * 4) as usize];
        let light = vec![255u8; (w * h * 4) as usize];
        let check = PixelCheck {
            region: Rect::new(0, 0, 40, 20),
            area: Rect::new(0, 0, 40, 20),
            scale_permille: 1000,
            fingerprint: pixels::fingerprint(&dark, w, h, (0, 0, w, h)),
        };
        assert!(!pixels_changed_with(&check, |_| Ok((w, h, dark.clone()))));
        assert!(pixels_changed_with(&check, |_| Ok((w, h, light.clone()))));
        assert!(pixels_changed_with(&check, |_| Err(
            "grim did not capture the region in time".into()
        )));
    }

    // REQ-SRC-07: a detection under a layer that takes input, such as the
    // bar strip or a notification's toast column, belongs to no window.
    #[test]
    fn req_src_07_monitor_detections_under_layers_are_dropped() {
        let mut app = window();
        app.address = "0xapp".into();
        app.at = (0, 0);
        app.size = (1000, 600);
        let layers = [
            thodu_core::model::Layer {
                namespace: "omarchy-bar".into(),
                level: 2,
                rect: Rect::new(0, 0, 1000, 26),
            },
            thodu_core::model::Layer {
                namespace: "omarchy-notifications".into(),
                level: 3,
                rect: Rect::new(0, 0, 1000, 600),
            },
        ];
        let detection = |x, y| DetectionBox {
            bounds: Rect::new(x, y, 20, 10),
            kind: DetectionKind::TextLine,
            confidence: 0.9,
        };
        let windows = [app];
        let targets = targets_from_monitor_detections(
            &windows,
            &windows,
            &layers,
            Rect::new(0, 0, 1000, 600),
            1.0,
            &[detection(100, 5), detection(900, 200), detection(100, 200)],
            &[],
        );
        assert_eq!(
            targets.len(),
            1,
            "only the detection on the app itself remains"
        );
        assert_eq!(targets[0].bounds, Rect::new(100, 200, 20, 10));
    }

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
            check: None,
        };
        let visible_windows = [front, back];
        let targets = targets_from_monitor_detections(
            &visible_windows,
            &visible_windows,
            &[],
            Rect::new(0, 0, 200, 100),
            1.0,
            &[detection(20), detection(130)],
            &[accessible],
        );
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].window, "0xback");
        assert_eq!(targets[0].bounds, Rect::new(20, 20, 20, 10));
    }

    // REQ-SRC-06 and REQ-SRC-07: a focused single-window scope must still
    // discard image pixels owned by a visible floating window above it.
    #[test]
    fn req_src_07_single_window_scope_omits_pixels_from_occluding_windows() {
        let mut selected = window();
        selected.address = "0xselected".into();
        selected.at = (0, 0);
        selected.size = (200, 100);
        let mut occluder = selected.clone();
        occluder.address = "0xoccluder".into();
        occluder.at = (80, 0);
        occluder.size = (120, 100);
        occluder.floating = true;
        occluder.focus_history_id = 0;
        let visible_windows = [occluder, selected.clone()];
        let detections = [
            DetectionBox {
                bounds: Rect::new(20, 20, 20, 10),
                kind: DetectionKind::TextLine,
                confidence: 0.9,
            },
            DetectionBox {
                bounds: Rect::new(90, 20, 20, 10),
                kind: DetectionKind::TextLine,
                confidence: 0.9,
            },
        ];

        let targets = targets_from_monitor_detections(
            &[selected],
            &visible_windows,
            &[],
            Rect::new(0, 0, 200, 100),
            1.0,
            &detections,
            &[],
        );
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].window, "0xselected");
        assert_eq!(targets[0].bounds, Rect::new(20, 20, 20, 10));
    }
}
