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

/// How the monitor is captured: the logical area, its scale and how long the
/// capture may take, to PNG bytes.
type Capture = dyn Fn(Rect, f64, Duration) -> Result<Vec<u8>, String> + Send + Sync;

/// A screen-image source, with detector scratch memory reused across
/// sessions. A detection still running when the next one starts holds that
/// memory, so the next one uses scratch of its own rather than wait for it.
pub struct ImageSource {
    detector: Mutex<Box<dyn Detector>>,
    spare: Box<dyn Fn() -> Box<dyn Detector> + Send + Sync>,
    capture: Box<Capture>,
    budget: Duration,
}

/// The part of the image budget kept back from detection, for mapping the
/// detections to targets, fingerprinting them and sending them, so a
/// detection that runs to its deadline is still used in part.
fn reserve(budget: Duration) -> Duration {
    budget / 6
}

impl ImageSource {
    pub fn new(budget: Duration) -> Self {
        Self::with(
            budget,
            Box::new(grim),
            Box::new(|| Box::new(ClassicalDetector::new())),
        )
    }

    fn with(
        budget: Duration,
        capture: Box<Capture>,
        spare: Box<dyn Fn() -> Box<dyn Detector> + Send + Sync>,
    ) -> Self {
        Self {
            detector: Mutex::new(spare()),
            spare,
            capture,
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
        let detect_budget = self.budget.saturating_sub(reserve(self.budget));
        let detect_until = started + detect_budget;
        let captured = (self.capture)(
            visible,
            monitor.scale,
            detect_until.saturating_duration_since(Instant::now()),
        )?;

        let (width, height, rgba) = decode_png(&captured)?;
        let frame = Frame {
            rgba,
            width,
            height,
            rect: VisionRect::<Global>::new(visible.x, visible.y, visible.w, visible.h),
            scale: monitor.scale as f32,
        };
        let params = detect_params(started, detect_budget);
        let mut shared = self.detector.try_lock().ok();
        let mut spare;
        let detector: &mut dyn Detector = match shared.as_deref_mut() {
            Some(detector) => detector.as_mut(),
            None => {
                spare = (self.spare)();
                spare.as_mut()
            }
        };
        let detections = detector.detect(&frame, &params);
        drop(shared);
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
/// grim, within RECAPTURE_LIMIT.
fn capture_region(check: &PixelCheck) -> Result<(u32, u32, Vec<u8>), String> {
    let scale = f64::from(check.scale_permille) / 1000.0;
    decode_png(&grim(check.area, scale, RECAPTURE_LIMIT)?)
}

/// A PNG of the logical `area` at `scale` from grim, which is killed once it
/// has run for `limit`.
fn grim(area: Rect, scale: f64, limit: Duration) -> Result<Vec<u8>, String> {
    let geometry = format!("{},{} {}x{}", area.x, area.y, area.w, area.h);
    let mut command = Command::new("grim");
    // Compression level 0 keeps the PNG stream lossless while avoiding
    // capture time that would consume the detector budget.
    command.args(["-l", "0", "-g", &geometry, "-s", &scale.to_string(), "-"]);
    run_bounded(command, limit)
}

/// `command`'s standard output, read on its own thread so the image cannot
/// fill the pipe; a command still running after `limit` is killed.
fn run_bounded(mut command: Command, limit: Duration) -> Result<Vec<u8>, String> {
    let until = Instant::now() + limit;
    let mut child = command
        .stdin(Stdio::null())
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
    let stop = |mut child: std::process::Child, why: &str| {
        let _ = child.kill();
        let _ = child.wait();
        Err(why.to_string())
    };
    let bytes = match rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(_)) => return stop(child, "grim output could not be read"),
        Err(_) => return stop(child, "grim did not capture in time"),
    };
    exit_status(child, bytes, until)
}

/// How long a capture whose output is already complete gets to exit, even
/// at or past its limit. grim exits within a millisecond of closing its
/// output, so a few milliseconds keep a capture that finished right at its
/// limit without letting a process that lingers run on.
const EXIT_GRACE: Duration = Duration::from_millis(10);

/// The complete output of `child`, once it exits successfully by `until`
/// or within EXIT_GRACE of now; a child still running after both is
/// killed.
fn exit_status(
    mut child: std::process::Child,
    bytes: Vec<u8>,
    until: Instant,
) -> Result<Vec<u8>, String> {
    let until = until.max(Instant::now() + EXIT_GRACE);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(bytes),
            Ok(Some(status)) => {
                return Err(format!(
                    "grim capture exited with {}",
                    status.code().unwrap_or(-1)
                ));
            }
            Ok(None) if Instant::now() < until => thread::sleep(Duration::from_millis(1)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("grim did not capture in time".into());
            }
        }
    }
}

