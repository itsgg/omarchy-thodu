//! Requirement tests for thodu-core, each named for the SPEC requirement it
//! checks, against the conformance cases in spec/cases/.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use proptest::prelude::*;
use serde_json::Value;
use thodu_core::config::{self, Config};
use thodu_core::grid;
use thodu_core::input;
use thodu_core::labels::{self, ALPHABET};
use thodu_core::model::{Monitor, Rect, Role, Source, Target, Window};
use thodu_core::perf;
use thodu_core::protocol::{Envelope, LabeledTarget, OpenEvent, Reply, Request, Timing};
use thodu_core::session::{Button, DOUBLE_MS, Key, Session, State};
use thodu_core::stale::is_stale;
use thodu_core::tree::{self, App, Snapshot};
use thodu_core::unregistered;

fn cases(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/cases")
        .join(path)
}

fn jsonl(path: &str) -> Vec<Value> {
    fs::read_to_string(cases(path))
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{path}: {e}")))
        .collect()
}

fn snapshot(path: &str) -> Snapshot {
    serde_json::from_str(&fs::read_to_string(cases(path)).expect("snapshot"))
        .expect("snapshot json")
}

fn monitor() -> Monitor {
    Monitor {
        name: "eDP-1".into(),
        x: 0,
        y: 0,
        width: 1920,
        height: 1200,
        scale: 1.25,
    }
}

fn window() -> Window {
    Window {
        address: "0x1".into(),
        class: "app".into(),
        title: "t".into(),
        pid: 1,
        at: (100, 200),
        size: (400, 300),
        workspace: 1,
        monitor: 0,
    }
}

fn target(text: &str, bounds: Rect) -> Target {
    Target {
        id: String::new(),
        bounds,
        role: Role::Button,
        text: text.into(),
        source: Source::Atspi,
        actions: Vec::new(),
        window: "0x1".into(),
    }
}

/// The recorded reference windows of REQ-SRC-02 (change 0001).
const REFERENCES: [&str; 5] = [
    "gtk3-reference",
    "gtk4-reference",
    "qt-reference",
    "chromium-reference",
    "obsidian-reference",
];

/// A recorded tree's targets in reading order, as sessions and `thodu
/// targets` see them.
fn replayed(name: &str) -> Vec<Target> {
    let snap = snapshot(&format!("atspi/{name}.json"));
    let frame = tree::match_frame(&snap).expect("a frame for the window");
    let mut targets = tree::targets(frame, &snap.window, &snap.monitor);
    labels::reading_order(&mut targets);
    targets
}

