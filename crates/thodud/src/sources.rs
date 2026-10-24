//! The sources of a session's targets, each held to its budget (REQ-SRC-05):
//! a source that has not answered in time is left out of the session and
//! named in the status line, so one slow source never holds up the labels.

use std::collections::HashSet;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use thodu_core::model::{ActionKind, Monitor, Rect, Role, Source as From, Target, Window};
use thodu_core::protocol::ShellItem;
use thodu_core::{tree, unregistered};

use crate::atspi;

/// A source's targets, status, and whether a later fallback can fill a gap.
pub struct SourceTargets {
    pub targets: Vec<Target>,
    pub status: String,
    pub request_fallback: bool,
}

impl SourceTargets {
    pub fn new(targets: Vec<Target>, status: impl Into<String>) -> Self {
        Self {
            targets,
            status: status.into(),
            request_fallback: false,
        }
    }

    fn with_fallback(mut self) -> Self {
        self.request_fallback = true;
        self
    }
}

/// A source's result, or why it failed.
pub type Found = Result<SourceTargets, String>;

pub trait Source: Send + Sync {
    fn name(&self) -> &'static str;
    fn budget(&self) -> Duration;
    /// Sources marked as fallback run only when a primary source asks for one.
    fn is_fallback(&self) -> bool {
        false
    }
    /// Whether a primary-source error or timeout should trigger a fallback.
    fn fallback_on_error(&self) -> bool {
        false
    }
    /// The targets, and what to say about them in the status line.
    fn collect(&self, window: &Window, monitor: &Monitor) -> Found;
    /// Collect a session spanning multiple windows. Existing single-window
    /// sources keep their original implementation through this adapter.
    fn collect_many(&self, windows: &[Window], monitor: &Monitor) -> Found {
        match windows.first() {
            Some(window) => self.collect(window, monitor),
            None => Ok(SourceTargets::new(Vec::new(), "0 targets")),
        }
    }
}

pub struct Collected {
    pub targets: Vec<Target>,
    pub status: Vec<String>,
    pub fallback_requested: bool,
}

/// Run primary sources together, then only run fallbacks requested by them.
#[cfg(test)]
pub fn collect(sources: &[Arc<dyn Source>], window: &Window, monitor: &Monitor) -> Collected {
    collect_visible(sources, std::slice::from_ref(window), monitor)
}

/// Collect targets across every window in the session's visible scope.
pub fn collect_visible(
    sources: &[Arc<dyn Source>],
    windows: &[Window],
    monitor: &Monitor,
) -> Collected {
    let mut got = collect_primary_visible(sources, windows, monitor);
    if got.fallback_requested {
        let fallback = collect_fallback_visible(sources, windows, monitor);
        got.targets.extend(fallback.targets);
        got.status.extend(fallback.status);
    }
    finish_visible_targets(&mut got.targets, windows);
    got.fallback_requested = false;
    got
}

/// Collect primary sources only, so an overlay can draw them while a slower
/// image fallback runs in the background.
pub fn collect_primary_visible(
    sources: &[Arc<dyn Source>],
    windows: &[Window],
    monitor: &Monitor,
) -> Collected {
    let (primary, fallback): (Vec<_>, Vec<_>) =
        sources.iter().partition(|source| !source.is_fallback());
    let got = collect_phase(&primary, windows, monitor);
    let fallback_requested = got.request_fallback && !fallback.is_empty();
    let mut targets = got.targets;
    finish_visible_targets(&mut targets, windows);
    Collected {
        targets,
        status: got.status,
        fallback_requested,
    }
}

/// Run only fallback sources. The caller can merge these targets into a
/// session after its first draw, or combine them synchronously for a query.
pub fn collect_fallback_visible(
    sources: &[Arc<dyn Source>],
    windows: &[Window],
    monitor: &Monitor,
) -> Collected {
    let (_, fallback): (Vec<_>, Vec<_>) = sources.iter().partition(|source| !source.is_fallback());
    let got = collect_phase(&fallback, windows, monitor);
    let mut targets = got.targets;
    finish_visible_targets(&mut targets, windows);
    Collected {
        targets,
        status: got.status,
        fallback_requested: false,
    }
}