fn detect_params(started: Instant, budget: Duration) -> DetectParams {
    DetectParams {
        // Keep file-row text legible on common 1920-pixel outputs while
        // bounding the detector's work and mapping boxes to source pixels.
        downscale_width: 1280,
        max_elements: DETECTOR_CAP,
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

/// How many raw detections the detector may return. High enough that a
/// screen of terminal words reaches the joining step whole, so the words at
/// the bottom of the screen are not cut before they are joined into lines;
/// the image targets themselves are bounded after joining by
/// IMAGE_TARGET_CAP.
const DETECTOR_CAP: usize = 4000;

/// At most this many image targets come from one collection (REQ-SRC-06).
/// Past it the lowest-confidence targets are dropped, the latest in reading
/// order first. REQ-LABEL-01: image targets join a session beneath its
/// reserved label prefix, which holds `labels::BENEATH_CAPACITY` labels of at
/// most three letters, so the cap may not exceed it; the build fails if it
/// does.
pub const IMAGE_TARGET_CAP: usize = 500;
const _: () = assert!(IMAGE_TARGET_CAP <= thodu_core::labels::BENEATH_CAPACITY);

/// Text detections closer than this, in logical units, are already joined
/// by the detector's own text merge.
const DETECTOR_TEXT_GAP: f64 = 4.0;

/// How many same-line gaps must agree before a window's word gap counts as
/// measured.
const MIN_GAP_SAMPLES: usize = 6;

/// A detection mapped onto the window that owns it, in logical units.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Mapped {
    window: usize,
    bounds: Rect,
    kind: DetectionKind,
    confidence: f32,
}

/// Convert monitor-image candidates and attach each one to the window that
/// owns its click point; one under a layer, such as the bar or a
/// notification, is dropped. A candidate whose click point shifts when
/// clipped to its window is dropped too, keeping its eventual click safe.
/// Text on one line is then joined into runs of words (join_text_runs), and
/// at most IMAGE_TARGET_CAP targets are kept.
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
    let owner_at = |point: (i32, i32)| {
        let owner = model::owner_among(visible_windows, layers, point.0, point.1);
        visible_windows
            .iter()
            .position(|window| Some(window.address.as_str()) == owner.window())
    };
    let mapped: Vec<Mapped> = detections
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
            let index = owner_at(center)?;
            let window = &visible_windows[index];
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
            Some(Mapped {
                window: index,
                bounds,
                kind: d.kind,
                confidence: d.confidence,
            })
        })
        .collect();
    let joined = join_text_runs(
        mapped,
        |window, run| {
            let address = &visible_windows[window].address;
            owner_at(run.center()) == Some(window)
                && !covered_by_accessible(run, address, accessible)
        },
        |window, gap| {
            let address = &visible_windows[window].address;
            owner_at(gap.center()) == Some(window)
                && !accessible.iter().any(|target| {
                    target.source != From::Image
                        && (target.window.is_empty() || &target.window == address)
                        && target.bounds.intersect(&gap).is_some()
                })
        },
    );
    cap_targets(joined, IMAGE_TARGET_CAP)
        .into_iter()
        .enumerate()
        .map(|(i, mapped)| {
            let window = visible_windows[mapped.window].address.clone();
            Target {
                id: format!("{window}:i{i}"),
                bounds: mapped.bounds,
                role: match mapped.kind {
                    DetectionKind::TextLine => Role::Text,
                    DetectionKind::Link => Role::Link,
                    DetectionKind::Box => Role::Unknown,
                },
                text: String::new(),
                source: From::Image,
                actions: vec![ActionKind::Click],
                window,
                check: None,
            }
        })
        .collect()
}

/// The `cap` most confident of `mapped`, ties going to the earliest in
/// reading order, kept in their order.
fn cap_targets(mapped: Vec<Mapped>, cap: usize) -> Vec<Mapped> {
    if mapped.len() <= cap {
        return mapped;
    }
    let mut order: Vec<usize> = (0..mapped.len()).collect();
    order.sort_by(|&a, &b| {
        let (a, b) = (&mapped[a], &mapped[b]);
        b.confidence
            .total_cmp(&a.confidence)
            .then((a.bounds.y, a.bounds.x).cmp(&(b.bounds.y, b.bounds.x)))
    });
    let mut keep = vec![false; mapped.len()];
    for &i in order.iter().take(cap) {
        keep[i] = true;
    }
    mapped
        .into_iter()
        .zip(keep)
        .filter_map(|(mapped, keep)| keep.then_some(mapped))
        .collect()
}

/// Whether two rectangles sit on one text line: each overlaps the other
/// vertically by at least half the shorter one's height.
fn same_line(a: &Rect, b: &Rect) -> bool {
    let overlap = a.bottom().min(b.bottom()) - a.y.max(b.y);
    overlap > 0 && overlap * 2 >= a.h.min(b.h)
}

/// The smallest rectangle holding both.
fn union(a: &Rect, b: &Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Rect::new(
        x,
        y,
        a.right().max(b.right()) - x,
        a.bottom().max(b.bottom()) - y,
    )
}

