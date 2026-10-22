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

/// A source's targets and what to say about them, or why it found none.
pub type Found = Result<(Vec<Target>, String), String>;

pub trait Source: Send + Sync {
    fn name(&self) -> &'static str;
    fn budget(&self) -> Duration;
    /// The targets, and what to say about them in the status line.
    fn collect(&self, window: &Window, monitor: &Monitor) -> Found;
}

pub struct Collected {
    pub targets: Vec<Target>,
    pub status: Vec<String>,
}

/// Run every source at once and wait for each until its budget is spent.
pub fn collect(sources: &[Arc<dyn Source>], window: &Window, monitor: &Monitor) -> Collected {
    let started = Instant::now();
    let (tx, rx) = mpsc::channel();
    for (i, source) in sources.iter().enumerate() {
        let source = Arc::clone(source);
        let tx = tx.clone();
        let (window, monitor) = (window.clone(), monitor.clone());
        thread::spawn(move || {
            let _ = tx.send((i, source.collect(&window, &monitor)));
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
    for (source, result) in sources.iter().zip(results) {
        match result {
            Some(Ok((found, said))) => {
                targets.extend(found);
                status.push(said);
            }
            Some(Err(e)) => status.push(format!("{}: {e}", source.name())),
            None => status.push(format!(
                "{} missed its {} ms budget",
                source.name(),
                source.budget().as_millis()
            )),
        }
    }
    Collected { targets, status }
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

    fn collect(&self, window: &Window, monitor: &Monitor) -> Found {
        let started = Instant::now();
        let mut bus = self.bus.lock().map_err(|_| "bus lock".to_string())?;
        if bus.is_none() {
            *bus = Some(atspi::Bus::connect()?);
        }
        let reader = bus.as_ref().ok_or("no bus")?;
        let mut woken = self.woken.lock().map_err(|_| "wake lock".to_string())?;
        // The walk stops short of the budget, so its partial tree still
        // arrives in time.
        let mut walk = atspi::Walk::new(self.budget.saturating_sub(self.budget / 8));
        let snapshot = atspi::snapshot(reader, window, monitor, &mut walk, &mut woken);
        let took = started.elapsed().as_millis();
        if snapshot.apps.is_empty() {
            // REQ-SESSION-05: say what the app needs; the other sources
            // carry on.
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
                return Ok((Vec::new(), unregistered::inspection_failed(&window.class)));
            }
            return Ok((
                Vec::new(),
                unregistered::message(&window.class, &exe, &libs),
            ));
        }
        let targets = tree::match_frame(&snapshot)
            .map(|frame| tree::targets(frame, window, monitor))
            .unwrap_or_default();
        let cut = if walk.truncated {
            ", cut short at the budget"
        } else {
            ""
        };
        let said = format!("{} targets from atspi in {took} ms{cut}", targets.len());
        Ok((targets, said))
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
            return Ok((
                Vec::new(),
                format!("no bar items reported on {}", monitor.name),
            ));
        }
        let Some(origin) = crate::hypr::layer_origin(&monitor.name, BAR_NAMESPACE) else {
            return Ok((Vec::new(), format!("no bar on {}", monitor.name)));
        };
        let targets = shell_targets(&items, origin, monitor);
        let said = format!("{} targets from the bar", targets.len());
        Ok((targets, said))
    }
}

/// The layer-shell namespace of Omarchy's bar.
const BAR_NAMESPACE: &str = "omarchy-bar";

#[cfg(test)]
mod tests {
    use super::*;

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
            Ok((
                vec![target; self.found],
                format!("{} found {}", self.name, self.found),
            ))
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
}