/// Apply the ownership and deduplication rules after source results merge.
pub fn finish_visible_targets(targets: &mut Vec<Target>, windows: &[Window]) {
    keep_targets_on_topmost_windows(targets, windows);
    remove_covered_image_targets(targets);
}

struct Phase {
    targets: Vec<Target>,
    status: Vec<String>,
    request_fallback: bool,
}

fn collect_phase(sources: &[&Arc<dyn Source>], windows: &[Window], monitor: &Monitor) -> Phase {
    let started = Instant::now();
    let (tx, rx) = mpsc::channel();
    for (i, source) in sources.iter().enumerate() {
        let source = Arc::clone(source);
        let tx = tx.clone();
        let (windows, monitor) = (windows.to_vec(), monitor.clone());
        thread::spawn(move || {
            let _ = tx.send((i, source.collect_many(&windows, &monitor)));
        });
    }
    drop(tx);
    let mut results: Vec<Option<Found>> = sources.iter().map(|_| None).collect();
    loop {
        let pending: Vec<usize> = (0..sources.len())
            .filter(|&i| results[i].is_none())
            .collect();
        let Some(deadline) = pending
            .iter()
            .map(|&i| started + sources[i].budget())
            .filter(|d| *d > Instant::now())
            .min()
        else {
            break;
        };
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok((i, result)) if started.elapsed() <= sources[i].budget() => {
                results[i] = Some(result)
            }
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let mut targets = Vec::new();
    let mut status = Vec::new();
    let mut request_fallback = false;
    for (source, result) in sources.iter().zip(results) {
        match result {
            Some(Ok(found)) => {
                targets.extend(found.targets);
                status.push(found.status);
                request_fallback |= found.request_fallback;
            }
            Some(Err(e)) => {
                status.push(format!("{}: {e}", source.name()));
                request_fallback |= source.fallback_on_error();
            }
            None => {
                status.push(format!(
                    "{} missed its {} ms budget",
                    source.name(),
                    source.budget().as_millis()
                ));
                request_fallback |= source.fallback_on_error();
            }
        }
    }
    Phase {
        targets,
        status,
        request_fallback,
    }
}

fn remove_covered_image_targets(targets: &mut Vec<Target>) {
    let accessible: Vec<_> = targets
        .iter()
        .filter(|target| matches!(target.source, From::Atspi | From::Shell))
        .cloned()
        .collect();
    targets.retain(|target| {
        target.source != From::Image
            || !crate::image::covered_by_accessible(target.bounds, &target.window, &accessible)
    });
}

/// A window target is actionable only when its click point belongs to the
/// frontmost collected window. Shell targets use their own layer and remain.
fn keep_targets_on_topmost_windows(targets: &mut Vec<Target>, windows: &[Window]) {
    targets.retain(|target| {
        if target.window.is_empty() {
            return true;
        }
        windows
            .iter()
            .filter(|window| {
                window
                    .rect()
                    .contains_point(target.bounds.center().0, target.bounds.center().1)
            })
            .min_by_key(|window| window.stacking_key())
            .is_some_and(|window| window.address == target.window)
    });
}

fn unregistered_status(window: &Window) -> String {
    let proc = std::path::PathBuf::from(format!("/proc/{}", window.pid));
    let exe_result = std::fs::read_link(proc.join("exe"));
    let exe = exe_result
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let maps_result = std::fs::read_to_string(proc.join("maps"));
    let libs: Vec<String> = maps_result
        .as_deref()
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_whitespace().nth(5))
        .filter(|p| p.contains(".so"))
        .map(String::from)
        .collect();
    if (exe_result.is_err() || maps_result.is_err())
        && unregistered::toolkit(&exe, &libs).is_empty()
    {
        unregistered::inspection_failed(&window.class)
    } else {
        unregistered::message(&window.class, &exe, &libs)
    }
}

/// The accessibility tree of the window's process.
pub struct Atspi {
    pub bus: Mutex<Option<atspi::Bus>>,
    pub woken: Mutex<HashSet<u32>>,
    pub budget: Duration,
}