fn expected(name: &str) -> Vec<(Rect, Role, String)> {
    jsonl(&format!("atspi/{name}.targets.jsonl"))
        .into_iter()
        .map(|v| {
            let b: Vec<i32> = serde_json::from_value(v["bounds"].clone()).expect("bounds");
            let role: Role = serde_json::from_value(v["role"].clone()).expect("role");
            (
                Rect::new(b[0], b[1], b[2], b[3]),
                role,
                v["text"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

fn summary(targets: &[Target]) -> Vec<(Rect, Role, String)> {
    targets
        .iter()
        .map(|t| (t.bounds, t.role, t.text.clone()))
        .collect()
}

// REQ-SESSION-01: each recorded tree yields the targets marked beside it.
#[test]
fn req_session_01_recorded_trees_yield_their_marked_targets() {
    // Which targets, not their order: both sides in reading order.
    let ordered = |mut v: Vec<(Rect, Role, String)>| {
        v.sort_by_key(|(r, _, _)| (r.y, r.x));
        v
    };
    for name in ["covered-dialog"].iter().chain(&REFERENCES) {
        assert_eq!(
            ordered(summary(&replayed(name))),
            ordered(expected(name)),
            "{name}"
        );
    }
}

// REQ-SESSION-01: every target lies inside its window and has area.
#[test]
fn req_session_01_targets_lie_inside_their_window() {
    let snap = snapshot("atspi/gtk3-reference.json");
    for t in replayed("gtk3-reference") {
        assert!(
            snap.window.rect().contains(&t.bounds) && !t.bounds.is_empty(),
            "{t:?}"
        );
        assert_eq!(t.window, snap.window.address);
    }
}

// REQ-SRC-01: the frame is found by process and ACTIVE state, never by name.
#[test]
fn req_src_01_frames_are_matched_by_process() {
    for case in jsonl("atspi/match.jsonl") {
        let window: Window = serde_json::from_value(case["window"].clone()).expect("window");
        let apps: Vec<App> = serde_json::from_value(case["apps"].clone()).expect("apps");
        let snap = Snapshot {
            window,
            monitor: monitor(),
            apps,
        };
        let got = tree::match_frame(&snap).map(|f| f.name.clone());
        let want = case["expected"].as_str().map(String::from);
        assert_eq!(got, want, "{}", case["case"]);
    }
}

// REQ-SRC-04: hidden elements and a closed popup's items are not targets.
#[test]
fn req_src_04_hidden_and_closed_popups_are_dropped() {
    let texts: Vec<String> = replayed("gtk3-reference")
        .into_iter()
        .map(|t| t.text)
        .collect();
    for absent in ["Hidden", "Medium", "Large"] {
        assert!(
            !texts.iter().any(|t| t == absent),
            "{absent} became a target"
        );
    }
}

// REQ-SRC-04: an element a later dialog covers is not a target.
#[test]
fn req_src_04_covered_elements_are_dropped() {
    let texts: Vec<String> = replayed("covered-dialog")
        .into_iter()
        .map(|t| t.text)
        .collect();
    assert!(!texts.contains(&"Behind".to_string()));
    assert!(texts.contains(&"OK".to_string()));
}

// REQ-LABEL-01: the golden cases of the assignment rule.
#[test]
fn req_label_01_assignment_cases() {
    for case in jsonl("labels/assign.jsonl") {
        let n = case["n"].as_u64().expect("n") as usize;
        let got = labels::assign(n);
        assert_eq!(got.len(), n);
        let first: Vec<String> = serde_json::from_value(case["first"].clone()).expect("first");
        let last: Vec<String> = serde_json::from_value(case["last"].clone()).expect("last");
        assert_eq!(
            got.iter().take(3).cloned().collect::<Vec<_>>(),
            first,
            "n={n}"
        );
        assert_eq!(
            got.iter()
                .skip(n.saturating_sub(3))
                .cloned()
                .collect::<Vec<_>>(),
            last,
            "n={n}"
        );
        assert_eq!(
            got.iter().map(String::len).max().unwrap_or(0),
            case["longest"].as_u64().expect("longest") as usize
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, rng_seed: proptest::test_runner::RngSeed::Fixed(1), ..ProptestConfig::default() })]

    // REQ-LABEL-01: distinct, prefix-free, at most three letters, home row first.
    #[test]
    fn req_label_01_labels_are_distinct_and_prefix_free(n in 1usize..2000) {
        let got = labels::assign(n);
        prop_assert_eq!(got.len(), n);
        let unique: HashSet<&String> = got.iter().collect();
        prop_assert_eq!(unique.len(), n);
        for a in &got {
            prop_assert!(a.len() <= 3 && a.chars().all(|c| ALPHABET.contains(c)));
        }
        // In sorted order a label that prefixes any other prefixes the next one.
        let mut sorted = got.clone();
        sorted.sort();
        for w in sorted.windows(2) {
            prop_assert!(!w[1].starts_with(w[0].as_str()), "{} is a prefix of {}", w[0], w[1]);
        }
        prop_assert!(got.windows(2).all(|w| w[0].len() <= w[1].len()), "shortest first");
    }

    // REQ-LABEL-02: inside a large enough target, else on its corner, always on the monitor.
    #[test]
    fn req_label_02_labels_sit_on_their_targets(
        x in 0i32..1500, y in 0i32..900, w in 1i32..400, h in 1i32..300, lw in 8i32..40, lh in 8i32..24,
    ) {
        let screen = Rect::new(0, 0, 1536, 960);
        let t = Rect::new(x, y, w, h);
        let placed = labels::place(&t, (lw, lh), &screen);
        prop_assert!(screen.contains(&placed));
        if w >= lw && h >= lh && screen.contains(&t) {
            prop_assert!(t.contains(&placed));
        }
    }

    // SPEC section 5 invariant: logical to physical and back moves a point by at most one unit.
    #[test]
    fn invariant_coordinates_round_trip(px in 0i32..1536, py in 0i32..960) {
        let m = monitor();
        let (lx, ly) = m.to_logical(px, py);
        let (bx, by) = m.to_physical(lx, ly);
        let (rx, ry) = m.to_logical(bx, by);
        prop_assert!((rx - lx).abs() <= 1 && (ry - ly).abs() <= 1);
    }
}

// REQ-SEARCH-01: typed letters keep the targets whose text holds them.
#[test]
fn req_search_01_filter_cases() {
    for case in jsonl("search/filter.jsonl") {
        let texts: Vec<String> = serde_json::from_value(case["texts"].clone()).expect("texts");
        let targets: Vec<Target> = texts
            .iter()
            .enumerate()
            .map(|(i, t)| target(t, Rect::new(0, i as i32 * 40, 100, 30)))
            .collect();
        let mut s = Session::new(targets, false);
        for c in case["typed"].as_str().expect("typed").chars() {
            s.key(Key::Char { c, shift: false });
        }
        let kept: Vec<usize> = serde_json::from_value(case["kept"].clone()).expect("kept");
        assert_eq!(s.visible(), kept, "{}", case["case"]);
        assert_eq!(
            s.focused,
            case["focused"].as_u64().map(|f| f as usize),
            "{}",
            case["case"]
        );
    }
}

// REQ-SEARCH-01: Shift and a label focuses that target; Tab moves the focus.
#[test]
fn req_search_01_shift_types_labels_and_tab_moves() {
    let targets: Vec<Target> = (0..30)
        .map(|i| target(&format!("t{i}"), Rect::new(0, i * 40, 100, 30)))
        .collect();
    let mut s = Session::new(targets, false);
    let label = s.labels[29].clone();
    for c in label.chars() {
        s.key(Key::Char {
            c: c.to_ascii_uppercase(),
            shift: true,
        });
    }
    assert_eq!(s.focused, Some(29));
    assert!(s.search.is_empty());
    s.key(Key::Tab { shift: false });
    assert_eq!(s.focused, Some(0));
    s.key(Key::Tab { shift: true });
    assert_eq!(s.focused, Some(29));
}

// REQ-ACT-01: Return clicks the focused target's center with the left button.
#[test]
fn req_act_01_return_clicks_the_center() {
    let mut s = Session::new(vec![target("Save", Rect::new(100, 200, 80, 30))], false);
    for c in "save".chars() {
        s.key(Key::Char { c, shift: false });
    }
    let act = s
        .key(Key::Return {
            shift: false,
            ctrl: false,
        })
        .expect("an action");
    assert_eq!(
        (act.point, act.button, act.count, act.ctrl),
        ((140, 215), Button::Left, 1, false)
    );
    assert_eq!(s.state, State::Acting);
    assert_eq!(
        s.key_at(
            Key::Return {
                shift: false,
                ctrl: false
            },
            DOUBLE_MS + 1
        ),
        None,
        "a session acts once, unless a second Return comes in time (REQ-ACT-02)"
    );
    assert_eq!(s.state, State::Closed);
    let physical = monitor().to_physical(act.point.0, act.point.1);
    assert_eq!(physical, (175, 269), "at scale 1.25");
}

// REQ-ACT-01: Return with nothing focused does nothing; Escape ends the session.
#[test]
fn req_act_01_return_needs_a_focus() {
    let mut s = Session::new(vec![target("Save", Rect::new(0, 0, 80, 30))], false);
    assert_eq!(
        s.key(Key::Return {
            shift: false,
            ctrl: false
        }),
        None
    );
    assert_eq!(s.state, State::Showing);
    s.key(Key::Escape);
    assert_eq!(s.state, State::Closed);
}

// REQ-ACT-03: a window that moved, resized, closed or left its workspace is stale.
#[test]
fn req_act_03_moved_windows_are_stale() {
    let w = window();
    assert!(!is_stale(&w, Some(&w)));
    assert!(
        is_stale(
            &w,
            Some(&Window {
                at: (110, 200),
                ..w.clone()
            })
        ),
        "moved 10 to the right"
    );
    assert!(
        is_stale(
            &w,
            Some(&Window {
                size: (400, 301),
                ..w.clone()
            })
        ),
        "resized"
    );
    assert!(
        is_stale(
            &w,
            Some(&Window {
                workspace: 2,
                ..w.clone()
            })
        ),
        "other workspace"
    );
    assert!(
        is_stale(
            &w,
            Some(&Window {
                address: "0x2".into(),
                ..w.clone()
            })
        ),
        "another window"
    );
    assert!(is_stale(&w, None), "closed");
}

// The overlay's requests (SPEC section 7, changes/0002): a heartbeat is a
// brief session request, and a session request is full unless it says so.
#[test]
fn overlay_requests_parse() {
    let parse = |line: &str| -> Request {
        serde_json::from_str::<Envelope>(line)
            .unwrap_or_else(|e| panic!("{line}: {e}"))
            .request
    };
    assert_eq!(
        parse(r#"{"v":1,"op":"session","session":"s1"}"#),
        Request::Session {
            session: "s1".into(),
            brief: false
        }
    );
    assert_eq!(
        parse(r#"{"v":1,"op":"session","session":"s1","brief":true}"#),
        Request::Session {
            session: "s1".into(),
            brief: true
        }
    );
    assert_eq!(
        parse(r#"{"v":1,"op":"hidden","session":"s1"}"#),
        Request::Hidden {
            session: "s1".into()
        }
    );
    assert_eq!(
        parse(r#"{"v":1,"op":"key","session":"s1","key":"s","shift":true}"#),
        Request::Key {
            session: "s1".into(),
            key: "s".into(),
            shift: true,
            ctrl: false
        }
    );
}

// REQ-PERF-01: the thresholds bench holds sessions to, and its percentiles.
#[test]
fn req_perf_01_thresholds_and_percentiles() {
    assert_eq!(perf::MEDIAN_MS, 100.0);
    assert_eq!(perf::P95_MS, 250.0);
    let samples: Vec<f64> = (1..=100).map(f64::from).collect();
    assert_eq!(perf::percentile(&samples, 50.0), Some(50.0));
    assert_eq!(perf::percentile(&samples, 95.0), Some(95.0));
    assert_eq!(perf::percentile(&[7.0], 95.0), Some(7.0));
    assert_eq!(perf::percentile(&[], 50.0), None);
    let unsorted = [30.0, 10.0, 20.0];
    assert_eq!(perf::percentile(&unsorted, 50.0), Some(20.0));
    assert_eq!(perf::percentile(&unsorted, 95.0), Some(30.0));
}

// REQ-PERF-01: bench fails a run over either threshold, and passes one at
// them.
#[test]
fn req_perf_01_verdict() {
    let at = |median_ms, p95_ms| perf::Summary {
        sessions: 50,
        median_ms,
        p95_ms,
    };
    assert!(at(100.0, 250.0).within());
    assert!(!at(100.1, 120.0).within());
    assert!(!at(40.0, 250.1).within());
    let mut slow_tail: Vec<f64> = vec![20.0; 47];
    slow_tail.extend([300.0, 300.0, 300.0]);
    let summary = perf::summarize(&slow_tail).expect("samples");
    assert_eq!(summary.median_ms, 20.0);
    assert_eq!(summary.p95_ms, 300.0);
    assert!(
        !summary.within(),
        "three slow sessions in fifty fail the 95th percentile"
    );
}

// The pushed session (changes/0003) carries the full reply.
#[test]
fn open_event_round_trips() {
    let event = OpenEvent {
        v: 1,
        event: "open".into(),
        session: "s1".into(),
        test: true,
        reply: Reply {
            session: Some("s1".into()),
            timing: Some(Timing {
                collect_ms: 12,
                drawn_at_us: None,
            }),
            ..Reply::ok()
        },
    };
    let line = serde_json::to_string(&event).expect("serializes");
    assert!(line.contains(r#""event":"open""#), "{line}");
    let back: OpenEvent = serde_json::from_str(&line).expect("parses");
    assert_eq!(back, event);
    assert_eq!(
        serde_json::from_str::<Envelope>(r#"{"v":1,"op":"listen"}"#)
            .expect("listen")
            .request,
        Request::Listen
    );
    assert_eq!(
        serde_json::from_str::<Envelope>(r#"{"v":1,"op":"drawn","session":"s1"}"#)
            .expect("drawn")
            .request,
        Request::Drawn {
            session: "s1".into()
        }
    );
}

// REQ-SEARCH-02: with labels_only, letters type labels and / starts a
// search; without it, the default stays search-first. A key "?label:N"
// types the whole label of target N, without Shift.
#[test]
fn req_search_02_labels_only_cases() {
    for case in jsonl("search/labels-only.jsonl") {
        let name = case["case"].as_str().expect("case");
        let texts: Vec<String> = serde_json::from_value(case["texts"].clone()).expect("texts");
        let targets: Vec<Target> = texts
            .iter()
            .enumerate()
            .map(|(i, t)| target(t, Rect::new(0, i as i32 * 40, 100, 30)))
            .collect();
        let n = targets.len();
        let mut s = Session::new(targets, case["labels_only"].as_bool().expect("labels_only"));
        for key in case["keys"].as_array().expect("keys") {
            let key = key.as_str().expect("key");
            if let Some(i) = key.strip_prefix("?label:") {
                let label = s.labels[i.parse::<usize>().expect("index")].clone();
                for c in label.chars() {
                    s.key(Key::Char { c, shift: false });
                }
                continue;
            }
            let key = match key {
                "BackSpace" => Key::Backspace,
                k => {
                    let c = k.chars().next().expect("a key");
                    Key::Char {
                        c,
                        shift: c.is_uppercase(),
                    }
                }
            };
            s.key(key);
        }
        let kept: Vec<usize> = if case["kept"] == "all" {
            (0..n).collect()
        } else {
            serde_json::from_value(case["kept"].clone()).expect("kept")
        };
        assert_eq!(s.visible(), kept, "{name}");
        assert_eq!(
            s.focused,
            case["focused"].as_u64().map(|f| f as usize),
            "{name}"
        );
        assert_eq!(s.search, case["search"].as_str().expect("search"), "{name}");
    }
}

// REQ-SEARCH-02: the setting comes from config.toml, off unless set, and a
// typo is an error instead of a silent default.
#[test]
fn req_search_02_setting() {
    assert_eq!(config::parse(""), Ok(Config::default()));
    assert!(
        !Config::default().labels_only,
        "search-first is the default"
    );
    assert_eq!(
        config::parse("labels_only = true\n"),
        Ok(Config { labels_only: true })
    );
    assert!(config::parse("label_only = true").is_err(), "unknown key");
    assert!(
        config::parse("labels_only = \"yes\"").is_err(),
        "wrong type"
    );
}

fn role_named(name: &str) -> Role {
    serde_json::from_value(Value::String(name.into())).expect("a role")
}

fn key_named(name: &str) -> Key {
    match name {
        "Return" => Key::Return {
            shift: false,
            ctrl: false,
        },
        "Shift-Return" => Key::Return {
            shift: true,
            ctrl: false,
        },
        "Control-Return" => Key::Return {
            shift: false,
            ctrl: true,
        },
        other => Key::Char {
            c: other.chars().next().expect("a letter"),
            shift: false,
        },
    }
}

// REQ-ACT-02: Shift-Return right-clicks, a second Return within 300 ms
// double-clicks, Control-Return clicks with Control held; the inputs each
// key sequence makes, in order.
#[test]
fn req_act_02_key_cases() {
    for case in jsonl("act/keys.jsonl") {
        let name = case["case"].as_str().expect("case");
        let b: Vec<i32> = serde_json::from_value(case["bounds"].clone()).expect("bounds");
        let mut t = target("Target", Rect::new(b[0], b[1], b[2], b[3]));
        t.role = role_named(case["role"].as_str().expect("role"));
        let mut s = Session::new(vec![t], false);
        s.key(Key::Tab { shift: false });
        let mut inputs = Vec::new();
        let mut first = None;
        for pair in case["keys"].as_array().expect("keys") {
            let key = key_named(pair[0].as_str().expect("key"));
            let at = pair[1].as_u64().expect("ms");
            if let Some(act) = s.key_at(key, at) {
                first.get_or_insert(act.linger);
                inputs.extend(input::plan(&act).iter().map(ToString::to_string));
            }
        }
        let expected: Vec<String> = serde_json::from_value(case["inputs"].clone()).expect("inputs");
        assert_eq!(inputs, expected, "{name}");
        let state = serde_json::to_value(s.state).expect("state");
        assert_eq!(state, case["state"], "{name}");
        assert_eq!(
            first,
            case["linger"].as_bool(),
            "{name}: whether the overlay keeps the keyboard after the first click"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, rng_seed: proptest::test_runner::RngSeed::Fixed(1), ..ProptestConfig::default() })]

    // REQ-GRID-01: for any monitor and any point on it, at most four
    // choices end on a cell of at most 10 units that holds the point.
    #[test]
    fn req_grid_01_every_point_is_reachable(
        w in 1i32..8000,
        h in 1i32..8000,
        ox in -4000i32..4000,
        oy in -2000i32..2000,
        fx in 0.0f64..1.0,
        fy in 0.0f64..1.0,
    ) {
        let area = Rect::new(ox, oy, w, h);
        let px = ox + ((f64::from(w) * fx) as i32).min(w - 1);
        let py = oy + ((f64::from(h) * fy) as i32).min(h - 1);
        let mut s = Session::new(Vec::new(), false).with_area(area);
        s.key(Key::Space);
        let mut choices = 0;
        while s.focused.is_none() {
            prop_assert!(choices < 4, "more than four choices for ({px}, {py}) on {w}x{h}");
            let cells = grid::cells(s.grid.last().expect("a grid"));
            prop_assert!(cells.len() <= 26 * 26);
            let i = s.targets.iter().position(|t| t.bounds.contains_point(px, py));
            prop_assert!(i.is_some(), "the cells cover ({px}, {py})");
            let label = s.labels[i.expect("a cell")].clone();
            for c in label.chars() {
                s.key(Key::Char { c, shift: false });
            }
            choices += 1;
        }
        let cell = s.targets[s.focused.expect("a cell")].bounds;
        prop_assert!(cell.contains_point(px, py));
        prop_assert!(cell.w <= grid::FINE && cell.h <= grid::FINE);
        let act = s.key(Key::Return { shift: false, ctrl: false }).expect("Return clicks the cell");
        prop_assert_eq!(act.point, cell.center());
    }
}

// REQ-GRID-01: the cells tile the area exactly, and Backspace goes back a
// level.
#[test]
fn req_grid_01_cells_tile_and_backspace_goes_back() {
    let area = Rect::new(1536, 0, 1536, 864);
    let cells = grid::cells(&area);
    assert_eq!(cells.len(), 26 * 26);
    let covered: i64 = cells.iter().map(|c| i64::from(c.w) * i64::from(c.h)).sum();
    assert_eq!(covered, 1536 * 864);
    assert!(cells.iter().all(|c| area.contains(c) && !c.is_empty()));
    let mut s =
        Session::new(vec![target("Save", Rect::new(1600, 40, 80, 30))], false).with_area(area);
    s.key(Key::Char {
        c: 's',
        shift: false,
    });
    s.key(Key::Space);
    assert!(
        s.grid.is_empty(),
        "Space with a focused target does not open the grid"
    );
    s.key(Key::Backspace);
    s.key(Key::Space);
    assert_eq!(s.grid.len(), 1);
    let first = s.generation;
    let label = s.labels[0].clone();
    for c in label.chars() {
        s.key(Key::Char { c, shift: true });
    }
    assert_eq!(s.grid.len(), 2, "a chosen cell is refined");
    assert!(s.generation > first, "new targets for the overlay");
    s.key(Key::Backspace);
    assert_eq!(s.grid.len(), 1);
    assert_eq!(s.targets.len(), 26 * 26);
}

// REQ-ACT-01 (change 0004): the pointer goes back unless focus follows the
// mouse and going back would leave the clicked window.
#[test]
fn req_act_01_pointer_goes_back_unless_it_would_take_the_focus() {
    let w = Some("0x1");
    assert!(
        input::should_restore(false, Some("0x2"), w),
        "focus on click"
    );
    assert!(
        input::should_restore(true, w, w),
        "back over the same window"
    );
    assert!(
        !input::should_restore(true, Some("0x2"), w),
        "over another window"
    );
    assert!(!input::should_restore(true, None, w), "off every window");
    assert!(
        input::should_restore(true, None, None),
        "a grid click on the desktop"
    );
}

// REQ-SRC-02: over the recorded GTK3, GTK4, Qt, Chromium and Electron
// reference windows, at least 90 percent of the controls a person marked
// are found, and no target lies outside its window. The threshold is over
// all the marked controls together: a window has about seven, so one miss
// is fourteen points of its own share. Qt's QLabel link, which Qt reports
// as a plain label, is the one known miss; the grid reaches it.
#[test]
fn req_src_02_reference_windows_yield_the_marked_controls() {
    let (mut marked, mut found) = (0, 0);
    let mut report = Vec::new();
    for name in REFERENCES {
        let snap = snapshot(&format!("atspi/{name}.json"));
        let targets = replayed(name);
        for t in &targets {
            assert!(
                snap.window.rect().contains(&t.bounds) && !t.bounds.is_empty(),
                "{name}: {t:?} outside its window"
            );
        }
        let marks = jsonl(&format!("atspi/{name}.marked.jsonl"));
        let hit = marks
            .iter()
            .filter(|m| {
                let (x, y) = (
                    m["at"][0].as_i64().expect("x") as i32,
                    m["at"][1].as_i64().expect("y") as i32,
                );
                targets.iter().any(|t| t.bounds.contains_point(x, y))
            })
            .count();
        report.push(format!("{name} {hit}/{}", marks.len()));
        marked += marks.len();
        found += hit;
    }
    assert!(
        found * 10 >= marked * 9,
        "{found} of {marked} marked controls found: {}",
        report.join(", ")
    );
    assert_eq!(
        report.join(", "),
        "gtk3-reference 7/7, gtk4-reference 7/7, qt-reference 6/7, chromium-reference 7/7, obsidian-reference 9/9"
    );
}

// REQ-CLI-01: the replayed trees printed as `thodu targets --json` prints
// them match their .targets.jsonl files.
#[test]
fn req_cli_01_targets_json_matches_the_recorded_targets() {
    for name in REFERENCES {
        let lines: Vec<Value> = replayed(name)
            .into_iter()
            .map(|t| serde_json::from_str(&LabeledTarget::unlabeled(t).json_line()).expect("json"))
            .collect();
        let printed: Vec<(Rect, Role, String)> = lines
            .iter()
            .map(|v| {
                assert!(v.get("label").is_none(), "no labels in targets --json");
                assert_eq!(v["source"], "atspi");
                let b = &v["bounds"];
                let at = |k: &str| b[k].as_i64().expect("bound") as i32;
                (
                    Rect::new(at("x"), at("y"), at("w"), at("h")),
                    serde_json::from_value(v["role"].clone()).expect("role"),
                    v["text"].as_str().expect("text").to_string(),
                )
            })
            .collect();
        assert_eq!(printed, expected(name), "{name}");
    }
}

// REQ-SESSION-04: with nothing to label, the session opens on the grid.
#[test]
fn req_session_04_nothing_to_label_opens_the_grid() {
    let area = Rect::new(0, 0, 1536, 864);
    let s = Session::begin(Vec::new(), area, false, false);
    assert_eq!(s.grid, [area], "the grid covers the monitor");
    assert_eq!(s.targets.len(), 26 * 26);
    let s = Session::begin(
        vec![target("Save", Rect::new(0, 0, 80, 30))],
        area,
        false,
        false,
    );
    assert!(s.grid.is_empty(), "targets found: no grid");
    let s = Session::begin(
        vec![target("Save", Rect::new(0, 0, 80, 30))],
        area,
        false,
        true,
    );
    assert_eq!(s.grid.len(), 1, "thodu grid asks for it");
}

// REQ-SESSION-05: an app not on the bus gets a status line naming what it
// needs, from its executable and libraries.
#[test]
fn req_session_05_unregistered_apps_are_told_what_they_need() {
    for case in jsonl("atspi/unregistered.jsonl") {
        let name = case["case"].as_str().expect("case");
        let libs: Vec<String> = serde_json::from_value(case["libs"].clone()).expect("libs");
        let exe = case["exe"].as_str().expect("exe");
        let class = case["class"].as_str().expect("class");
        assert_eq!(unregistered::toolkit(exe, &libs), case["toolkit"], "{name}");
        let message = unregistered::message(class, exe, &libs);
        assert!(message.starts_with(class), "{name}: {message}");
        assert!(
            message.contains(case["needs"].as_str().expect("needs")),
            "{name}: {message}"
        );
        if case["toolkit"] == "GTK3" {
            assert!(
                message.contains("remove it and restart the app"),
                "GTK3 advice must remove the blocking variable: {message}"
            );
            assert!(
                !message.contains("set NO_AT_BRIDGE=1"),
                "GTK3 advice must not recommend setting NO_AT_BRIDGE: {message}"
            );
        }
    }

    let gtk_lib = vec!["/usr/lib/libgtk-3.so.0".to_string()];
    for exe in [
        "/usr/lib/slack/slack",
        "/usr/lib/obsidian/obsidian",
        "/usr/lib/discord/Discord",
        "/usr/lib/code/code",
    ] {
        assert_eq!(unregistered::toolkit(exe, &gtk_lib), "Electron", "{exe}");
        assert!(
            unregistered::message("electron-app", exe, &gtk_lib)
                .contains("ACCESSIBILITY_ENABLED=1"),
            "{exe}"
        );
    }
}