/// The widest gap, in logical units, at which a window's text detections
/// on one line are words of one run (REQ-SRC-06). It is measured from the
/// frame: the gaps between each text detection and its nearest neighbour to
/// the right on its line cluster at the window's word gap, one space of a
/// monospace terminal or of proportional text, so the most common gap, with
/// a unit of rounding either side, is that word gap. A gap of two or more
/// spaces, such as the columns of `ls`, is longer by at least one character
/// advance and stays a break. Gaps longer than one and a half median text
/// heights are never word gaps, which keeps separate controls apart. With
/// too few agreeing gaps the detector's own merge gap stands.
fn word_gap(texts: &[Rect]) -> f64 {
    if texts.len() < 2 {
        return DETECTOR_TEXT_GAP;
    }
    let mut heights: Vec<i32> = texts.iter().map(|r| r.h).collect();
    heights.sort_unstable();
    let ceiling = f64::from(heights[heights.len() / 2]) * 1.5;
    let mut counts = std::collections::BTreeMap::<i32, usize>::new();
    for a in texts {
        let nearest = texts
            .iter()
            .filter(|b| b.x >= a.right() && same_line(a, b))
            .map(|b| b.x - a.right())
            .min();
        if let Some(gap) = nearest
            && f64::from(gap) > DETECTOR_TEXT_GAP
            && f64::from(gap) <= ceiling
        {
            *counts.entry(gap).or_default() += 1;
        }
    }
    let near = |gap: i32| {
        (gap - 1..=gap + 1)
            .filter_map(|g| counts.get(&g))
            .sum::<usize>()
    };
    let Some((mode, agreeing)) = counts
        .keys()
        .map(|&gap| (gap, near(gap)))
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
    else {
        return DETECTOR_TEXT_GAP;
    };
    if agreeing < MIN_GAP_SAMPLES {
        return DETECTOR_TEXT_GAP;
    }
    (f64::from(mode + 1) * 1.3).min(ceiling)
}