impl Source for Atspi {
    fn name(&self) -> &'static str {
        "atspi"
    }

    fn budget(&self) -> Duration {
        self.budget
    }

    fn fallback_on_error(&self) -> bool {
        true
    }

    fn collect(&self, window: &Window, monitor: &Monitor) -> Found {
        self.collect_windows(std::slice::from_ref(window), monitor)
    }

    fn collect_many(&self, windows: &[Window], monitor: &Monitor) -> Found {
        self.collect_windows(windows, monitor)
    }
}

impl Atspi {
    fn collect_windows(&self, windows: &[Window], monitor: &Monitor) -> Found {
        let started = Instant::now();
        if windows.is_empty() {
            return Ok(SourceTargets::new(Vec::new(), "0 targets from atspi"));
        }
        let mut bus = self.bus.lock().map_err(|_| "bus lock".to_string())?;
        if bus.is_none() {
            *bus = Some(atspi::Bus::connect()?);
        }
        let reader = bus.as_ref().ok_or("no bus")?;
        let mut woken = self.woken.lock().map_err(|_| "wake lock".to_string())?;
        // The walk stops short of the budget, so its partial tree still
        // arrives in time.
        let mut walk = atspi::Walk::new(self.budget.saturating_sub(self.budget / 8));
        let snapshots = atspi::snapshot_many(reader, windows, monitor, &mut walk, &mut woken);
        let took = started.elapsed().as_millis();
        let mut targets = Vec::new();
        let mut fallback = false;
        let mut unavailable = Vec::new();
        let frame_indices = tree::match_frame_indices(&snapshots);
        for ((window, snapshot), frame_index) in
            windows.iter().zip(snapshots.iter()).zip(frame_indices)
        {
            if !snapshot.apps.iter().any(|app| app.pid == window.pid) {
                unavailable.push(unregistered_status(window));
                fallback = true;
                continue;
            }
            let frame = frame_index.map(|(app, frame)| &snapshot.apps[app].frames[frame]);
            let found = frame
                .map(|frame| tree::targets(frame, window, monitor))
                .unwrap_or_default();
            fallback |= found.is_empty() || frame.is_none_or(tree::needs_image_fallback);
            targets.extend(found.into_iter().map(|mut target| {
                target.id = format!("{}:{}", target.window, target.id);
                target
            }));
        }
        let cut = if walk.truncated {
            ", cut short at the budget"
        } else {
            ""
        };
        let said = if unavailable.is_empty() {
            format!(
                "{} targets from atspi across {} windows in {took} ms{cut}",
                targets.len(),
                windows.len()
            )
        } else {
            format!(
                "{} targets from atspi across {} windows in {took} ms{cut}; {}",
                targets.len(),
                windows.len(),
                unavailable
                    .into_iter()
                    .take(2)
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        };
        let result = SourceTargets::new(targets, said);
        Ok(if fallback {
            result.with_fallback()
        } else {
            result
        })
    }
}

/// The Omarchy bar's clickable items, as the overlay plugin's bar widget
/// last reported them for each monitor (REQ-SHELL-01).
pub struct Shell {
    pub items: Mutex<std::collections::HashMap<String, Vec<ShellItem>>>,
    pub budget: Duration,
}

/// The bar's targets on `monitor`, its items placed at the bar's origin.
pub fn shell_targets(items: &[ShellItem], origin: (i32, i32), monitor: &Monitor) -> Vec<Target> {
    let area = monitor.logical();
    items
        .iter()
        .filter_map(|it| {
            let bounds =
                Rect::new(origin.0 + it.x, origin.1 + it.y, it.w, it.h).intersect(&area)?;
            Some((bounds, it.text.clone()))
        })
        .enumerate()
        .map(|(i, (bounds, text))| Target {
            id: format!("b{i}"),
            bounds,
            role: Role::Button,
            text,
            source: From::Shell,
            actions: vec![ActionKind::Click, ActionKind::Right],
            window: String::new(),
        })
        .collect()
}

impl Source for Shell {
    fn name(&self) -> &'static str {
        "shell"
    }

    fn budget(&self) -> Duration {
        self.budget
    }

    fn collect(&self, _: &Window, monitor: &Monitor) -> Found {
        let items = self
            .items
            .lock()
            .map_err(|_| "shell lock".to_string())?
            .get(&monitor.name)
            .cloned()
            .unwrap_or_default();
        if items.is_empty() {
            return Ok(SourceTargets::new(
                Vec::new(),
                format!("no bar items reported on {}", monitor.name),
            ));
        }
        let Some(origin) = crate::hypr::layer_origin(&monitor.name, BAR_NAMESPACE) else {
            return Ok(SourceTargets::new(
                Vec::new(),
                format!("no bar on {}", monitor.name),
            ));
        };
        let targets = shell_targets(&items, origin, monitor);
        let said = format!("{} targets from the bar", targets.len());
        Ok(SourceTargets::new(targets, said))
    }
}

/// The layer-shell namespace of Omarchy's bar.
const BAR_NAMESPACE: &str = "omarchy-bar";

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use thodu_core::protocol::LabeledTarget;
    use thodu_core::session::Session;

    struct Fake {
        name: &'static str,
        budget: Duration,
        sleep: Duration,
        found: usize,
    }

    impl Source for Fake {
        fn name(&self) -> &'static str {
            self.name
        }

        fn budget(&self) -> Duration {
            self.budget
        }

        fn collect(&self, _: &Window, _: &Monitor) -> Found {
            thread::sleep(self.sleep);
            let target = Target {
                id: String::new(),
                bounds: Rect::new(0, 0, 10, 10),
                role: Role::Button,
                text: self.name.into(),
                source: From::Atspi,
                actions: Vec::new(),
                window: String::new(),
            };
            Ok(SourceTargets::new(
                vec![target; self.found],
                format!("{} found {}", self.name, self.found),
            ))
        }
    }

    struct Conditional {
        name: &'static str,
        is_fallback: bool,
        fallback_on_error: bool,
        request_fallback: bool,
        fail: bool,
        source: From,
        calls: Arc<AtomicUsize>,
    }

    struct SlowFallback;

    impl Source for SlowFallback {
        fn name(&self) -> &'static str {
            "image"
        }

        fn budget(&self) -> Duration {
            Duration::from_millis(20)
        }

        fn is_fallback(&self) -> bool {
            true
        }

        fn collect(&self, _: &Window, _: &Monitor) -> Found {
            thread::sleep(Duration::from_millis(60));
            Ok(SourceTargets::new(Vec::new(), "late image result"))
        }
    }

    impl Source for Conditional {
        fn name(&self) -> &'static str {
            self.name
        }

        fn budget(&self) -> Duration {
            Duration::from_millis(100)
        }

        fn is_fallback(&self) -> bool {
            self.is_fallback
        }

        fn fallback_on_error(&self) -> bool {
            self.fallback_on_error
        }

        fn collect(&self, _: &Window, _: &Monitor) -> Found {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if self.fail {
                return Err("test failure".into());
            }
            let target = Target {
                id: self.name.into(),
                bounds: Rect::new(if self.source == From::Image { 40 } else { 10 }, 10, 10, 10),
                role: Role::Text,
                text: String::new(),
                source: self.source,
                actions: vec![ActionKind::Click],
                window: "0x1".into(),
            };
            let mut result = SourceTargets::new(vec![target], self.name);
            result.request_fallback = self.request_fallback;
            Ok(result)
        }
    }

    fn window() -> Window {
        Window {
            address: "0x1".into(),
            class: "app".into(),
            title: String::new(),
            pid: 1,
            at: (0, 0),
            size: (100, 100),
            workspace: 1,
            monitor: 0,
            ..Window::default()
        }
    }

    fn monitor() -> Monitor {
        Monitor {
            name: "M".into(),
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            scale: 1.0,
        }
    }

    // REQ-SRC-07: a floating window stays actionable over a tiled window,
    // even when the tiled window has the newer focus history.
    #[test]
    fn req_src_07_targets_follow_floating_over_tiled_stacking() {
        let mut tiled = window();
        tiled.focus_history_id = 0;
        let mut floating = window();
        floating.address = "0x2".into();
        floating.at = (50, 0);
        floating.floating = true;
        floating.focus_history_id = 3;
        let target = |id: &str, x, window: &str| Target {
            id: id.into(),
            bounds: Rect::new(x, 10, 20, 20),
            role: Role::Button,
            text: id.into(),
            source: From::Atspi,
            actions: vec![ActionKind::Click],
            window: window.into(),
        };
        let mut targets = vec![
            target("covered", 60, "0x1"),
            target("floating", 60, "0x2"),
            target("visible", 120, "0x2"),
        ];

        keep_targets_on_topmost_windows(&mut targets, &[tiled, floating]);

        assert_eq!(
            targets
                .iter()
                .map(|target| target.id.as_str())
                .collect::<Vec<_>>(),
            ["floating", "visible"]
        );
    }

    struct EveryWindow;

    impl Source for EveryWindow {
        fn name(&self) -> &'static str {
            "atspi"
        }

        fn budget(&self) -> Duration {
            Duration::from_millis(100)
        }

        fn collect(&self, window: &Window, _: &Monitor) -> Found {
            Ok(SourceTargets::new(
                vec![Target {
                    id: "a0".into(),
                    bounds: Rect::new(window.at.0 + 10, window.at.1 + 10, 20, 20),
                    role: Role::Button,
                    text: window.address.clone(),
                    source: From::Atspi,
                    actions: vec![ActionKind::Click],
                    window: window.address.clone(),
                }],
                "targets from every window",
            ))
        }

        fn collect_many(&self, windows: &[Window], _: &Monitor) -> Found {
            Ok(SourceTargets::new(
                windows
                    .iter()
                    .map(|window| Target {
                        id: "a0".into(),
                        bounds: Rect::new(window.at.0 + 10, window.at.1 + 10, 20, 20),
                        role: Role::Button,
                        text: window.address.clone(),
                        source: From::Atspi,
                        actions: vec![ActionKind::Click],
                        window: window.address.clone(),
                    })
                    .collect(),
                "targets from every window",
            ))
        }
    }

    fn targets_from_two_windows() -> Vec<Target> {
        let first = window();
        let mut second = window();
        second.address = "0x2".into();
        second.at = (100, 0);
        let monitor = Monitor {
            width: 200,
            ..monitor()
        };
        let sources: Vec<Arc<dyn Source>> = vec![Arc::new(EveryWindow)];
        collect_visible(&sources, &[first, second], &monitor).targets
    }

    // REQ-SRC-07: one collection request carries targets from all windows
    // and preserves their distinct compositor addresses.
    #[test]
    fn req_src_07_source_collection_keeps_all_visible_window_targets() {
        let first = window();
        let mut second = window();
        second.address = "0x2".into();
        second.at = (100, 0);
        let monitor = Monitor {
            width: 200,
            ..monitor()
        };
        let sources: Vec<Arc<dyn Source>> = vec![Arc::new(EveryWindow)];

        let got = collect_visible(&sources, &[first.clone(), second.clone()], &monitor);

        assert_eq!(got.targets.len(), 2);
        assert_eq!(got.targets[0].window, first.address);
        assert_eq!(got.targets[1].window, second.address);
    }

    // REQ-SESSION-01: the default session labels targets from every visible
    // window while retaining each target's compositor address.
    #[test]
    fn req_session_01_one_session_labels_targets_from_every_visible_window() {
        let session = Session::new(targets_from_two_windows(), false);

        assert_eq!(session.targets.len(), 2);
        assert_eq!(
            session
                .targets
                .iter()
                .map(|target| target.window.as_str())
                .collect::<Vec<_>>(),
            ["0x1", "0x2"]
        );
        assert_ne!(session.labels[0], session.labels[1]);
    }

    // REQ-CLI-01: JSON output keeps the addresses of visible-window targets
    // on separate records, without labels.
    #[test]
    fn req_cli_01_visible_window_json_keeps_distinct_addresses() {
        let lines: Vec<serde_json::Value> = targets_from_two_windows()
            .into_iter()
            .map(|target| LabeledTarget::unlabeled(target).json_line())
            .map(|line| serde_json::from_str(&line).expect("JSON target"))
            .collect();

        assert_eq!(
            lines
                .iter()
                .map(|line| line["window"].as_str().expect("window address"))
                .collect::<Vec<_>>(),
            ["0x1", "0x2"]
        );
        assert!(lines.iter().all(|line| line.get("label").is_none()));
    }

    // REQ-SRC-05: a source that sleeps past its budget is left out and
    // named; the others' targets are shown, and the session does not wait
    // for the slow one.
    #[test]
    fn req_src_05_a_slow_source_is_left_out_and_named() {
        let sources: Vec<Arc<dyn Source>> = vec![
            Arc::new(Fake {
                name: "slow",
                budget: Duration::from_millis(40),
                sleep: Duration::from_millis(400),
                found: 5,
            }),
            Arc::new(Fake {
                name: "fast",
                budget: Duration::from_millis(80),
                sleep: Duration::from_millis(5),
                found: 2,
            }),
        ];
        let started = Instant::now();
        let got = collect(&sources, &window(), &monitor());
        let took = started.elapsed();
        assert_eq!(got.targets.len(), 2, "only the fast source's targets");
        assert!(got.targets.iter().all(|t| t.text == "fast"));
        assert_eq!(got.status, ["slow missed its 40 ms budget", "fast found 2"]);
        assert!(
            took < Duration::from_millis(200),
            "waited {took:?} for a source past its budget"
        );
    }

    // REQ-SHELL-01: the bar's items become shell targets at the bar's
    // origin on the monitor, clipped to it, with no window.
    #[test]
    fn req_shell_01_bar_items_become_shell_targets() {
        let items = vec![
            ShellItem {
                x: 10,
                y: 2,
                w: 30,
                h: 22,
                text: "1".into(),
            },
            ShellItem {
                x: 40,
                y: 2,
                w: 30,
                h: 22,
                text: "2".into(),
            },
            ShellItem {
                x: 72,
                y: 3,
                w: 40,
                h: 20,
                text: "omarchy.tray".into(),
            },
        ];
        let m = Monitor {
            name: "B".into(),
            x: 1536,
            y: 0,
            width: 100,
            height: 50,
            scale: 1.0,
        };
        let got = shell_targets(&items, (1536, 0), &m);
        let summary: Vec<(Rect, From, &str, &str)> = got
            .iter()
            .map(|t| (t.bounds, t.source, t.text.as_str(), t.window.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                (Rect::new(1546, 2, 30, 22), From::Shell, "1", ""),
                (Rect::new(1576, 2, 30, 22), From::Shell, "2", ""),
                (Rect::new(1608, 3, 28, 20), From::Shell, "omarchy.tray", ""),
            ],
            "the tray icon is clipped at the monitor's right edge"
        );
        assert_eq!(
            got.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["b0", "b1", "b2"]
        );
    }

    // REQ-SRC-05: sources in time all count, and the session waits for
    // nobody once they have answered.
    #[test]
    fn req_src_05_sources_in_time_all_count() {
        let sources: Vec<Arc<dyn Source>> = vec![
            Arc::new(Fake {
                name: "a",
                budget: Duration::from_millis(500),
                sleep: Duration::from_millis(1),
                found: 1,
            }),
            Arc::new(Fake {
                name: "b",
                budget: Duration::from_millis(500),
                sleep: Duration::from_millis(1),
                found: 3,
            }),
        ];
        let started = Instant::now();
        let got = collect(&sources, &window(), &monitor());
        assert_eq!(got.targets.len(), 4);
        assert!(started.elapsed() < Duration::from_millis(250));
    }

    // REQ-SRC-06: accessibility targets take precedence over overlapping
    // image detections, while uncovered image targets remain available.
    #[test]
    fn req_src_06_image_targets_fill_only_uncovered_areas() {
        let target = |id: &str, x, source| Target {
            id: id.into(),
            bounds: Rect::new(x, 10, 20, 20),
            role: Role::Text,
            text: String::new(),
            source,
            actions: vec![ActionKind::Click],
            window: "0x1".into(),
        };
        let mut targets = vec![
            target("a0", 10, From::Atspi),
            target("i0", 12, From::Image),
            target("i1", 60, From::Image),
        ];
        remove_covered_image_targets(&mut targets);
        assert_eq!(
            targets.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["a0", "i1"]
        );
    }

    // REQ-SRC-06: ordinary windows do not pay for image capture and detection.
    #[test]
    fn req_src_06_image_fallback_is_skipped_for_complete_primary_sources() {
        let primary_calls = Arc::new(AtomicUsize::new(0));
        let image_calls = Arc::new(AtomicUsize::new(0));
        let sources: Vec<Arc<dyn Source>> = vec![
            Arc::new(Conditional {
                name: "atspi",
                is_fallback: false,
                fallback_on_error: true,
                request_fallback: false,
                fail: false,
                source: From::Atspi,
                calls: Arc::clone(&primary_calls),
            }),
            Arc::new(Conditional {
                name: "image",
                is_fallback: true,
                fallback_on_error: false,
                request_fallback: false,
                fail: false,
                source: From::Image,
                calls: Arc::clone(&image_calls),
            }),
        ];

        let got = collect(&sources, &window(), &monitor());
        assert_eq!(got.targets.len(), 1);
        assert_eq!(primary_calls.load(Ordering::Relaxed), 1);
        assert_eq!(image_calls.load(Ordering::Relaxed), 0);
        assert_eq!(got.status, ["atspi"]);
    }

    // REQ-SRC-06: incomplete trees and source errors both run the image fallback.
    #[test]
    fn req_src_06_requested_or_failed_primary_runs_image_fallback() {
        for (request_fallback, fail) in [(true, false), (false, true)] {
            let primary_calls = Arc::new(AtomicUsize::new(0));
            let image_calls = Arc::new(AtomicUsize::new(0));
            let sources: Vec<Arc<dyn Source>> = vec![
                Arc::new(Conditional {
                    name: "atspi",
                    is_fallback: false,
                    fallback_on_error: true,
                    request_fallback,
                    fail,
                    source: From::Atspi,
                    calls: Arc::clone(&primary_calls),
                }),
                Arc::new(Conditional {
                    name: "image",
                    is_fallback: true,
                    fallback_on_error: false,
                    request_fallback: false,
                    fail: false,
                    source: From::Image,
                    calls: Arc::clone(&image_calls),
                }),
            ];

            let got = collect(&sources, &window(), &monitor());
            assert_eq!(primary_calls.load(Ordering::Relaxed), 1);
            assert_eq!(image_calls.load(Ordering::Relaxed), 1);
            assert!(
                got.targets
                    .iter()
                    .any(|target| target.source == From::Image)
            );
            assert!(got.status.iter().any(|status| status == "image"));
        }
    }

    // REQ-SRC-06: a hints session can draw primary results before it starts
    // the requested visual fallback.
    #[test]
    fn req_src_06_primary_collection_defers_the_requested_fallback() {
        let primary_calls = Arc::new(AtomicUsize::new(0));
        let fallback_calls = Arc::new(AtomicUsize::new(0));
        let sources: Vec<Arc<dyn Source>> = vec![
            Arc::new(Conditional {
                name: "atspi",
                is_fallback: false,
                fallback_on_error: true,
                request_fallback: true,
                fail: false,
                source: From::Atspi,
                calls: Arc::clone(&primary_calls),
            }),
            Arc::new(Conditional {
                name: "image",
                is_fallback: true,
                fallback_on_error: false,
                request_fallback: false,
                fail: false,
                source: From::Image,
                calls: Arc::clone(&fallback_calls),
            }),
        ];

        let primary = collect_primary_visible(&sources, &[window()], &monitor());
        assert!(primary.fallback_requested);
        assert_eq!(primary.targets.len(), 1);
        assert_eq!(primary_calls.load(Ordering::Relaxed), 1);
        assert_eq!(fallback_calls.load(Ordering::Relaxed), 0);

        let fallback = collect_fallback_visible(&sources, &[window()], &monitor());
        assert_eq!(fallback.targets.len(), 1);
        assert_eq!(fallback_calls.load(Ordering::Relaxed), 1);
    }

    // REQ-SRC-05: a requested fallback still observes its own budget.
    #[test]
    fn req_src_05_requested_fallback_past_budget_is_left_out_and_named() {
        let sources: Vec<Arc<dyn Source>> = vec![
            Arc::new(Conditional {
                name: "atspi",
                is_fallback: false,
                fallback_on_error: true,
                request_fallback: true,
                fail: false,
                source: From::Atspi,
                calls: Arc::new(AtomicUsize::new(0)),
            }),
            Arc::new(SlowFallback),
        ];

        let got = collect(&sources, &window(), &monitor());
        assert_eq!(got.status, ["atspi", "image missed its 20 ms budget"]);
        assert!(
            got.targets
                .iter()
                .all(|target| target.source != From::Image)
        );
    }
}