/// Join each window's text detections on one line into runs of words
/// (REQ-SRC-06). Two pieces join when the gap between them is no wider than
/// the window's measured word gap (word_gap), `gap_free` accepts the strip
/// between them, no box or link detection holds one piece but not the
/// other or, holding neither, lies in that strip, and `run_ok` accepts the
/// joined run. Boxes and
/// links are kept as they are.
fn join_text_runs(
    mapped: Vec<Mapped>,
    run_ok: impl Fn(usize, Rect) -> bool,
    gap_free: impl Fn(usize, Rect) -> bool,
) -> Vec<Mapped> {
    let mut windows: Vec<usize> = mapped.iter().map(|m| m.window).collect();
    windows.sort_unstable();
    windows.dedup();
    let mut out: Vec<Mapped> = mapped
        .iter()
        .filter(|m| m.kind != DetectionKind::TextLine)
        .copied()
        .collect();
    for window in windows {
        let barriers: Vec<Rect> = mapped
            .iter()
            .filter(|m| m.window == window && m.kind != DetectionKind::TextLine)
            .map(|m| m.bounds)
            .collect();
        let mut texts: Vec<Mapped> = mapped
            .iter()
            .filter(|m| m.window == window && m.kind == DetectionKind::TextLine)
            .copied()
            .collect();
        let gap = word_gap(&texts.iter().map(|m| m.bounds).collect::<Vec<_>>());
        texts.sort_by_key(|m| (m.bounds.x, m.bounds.y));
        let mut runs: Vec<Mapped> = Vec::new();
        for text in texts {
            let joins = |run: &Mapped| {
                let (left, right) = (run.bounds, text.bounds);
                if !same_line(&left, &right) || f64::from(right.x - left.right()) > gap {
                    return None;
                }
                let joined = union(&left, &right);
                let strip_x = left.right().min(right.x);
                let strip = Rect::new(strip_x, joined.y, (right.x - left.right()).max(1), joined.h);
                let separated = barriers.iter().any(|barrier| {
                    let (holds_left, holds_right) =
                        (barrier.contains(&left), barrier.contains(&right));
                    holds_left != holds_right || !holds_left && barrier.intersect(&strip).is_some()
                });
                (!separated && gap_free(window, strip) && run_ok(window, joined))
                    .then_some((right.x - left.right(), joined))
            };
            let best = runs
                .iter()
                .enumerate()
                .filter_map(|(i, run)| joins(run).map(|(gap, joined)| (gap, i, joined)))
                .min_by_key(|&(gap, i, _)| (gap.abs(), i));
            match best {
                Some((_, i, joined)) => {
                    runs[i].bounds = joined;
                    runs[i].confidence = runs[i].confidence.max(text.confidence);
                }
                None => runs.push(text),
            }
        }
        out.extend(runs);
    }
    out.sort_by_key(|m| (m.bounds.y, m.bounds.x));
    out
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

    /// A blank monitor image, as grim would return it.
    fn png_of(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width, height);
            encoder.set_color(png::ColorType::Rgba);
            let mut writer = encoder.write_header().expect("a header");
            writer
                .write_image_data(&vec![255; (width * height * 4) as usize])
                .expect("the pixels");
        }
        bytes
    }

    fn line_at(x: i32, y: i32) -> woints_core::vision::Detection {
        woints_core::vision::Detection {
            rect_px: woints_core::geom::Rect::new(x, y, 60, 14),
            confidence: 0.9,
            class: woints_vision::DetectionClass::TextLine,
            source: woints_core::vision::DetectionSource::default(),
        }
    }

    /// A detector that runs right up to its deadline and returns what it
    /// found by then, as the classical detector does.
    struct ToTheDeadline;

    impl woints_vision::Detector for ToTheDeadline {
        fn detect(
            &mut self,
            _: &woints_vision::Frame,
            params: &woints_vision::DetectParams,
        ) -> Vec<woints_core::vision::Detection> {
            let deadline = params.deadline.expect("a detection deadline");
            std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
            vec![line_at(120, 70)]
        }
    }

    fn monitor() -> thodu_core::model::Monitor {
        thodu_core::model::Monitor {
            name: "M".into(),
            x: 0,
            y: 0,
            width: 400,
            height: 300,
            scale: 1.0,
            layers: Vec::new(),
        }
    }

    /// A capture of the test monitor, encoded once so the encoding is not
    /// timed against the source's budget.
    fn blank_capture() -> Box<super::Capture> {
        let area = monitor().logical();
        let png = png_of(area.w as u32, area.h as u32);
        Box::new(move |_: Rect, _, _| Ok(png.clone()))
    }

    // REQ-SRC-06: the detector's deadline leaves room to map and send its
    // detections, so one that runs to its deadline is used in part rather
    // than arriving after the image budget and being discarded.
    #[test]
    #[ignore = "timing: run alone"]
    fn req_src_06_a_detection_that_reaches_its_deadline_is_used_in_part() {
        let source = super::ImageSource::with(
            Duration::from_millis(180),
            blank_capture(),
            Box::new(|| Box::new(ToTheDeadline)),
        );
        let sources: Vec<std::sync::Arc<dyn crate::sources::Source>> =
            vec![std::sync::Arc::new(source)];
        let got = crate::sources::collect_fallback_visible(&sources, &[window()], &monitor(), &[]);
        assert!(
            !got.status.iter().any(|status| status.contains("missed")),
            "{:?}",
            got.status
        );
        assert_eq!(got.targets.len(), 1, "{:?}", got.status);
        assert_eq!(got.targets[0].source, Source::Image);
    }

    // REQ-SRC-06: a capture that hangs is killed at its limit, and one that
    // fails is named; a complete one is read whole.
    #[test]
    #[ignore = "timing: run alone"]
    fn req_src_06_a_hung_grim_is_killed_at_its_limit() {
        let marker = format!("7.{}", std::process::id());
        let mut hung = std::process::Command::new("sleep");
        hung.arg(&marker);
        let started = Instant::now();
        let got = super::run_bounded(hung, Duration::from_millis(50));
        assert_eq!(got, Err("grim did not capture in time".to_string()));
        assert!(started.elapsed() < Duration::from_millis(1000));
        let still_running = std::fs::read_dir("/proc")
            .expect("/proc")
            .filter_map(|entry| std::fs::read(entry.ok()?.path().join("cmdline")).ok())
            .any(|cmdline| cmdline == format!("sleep\0{marker}\0").into_bytes());
        assert!(!still_running, "the hung capture was killed");

        let mut failed = std::process::Command::new("sh");
        failed.args(["-c", "exit 3"]);
        assert_eq!(
            super::run_bounded(failed, Duration::from_secs(5)),
            Err("grim capture exited with 3".to_string())
        );
        let mut whole = std::process::Command::new("printf");
        whole.arg("png");
        assert_eq!(
            super::run_bounded(whole, Duration::from_secs(5)),
            Ok(b"png".to_vec())
        );
    }

    /// A detector whose first detection overruns its deadline by far, as a
    /// worker left behind by a late session does.
    struct OverrunsOnce(std::sync::Arc<std::sync::atomic::AtomicUsize>);

    impl woints_vision::Detector for OverrunsOnce {
        fn detect(
            &mut self,
            _: &woints_vision::Frame,
            _: &woints_vision::DetectParams,
        ) -> Vec<woints_core::vision::Detection> {
            if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                std::thread::sleep(Duration::from_millis(1500));
            }
            vec![line_at(120, 70)]
        }
    }

    // REQ-SRC-06: a late image worker still holding the detector does not
    // block the next session's detection.
    #[test]
    #[ignore = "timing: run alone"]
    fn req_src_06_a_late_image_worker_does_not_block_the_next_detection() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let spare_calls = std::sync::Arc::clone(&calls);
        let source = std::sync::Arc::new(super::ImageSource::with(
            Duration::from_millis(180),
            blank_capture(),
            Box::new(move || Box::new(OverrunsOnce(std::sync::Arc::clone(&spare_calls)))),
        ));
        let late = {
            let source = std::sync::Arc::clone(&source);
            std::thread::spawn(move || {
                crate::sources::Source::collect_many(&*source, &[window()], &monitor())
            })
        };
        while calls.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            std::thread::yield_now();
        }
        let started = Instant::now();
        let next = crate::sources::Source::collect_many(&*source, &[window()], &monitor())
            .expect("the next detection");
        let took = started.elapsed();
        assert!(took < Duration::from_millis(1000), "waited {took:?}");
        assert_eq!(next.targets.len(), 1, "{}", next.status);
        late.join().expect("the late worker").expect("its result");
    }

    // REQ-SRC-06: a capture whose output is complete at its limit is kept
    // if it exits within the grace, while one that lingers is still killed.
    #[test]
    #[ignore = "timing: run alone"]
    fn req_src_06_a_capture_complete_at_its_limit_is_kept() {
        let exited = std::process::Command::new("true")
            .spawn()
            .expect("true starts");
        assert_eq!(
            super::exit_status(exited, b"png".to_vec(), Instant::now()),
            Ok(b"png".to_vec())
        );
        let lingering = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .expect("sleep starts");
        let started = Instant::now();
        assert_eq!(
            super::exit_status(lingering, b"png".to_vec(), Instant::now()),
            Err("grim did not capture in time".to_string())
        );
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    /// A synthetic monitor frame in physical pixels, with what was drawn.
    struct Synthetic {
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        scale: f64,
        /// Each run of words drawn, in physical pixels.
        runs: Vec<Rect>,
        /// Each control drawn, in physical pixels.
        controls: Vec<Rect>,
    }

    impl Synthetic {
        fn new(width: u32, height: u32, scale: f64, background: [u8; 3]) -> Self {
            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            for _ in 0..width * height {
                rgba.extend_from_slice(&[background[0], background[1], background[2], 255]);
            }
            Self {
                rgba,
                width,
                height,
                scale,
                runs: Vec::new(),
                controls: Vec::new(),
            }
        }

        fn fill(&mut self, r: Rect, rgb: [u8; 3]) {
            for y in r.y.max(0)..r.bottom().min(self.height as i32) {
                for x in r.x.max(0)..r.right().min(self.width as i32) {
                    let i = ((y as u32 * self.width + x as u32) * 4) as usize;
                    self.rgba[i..i + 3].copy_from_slice(&rgb);
                }
            }
        }

        /// A rectangle's one-pixel outline.
        fn outline(&mut self, r: Rect, rgb: [u8; 3]) {
            self.fill(Rect::new(r.x, r.y, r.w, 1), rgb);
            self.fill(Rect::new(r.x, r.bottom() - 1, r.w, 1), rgb);
            self.fill(Rect::new(r.x, r.y, 1, r.h), rgb);
            self.fill(Rect::new(r.right() - 1, r.y, 1, r.h), rgb);
        }

        /// One glyph as a block of ink in its cell at `x`, its height set
        /// by the letter: an ascender, a descender or the x-height.
        fn glyph(&mut self, letter: u8, x: i32, top: i32, ink: i32, rgb: [u8; 3]) -> Rect {
            let (from, to) = match letter {
                b'b' | b'd' | b'f' | b'h' | b'k' | b'l' | b't' => (0, 13),
                b'g' | b'j' | b'p' | b'q' | b'y' => (3, 16),
                _ => (3, 13),
            };
            let rect = Rect::new(x, top + from, ink, to - from);
            self.fill(rect, rgb);
            rect
        }

        fn ink(&self, background: [u8; 3]) -> Vec<(i32, i32)> {
            let mut ink = Vec::new();
            for y in 0..self.height as i32 {
                for x in 0..self.width as i32 {
                    let i = ((y as u32 * self.width + x as u32) * 4) as usize;
                    if self.rgba[i..i + 3] != background {
                        ink.push((x, y));
                    }
                }
            }
            ink
        }

        /// A physical rectangle in logical units, as the mapper rounds it.
        fn logical(&self, r: Rect) -> Rect {
            let l = |v: i32| (f64::from(v) / self.scale).round() as i32;
            Rect::new(
                l(r.x),
                l(r.y),
                l(r.right()) - l(r.x),
                l(r.bottom()) - l(r.y),
            )
        }
    }

    const TERMINAL_BACKGROUND: [u8; 3] = [0x1e, 0x1e, 0x2e];
    const TERMINAL_TEXT: [u8; 3] = [0xcd, 0xd6, 0xf4];

    /// Two Foot-like terminals side by side on a 1920 by 1200 monitor at
    /// scale 1.25: monospace cells `advance` pixels wide with `ink` pixels
    /// of glyph each, so one space leaves `advance + advance - ink` pixels
    /// between words. Words run one space apart; one gap in eight is two to
    /// four spaces, as between the columns of `ls`, and starts a new run.
    /// The text is drawn from a fixed seed, not from any real screen.
    fn terminal_frame(advance: i32, ink: i32) -> (Synthetic, Vec<Window>) {
        let mut frame = Synthetic::new(1920, 1200, 1.25, TERMINAL_BACKGROUND);
        let mut seed: u64 = 0x7468_6f64_7530_0038;
        let mut next = |n: u64| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) % n
        };
        let letters = b"abcdefghijklmnopqrstuvwxyz";
        let line_height = 22;
        for column in 0..2 {
            let (left, right) = (column * 960 + 10, column * 960 + 950);
            let columns = (right - left) / advance;
            for line in 0..53 {
                let top = 8 + line * line_height;
                let length = 12 + next(columns as u64 - 12) as i32;
                let mut at = 0;
                let mut run: Option<Rect> = None;
                while at + 2 <= length {
                    let word = 2 + next(7) as i32;
                    if at + word > length {
                        break;
                    }
                    for c in 0..word {
                        let letter = letters[next(26) as usize];
                        let glyph =
                            frame.glyph(letter, left + (at + c) * advance, top, ink, TERMINAL_TEXT);
                        run = Some(run.map_or(glyph, |r| super::union(&r, &glyph)));
                    }
                    at += word;
                    let spaces = if next(8) == 0 { 2 + next(3) as i32 } else { 1 };
                    if spaces > 1
                        && let Some(done) = run.take()
                    {
                        frame.runs.push(done);
                    }
                    at += spaces;
                }
                frame.runs.extend(run);
            }
        }
        let terminal = |address: &str, x: i32| Window {
            address: address.into(),
            class: "foot".into(),
            at: (x, 0),
            size: (768, 960),
            workspace: 1,
            ..Window::default()
        };
        (frame, vec![terminal("0xleft", 0), terminal("0xright", 768)])
    }

    fn detect_all(frame: &Synthetic) -> Vec<DetectionBox> {
        let vision = woints_vision::Frame {
            rgba: frame.rgba.clone(),
            width: frame.width,
            height: frame.height,
            rect: woints_core::geom::Rect::new(0, 0, 1536, 960),
            scale: frame.scale as f32,
        };
        let mut detector = woints_vision::ClassicalDetector::new();
        let mut params = super::detect_params(Instant::now(), Duration::from_secs(3600));
        params.deadline = None;
        woints_vision::Detector::detect(&mut detector, &vision, &params)
            .iter()
            .filter_map(|d| {
                let kind = match d.class {
                    woints_vision::DetectionClass::TextLine => DetectionKind::TextLine,
                    woints_vision::DetectionClass::Link => DetectionKind::Link,
                    woints_vision::DetectionClass::Box => DetectionKind::Box,
                    _ => return None,
                };
                Some(DetectionBox {
                    bounds: Rect::new(d.rect_px.x, d.rect_px.y, d.rect_px.w, d.rect_px.h),
                    kind,
                    confidence: d.confidence,
                })
            })
            .collect()
    }

    fn near(a: &Rect, b: &Rect, slack: i32) -> bool {
        (a.x - b.x).abs() <= slack
            && (a.y - b.y).abs() <= slack
            && (a.right() - b.right()).abs() <= slack
            && (a.bottom() - b.bottom()).abs() <= slack
    }

    // REQ-SRC-06: on a synthetic monospace terminal at Foot's spacing, with
    // one space leaving 8.0 or 7.2 logical units between words, the words of
    // a run join into one image target: one target per run, not per word,
    // every run reaches the mapper, and the count stays under the cap.
    #[test]
    fn req_src_06_terminal_text_is_one_target_per_word_run() {
        for (advance, ink) in [(8, 6), (8, 7)] {
            let (frame, windows) = terminal_frame(advance, ink);
            let detections = detect_all(&frame);
            let words = detections
                .iter()
                .filter(|d| d.kind == DetectionKind::TextLine)
                .count();
            assert!(
                words > 600,
                "the frame has the screen's density of words: {words}"
            );
            let targets = targets_from_monitor_detections(
                &windows,
                &windows,
                &[],
                Rect::new(0, 0, 1536, 960),
                frame.scale,
                &detections,
                &[],
            );
            assert!(
                targets.len() < super::IMAGE_TARGET_CAP && targets.len() * 3 < words,
                "{} targets from {words} words",
                targets.len()
            );
            assert_eq!(
                targets.len(),
                frame.runs.len(),
                "one target per run for {words} words at gap {}",
                2 * advance - ink
            );
            for run in &frame.runs {
                let want = frame.logical(*run);
                assert!(
                    targets
                        .iter()
                        .any(|t| t.role == Role::Text && near(&t.bounds, &want, 3)),
                    "run {want:?} at gap {} is one target",
                    2 * advance - ink
                );
            }
        }
    }

    /// The share of `ink` pixels, physical, under any of `chips`, logical.
    fn ink_share(frame: &Synthetic, ink: &[(i32, i32)], chips: &[Rect]) -> f64 {
        let covered = ink
            .iter()
            .filter(|&&(x, y)| {
                let (lx, ly) = (f64::from(x) / frame.scale, f64::from(y) / frame.scale);
                chips.iter().any(|c| {
                    lx >= f64::from(c.x)
                        && lx < f64::from(c.right())
                        && ly >= f64::from(c.y)
                        && ly < f64::from(c.bottom())
                })
            })
            .count();
        covered as f64 / ink.len() as f64
    }

    /// The most of a terminal's text ink that image labels may lie over,
    /// translucent or not.
    const MAX_INK_UNDER_LABELS: f64 = 0.08;
    /// The most of it that opaque labels, the ones beside the text, may hide.
    const MAX_INK_UNDER_OPAQUE_LABELS: f64 = 0.005;

    // REQ-LABEL-02 and REQ-SRC-06: the labels placed for the terminal's
    // text runs lie over under MAX_INK_UNDER_LABELS of the text's ink,
    // measured over every ink pixel, and the opaque ones, beside the text,
    // hide under MAX_INK_UNDER_OPAQUE_LABELS; a label over its own text is
    // one the overlay draws translucent. Labels on per-word targets, opaque
    // on each word's corner, covered far more.
    #[test]
    fn req_label_02_terminal_labels_leave_the_text_readable() {
        let (frame, windows) = terminal_frame(8, 6);
        let ink = frame.ink(TERMINAL_BACKGROUND);
        let monitor = Rect::new(0, 0, 1536, 960);
        let detections = detect_all(&frame);
        let targets = targets_from_monitor_detections(
            &windows,
            &windows,
            &[],
            monitor,
            frame.scale,
            &detections,
            &[],
        );
        let labels = thodu_core::labels::assign_beneath(targets.len(), 'z');
        let chips = thodu_core::labels::place_all(&targets, &labels, &monitor, |target| {
            windows
                .iter()
                .find(|w| w.address == target.window)
                .map(Window::rect)
        });
        let share = ink_share(&frame, &ink, &chips);
        let opaque: Vec<Rect> = chips
            .iter()
            .zip(&targets)
            .filter(|(chip, target)| chip.intersect(&target.bounds).is_none())
            .map(|(chip, _)| *chip)
            .collect();
        let hidden = ink_share(&frame, &ink, &opaque);
        eprintln!(
            "ink under labels {share:.4}, under opaque labels {hidden:.4}, {} of {} labels beside their text",
            opaque.len(),
            chips.len()
        );
        assert!(
            share < MAX_INK_UNDER_LABELS,
            "labels lie over {share:.3} of the ink"
        );
        assert!(
            hidden < MAX_INK_UNDER_OPAQUE_LABELS,
            "opaque labels hide {hidden:.3} of the ink"
        );
        for (chip, target) in chips.iter().zip(&targets) {
            let window = windows
                .iter()
                .find(|w| w.address == target.window)
                .expect("its window");
            assert!(monitor.contains(chip));
            if chip.intersect(&target.bounds).is_none() {
                assert!(window.rect().contains(chip), "{chip:?} stays in its window");
                assert!(
                    chip.x - target.bounds.right() <= thodu_core::labels::TEXT_LABEL_GAP,
                    "{chip:?} sits right after {:?}",
                    target.bounds
                );
            }
            for other in &targets {
                if other.id != target.id {
                    assert!(
                        chip.intersect(&other.bounds).is_none()
                            || chip.intersect(&target.bounds).is_some(),
                        "{chip:?} beside its text covers no other text"
                    );
                }
            }
        }

        let words: Vec<Rect> = detections
            .iter()
            .filter(|d| d.kind == DetectionKind::TextLine)
            .take(500)
            .map(|d| frame.logical(d.bounds))
            .collect();
        let per_word: Vec<Rect> = words
            .iter()
            .map(|word| {
                thodu_core::labels::place(word, thodu_core::protocol::label_size(3), &monitor)
            })
            .collect();
        let before = ink_share(&frame, &ink, &per_word);
        eprintln!("ink under per-word labels {before:.4}");
        assert!(before > 4.0 * share, "per-word labels covered {before:.3}");
    }

    // REQ-SRC-06: on a Chromium-like page, joining leaves every control its
    // own target: a toolbar of outlined buttons a word gap apart, a row of
    // text items far apart, and paragraph text, which joins into lines.
    #[test]
    fn req_src_06_page_controls_keep_their_targets_when_text_joins() {
        let white = [255, 255, 255];
        let mut frame = Synthetic::new(1600, 600, 1.0, white);
        let grey = [0x5f, 0x63, 0x68];
        let dark = [0x20, 0x21, 0x24];
        // Buttons 80 by 28, 8 apart, each with a two-word label.
        for i in 0..6 {
            let button = Rect::new(40 + i * 88, 40, 80, 28);
            frame.outline(button, grey);
            frame.fill(Rect::new(button.x + 10, 50, 26, 9), dark);
            frame.fill(Rect::new(button.x + 42, 50, 26, 9), dark);
            frame.controls.push(button);
        }
        // Menu items 30 wide, 22 apart.
        let mut items = Vec::new();
        for i in 0..8 {
            let item = Rect::new(40 + i * 52, 120, 30, 10);
            frame.fill(item, dark);
            items.push(item);
        }
        // Paragraph lines of proportional words 10 apart, too far apart for
        // the detector's own merge.
        let mut x_lines = Vec::new();
        for line in 0..6 {
            let y = 200 + line * 20;
            let mut x = 40;
            let mut first = None;
            for word in 0..14 {
                let w = 18 + (word * 7 + line * 3) % 23;
                frame.fill(Rect::new(x, y, w, 10), dark);
                first.get_or_insert(x);
                x += w + 10;
            }
            x_lines.push(Rect::new(first.unwrap_or(40), y, x - 10 - 40, 10));
        }
        let page = Window {
            address: "0xpage".into(),
            class: "chromium".into(),
            at: (0, 0),
            size: (1600, 600),
            workspace: 1,
            ..Window::default()
        };
        let detections: Vec<DetectionBox> = {
            let vision = woints_vision::Frame {
                rgba: frame.rgba.clone(),
                width: frame.width,
                height: frame.height,
                rect: woints_core::geom::Rect::new(0, 0, 1600, 600),
                scale: 1.0,
            };
            let mut detector = woints_vision::ClassicalDetector::new();
            let mut params = super::detect_params(Instant::now(), Duration::from_secs(3600));
            params.deadline = None;
            woints_vision::Detector::detect(&mut detector, &vision, &params)
                .iter()
                .filter_map(|d| {
                    let kind = match d.class {
                        woints_vision::DetectionClass::TextLine => DetectionKind::TextLine,
                        woints_vision::DetectionClass::Link => DetectionKind::Link,
                        woints_vision::DetectionClass::Box => DetectionKind::Box,
                        _ => return None,
                    };
                    Some(DetectionBox {
                        bounds: Rect::new(d.rect_px.x, d.rect_px.y, d.rect_px.w, d.rect_px.h),
                        kind,
                        confidence: d.confidence,
                    })
                })
                .collect()
        };
        let windows = [page];
        let targets = targets_from_monitor_detections(
            &windows,
            &windows,
            &[],
            Rect::new(0, 0, 1600, 600),
            1.0,
            &detections,
            &[],
        );
        for button in &frame.controls {
            assert!(
                targets.iter().any(|t| near(&t.bounds, button, 4)),
                "button {button:?} keeps its target: {targets:?}"
            );
            let label = Rect::new(button.x + 10, 50, 58, 9);
            assert!(
                targets
                    .iter()
                    .any(|t| t.role == Role::Text && near(&t.bounds, &label, 2)),
                "the button's two words are one target"
            );
        }
        for target in &targets {
            let holds = frame
                .controls
                .iter()
                .filter(|b| b.intersect(&target.bounds).is_some())
                .count();
            assert!(holds <= 1, "{target:?} spans buttons");
        }
        for item in &items {
            assert!(
                targets.iter().any(|t| near(&t.bounds, item, 2)),
                "menu item {item:?} keeps its own target"
            );
        }
        for line in &x_lines {
            assert!(
                targets.iter().any(|t| near(&t.bounds, line, 2)),
                "paragraph line {line:?} is one target"
            );
        }
    }

    // REQ-SRC-06: text a word gap apart does not join across controls: not
    // out of one box into the next, and not over a small box between two
    // words, while words of the same line elsewhere still join.
    #[test]
    fn req_src_06_text_runs_do_not_join_across_boxes() {
        let mut app = window();
        app.at = (0, 0);
        app.size = (800, 200);
        let text = |x, y, w| DetectionBox {
            bounds: Rect::new(x, y, w, 10),
            kind: DetectionKind::TextLine,
            confidence: 0.4,
        };
        let boxed = |x, y, w, h| DetectionBox {
            bounds: Rect::new(x, y, w, h),
            kind: DetectionKind::Box,
            confidence: 0.5,
        };
        // A line of words 8 apart sets the window's word gap.
        let mut detections: Vec<DetectionBox> =
            (0..10).map(|i| text(10 + i * 38, 10, 30)).collect();
        detections.extend([
            boxed(100, 36, 60, 20),
            boxed(162, 36, 60, 20),
            text(104, 40, 52),
            text(164, 40, 50),
            text(270, 40, 25),
            boxed(298, 40, 4, 10),
            text(305, 40, 25),
            text(400, 70, 40),
            text(448, 70, 25),
        ]);
        let accessible = Target {
            id: "a0".into(),
            bounds: Rect::new(441, 68, 5, 14),
            role: Role::Button,
            text: "accessible".into(),
            source: Source::Atspi,
            actions: vec![ActionKind::Click],
            window: app.address.clone(),
            check: None,
        };
        let targets = targets_from_monitor_detections(
            std::slice::from_ref(&app),
            std::slice::from_ref(&app),
            &[],
            Rect::new(0, 0, 800, 200),
            1.0,
            &detections,
            &[accessible],
        );
        let has = |r: Rect| targets.iter().any(|t| t.bounds == r);
        assert!(
            has(Rect::new(10, 10, 372, 10)),
            "the line joins: {targets:?}"
        );
        assert!(has(Rect::new(104, 40, 52, 10)) && has(Rect::new(164, 40, 50, 10)));
        assert!(has(Rect::new(270, 40, 25, 10)) && has(Rect::new(305, 40, 25, 10)));
        assert!(has(Rect::new(100, 36, 60, 20)) && has(Rect::new(298, 40, 4, 10)));
        assert!(
            has(Rect::new(400, 70, 40, 10)) && has(Rect::new(448, 70, 25, 10)),
            "an accessible control between two words keeps them apart"
        );
    }

    // REQ-SRC-06: the image source is bounded in count as well as time: past
    // IMAGE_TARGET_CAP the least confident targets go, the latest first.
    #[test]
    fn req_src_06_image_targets_are_capped_in_count() {
        let mut app = window();
        app.at = (0, 0);
        app.size = (2000, 2000);
        let mut detections: Vec<DetectionBox> = (0..super::IMAGE_TARGET_CAP as i32 + 50)
            .map(|i| DetectionBox {
                bounds: Rect::new((i % 40) * 50, (i / 40) * 40, 20, 10),
                kind: DetectionKind::TextLine,
                confidence: 0.4,
            })
            .collect();
        detections.push(DetectionBox {
            bounds: Rect::new(10, 1900, 30, 30),
            kind: DetectionKind::Box,
            confidence: 0.7,
        });
        let targets = targets_from_monitor_detections(
            std::slice::from_ref(&app),
            std::slice::from_ref(&app),
            &[],
            Rect::new(0, 0, 2000, 2000),
            1.0,
            &detections,
            &[],
        );
        assert_eq!(targets.len(), super::IMAGE_TARGET_CAP);
        assert!(
            targets
                .iter()
                .any(|t| t.bounds == Rect::new(10, 1900, 30, 30))
        );
        assert!(targets.iter().any(|t| t.bounds == Rect::new(0, 0, 20, 10)));
        let last = Rect::new(450, 520, 20, 10);
        assert!(detections.iter().any(|d| d.bounds == last));
        assert!(!targets.iter().any(|t| t.bounds == last), "the latest goes");
    }

    // REQ-LABEL-01: the image target cap and the label limit are checked
    // together; a full collection's worth of image targets gets labels of at
    // most three letters beneath the reserved prefix.
    #[test]
    fn req_label_01_image_cap_fits_beneath_the_reserved_label_prefix() {
        let labels = thodu_core::labels::assign_beneath(super::IMAGE_TARGET_CAP, 'z');
        assert_eq!(labels.len(), super::IMAGE_TARGET_CAP);
        assert!(labels.iter().all(|label| label.len() <= 3), "{labels:?}");
    }
}
