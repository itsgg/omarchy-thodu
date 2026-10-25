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
use thodu_core::model::{
    Layer, Monitor, Owner, Rect, Role, Source, Target, Window, owner_at, shown_beside_fullscreen,
};
use thodu_core::perf;
use thodu_core::protocol::{Envelope, LabeledTarget, OpenEvent, Reply, Request, Timing};
use thodu_core::session::{Button, DOUBLE_MS, Key, ScrollAxis, Session, State};
use thodu_core::stale::{is_stale, target_is_stale};
use thodu_core::tree::{self, App, Node, Snapshot};
use thodu_core::unregistered;

fn cases(path: &str) -> PathBuf {
    let root = std::env::current_dir()
        .expect("test working directory")
        .ancestors()
        .find(|candidate| candidate.join("spec/cases").is_dir())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    root.join("spec/cases").join(path)
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
        layers: Vec::new(),
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
        ..Window::default()
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
        check: None,
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

// REQ-SRC-01 and REQ-SRC-07: active state identifies the focused window,
// while geometry identifies a distinct background sibling when possible.
#[test]
fn req_src_07_same_process_windows_use_their_own_frame_or_skip_ambiguity() {
    let frame = |name: &str, states: &[&str], width: i32, height: i32| Node {
        role: "frame".into(),
        name: name.into(),
        states: states.iter().map(|state| (*state).into()).collect(),
        extents: Some([0, 0, width, height]),
        ..Node::default()
    };
    let snapshot = Snapshot {
        window: Window {
            address: "0x2".into(),
            class: "chromium".into(),
            title: "second".into(),
            pid: 44,
            at: (300, 0),
            size: (300, 200),
            workspace: 1,
            monitor: 0,
            ..Window::default()
        },
        monitor: monitor(),
        apps: vec![App {
            name: "Chromium".into(),
            pid: 44,
            frames: vec![
                frame(
                    "focused sibling",
                    &["showing", "visible", "active"],
                    800,
                    600,
                ),
                frame("background window", &["showing", "visible"], 300, 200),
            ],
        }],
    };
    assert_eq!(
        tree::match_frame_for_window(&snapshot, false).map(|node| node.name.as_str()),
        Some("background window")
    );
    assert_eq!(
        tree::match_frame_for_window(&snapshot, true).map(|node| node.name.as_str()),
        Some("focused sibling")
    );

    let mut ambiguous = snapshot.clone();
    ambiguous.apps[0].frames[0].extents = Some([0, 0, 300, 200]);
    ambiguous.apps[0].frames.push(frame(
        "other background window",
        &["showing", "visible"],
        300,
        200,
    ));
    assert_eq!(tree::match_frame_for_window(&ambiguous, false), None);

    let mut mismatched = snapshot.clone();
    mismatched.apps[0].frames.truncate(1);
    assert_eq!(tree::match_frame_for_window(&mismatched, false), None);
}

// REQ-SRC-01: an explicitly selected background window cannot use its
// process's ACTIVE sibling frame when its own extents do not match.
#[test]
fn req_src_01_nonfocused_named_window_does_not_use_active_sibling_frame() {
    let frame = |name: &str, states: &[&str], width: i32, height: i32| Node {
        role: "frame".into(),
        name: name.into(),
        states: states.iter().map(|state| (*state).into()).collect(),
        extents: Some([0, 0, width, height]),
        ..Node::default()
    };
    let snapshot = Snapshot {
        window: Window {
            address: "0x2".into(),
            pid: 44,
            size: (300, 200),
            focused: false,
            ..Window::default()
        },
        monitor: monitor(),
        apps: vec![App {
            name: "Chromium".into(),
            pid: 44,
            frames: vec![
                frame(
                    "focused sibling",
                    &["showing", "visible", "active"],
                    300,
                    200,
                ),
                frame("different extents", &["showing", "visible"], 301, 200),
            ],
        }],
    };

    assert_eq!(tree::match_frame_indices(&[snapshot]), [None]);
}

// REQ-SRC-07: even a unique size match cannot assign one same-process frame
// to two background windows.
#[test]
fn req_src_07_same_process_frame_claimed_by_two_windows_is_omitted() {
    let frame = |name: &str, states: &[&str], width: i32, height: i32| Node {
        role: "frame".into(),
        name: name.into(),
        states: states.iter().map(|state| (*state).into()).collect(),
        extents: Some([0, 0, width, height]),
        ..Node::default()
    };
    let apps = vec![App {
        name: "Chromium".into(),
        pid: 44,
        frames: vec![
            frame("focused", &["showing", "visible", "active"], 300, 200),
            frame("one background frame", &["showing", "visible"], 300, 200),
            frame("other size", &["showing", "visible"], 301, 200),
        ],
    }];
    let snapshot = |address: &str, focused: bool| Snapshot {
        window: Window {
            address: address.into(),
            pid: 44,
            size: (300, 200),
            focused,
            ..Window::default()
        },
        monitor: monitor(),
        apps: apps.clone(),
    };

    let matches = tree::match_frame_indices(&[
        snapshot("0x1", true),
        snapshot("0x2", false),
        snapshot("0x3", false),
    ]);

    assert_eq!(matches, [Some((0, 0)), None, None]);
}

// REQ-SRC-08: scroll panes are found in visible nested accessibility trees.
#[test]
fn req_src_08_scroll_targets_use_visible_pane_bounds() {
    let pane =
        |role: &str, name: &str, states: &[&str], bounds: [i32; 4], children: Vec<Node>| Node {
            role: role.into(),
            name: name.into(),
            states: states.iter().map(|state| (*state).into()).collect(),
            extents: Some(bounds),
            children,
            ..Node::default()
        };
    let frame = pane(
        "frame",
        "",
        &["showing", "visible"],
        [0, 0, 400, 300],
        vec![
            pane(
                "scroll pane",
                "Files",
                &["showing", "visible"],
                [10, 20, 180, 200],
                vec![pane(
                    "viewport",
                    "Files viewport",
                    &["showing", "visible"],
                    [10, 20, 180, 200],
                    vec![],
                )],
            ),
            pane(
                "scroll pane",
                "Details",
                &["showing", "visible"],
                [220, 30, 150, 180],
                vec![],
            ),
            pane(
                "scroll pane",
                "Hidden",
                &["visible"],
                [10, 230, 180, 50],
                vec![],
            ),
            pane(
                "scroll pane",
                "Outside",
                &["showing", "visible"],
                [410, 0, 40, 40],
                vec![],
            ),
        ],
    );
    let targets = tree::scroll_targets(&frame, &window(), &monitor());
    assert_eq!(targets.len(), 2, "hidden and off-window panes are dropped");
    assert_eq!(targets[0].bounds, Rect::new(110, 220, 180, 200));
    assert_eq!(targets[0].text, "Files");
    assert_eq!(targets[1].bounds, Rect::new(320, 230, 150, 180));
    assert!(targets.iter().all(|target| target.role == Role::ScrollArea));
}

// REQ-SRC-08: a web document remains a scroll target without a scroll pane.
#[test]
fn req_src_08_web_document_without_scroll_pane_is_a_scroll_target() {
    let frame = Node {
        role: "frame".into(),
        states: vec!["showing".into(), "visible".into()],
        extents: Some([0, 0, 400, 300]),
        children: vec![Node {
            role: "document web".into(),
            name: "Article".into(),
            states: vec!["showing".into(), "visible".into()],
            extents: Some([0, 0, 400, 300]),
            ..Node::default()
        }],
        ..Node::default()
    };

    let targets = tree::scroll_targets(&frame, &window(), &monitor());
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].text, "Article");
    assert_eq!(targets[0].bounds, Rect::new(100, 200, 400, 300));
    assert_eq!(targets[0].actions, [thodu_core::model::ActionKind::Scroll]);
}

// REQ-SESSION-07: scroll mode cycles and selects numeric areas.
#[test]
fn req_session_07_scroll_keys_select_areas() {
    let targets: Vec<_> = (0..12)
        .map(|i| target(&format!("area {i}"), Rect::new(0, i * 30, 100, 24)))
        .collect();
    let mut session = Session::new_scroll(targets);
    assert!(session.scroll_mode);
    assert_eq!(session.focused, Some(0));
    assert_eq!(session.labels[0], "1");
    assert_eq!(session.labels[9], "10");
    session.key(Key::Digit { digit: 1 });
    session.key(Key::Digit { digit: 2 });
    assert_eq!(session.focused, Some(11));
    session.key(Key::Backspace);
    assert_eq!(session.focused, Some(0));
    session.key(Key::Return {
        shift: false,
        ctrl: false,
    });
    assert!(session.scroll_number.is_empty());
    session.key(Key::Tab { shift: false });
    assert_eq!(session.focused, Some(1));
    session.key(Key::Tab { shift: true });
    assert_eq!(session.focused, Some(0));
    session.key(Key::Escape);
    assert_eq!(session.state, State::Closed);
}

// REQ-SESSION-07: an invalid multi-digit number falls back to the latest
// single digit so every existing one-digit label remains directly selectable.
#[test]
fn req_session_07_invalid_scroll_number_selects_the_latest_valid_digit() {
    let targets = vec![
        target("area 1", Rect::new(0, 0, 100, 24)),
        target("area 2", Rect::new(0, 30, 100, 24)),
    ];
    let mut session = Session::new_scroll(targets);
    session.key(Key::Digit { digit: 1 });
    session.key(Key::Digit { digit: 2 });
    assert_eq!(session.focused, Some(1));
}

// REQ-ACT-04: hjkl select a signed wheel direction and Shift scales the step.
#[test]
fn req_act_04_scroll_keys_make_wheel_actions_without_clicks() {
    let mut session = Session::new_scroll(vec![target("pane", Rect::new(10, 20, 80, 60))]);
    for (key, axis, value) in [
        ('h', ScrollAxis::Horizontal, -1),
        ('j', ScrollAxis::Vertical, -1),
        ('k', ScrollAxis::Vertical, 1),
        ('l', ScrollAxis::Horizontal, 1),
    ] {
        session.key(Key::Char {
            c: key,
            shift: false,
        });
        let action = session.take_scroll_action().expect("one scroll action");
        assert_eq!(action.axis, axis);
        assert_eq!(action.value, value);
        session.resume_after_scroll();
    }
    session.key(Key::Char {
        c: 'j',
        shift: true,
    });
    let action = session.take_scroll_action().expect("shift scroll action");
    assert_eq!(action.axis, ScrollAxis::Vertical);
    assert_eq!(action.value, -5);
    assert_eq!(action.target, 0);
    assert!(session.act.is_none(), "scroll mode never creates a click");
}

// REQ-ACT-05: configured normal and Shift steps control the detent count in
// every direction, including a Shift step smaller than the normal step.
#[test]
fn req_act_05_session_uses_configured_steps_in_all_directions() {
    let settings = config::parse("scroll_step = 4\nscroll_shift_step = 2\n").expect("settings");
    let mut session =
        Session::new_scroll(vec![target("pane", Rect::new(10, 20, 80, 60))]).with_config(&settings);
    for (key, axis, value, shift) in [
        ('h', ScrollAxis::Horizontal, -4, false),
        ('j', ScrollAxis::Vertical, -4, false),
        ('k', ScrollAxis::Vertical, 4, false),
        ('l', ScrollAxis::Horizontal, 4, false),
        ('h', ScrollAxis::Horizontal, -2, true),
        ('j', ScrollAxis::Vertical, -2, true),
        ('k', ScrollAxis::Vertical, 2, true),
        ('l', ScrollAxis::Horizontal, 2, true),
    ] {
        session.key(Key::Char { c: key, shift });
        let action = session.take_scroll_action().expect("one wheel action");
        assert_eq!(action.axis, axis);
        assert_eq!(action.value, value);
        session.resume_after_scroll();
    }
}

// REQ-ACT-04: a key received while the wheel action is pending cannot close
// the scroll session or queue a second action.
#[test]
fn req_act_04_pending_scroll_ignores_additional_keys_until_resume() {
    let mut session = Session::new_scroll(vec![target("pane", Rect::new(10, 20, 80, 60))]);
    session.key(Key::Char {
        c: 'j',
        shift: false,
    });
    assert_eq!(session.state, State::Acting);
    assert!(session.take_scroll_action().is_some());

    session.key(Key::Char {
        c: 'j',
        shift: false,
    });
    assert_eq!(session.state, State::Acting);
    assert!(session.take_scroll_action().is_none());

    session.resume_after_scroll();
    assert_eq!(session.state, State::Showing);
    session.key(Key::Char {
        c: 'j',
        shift: false,
    });
    assert_eq!(session.state, State::Acting);
    assert!(session.take_scroll_action().is_some());
}

// REQ-ACT-06: Escape cancels a scroll action that has not passed through yet.
#[test]
fn req_act_06_escape_closes_scroll_mode_while_an_action_is_waiting() {
    let mut session = Session::new_scroll(vec![target("pane", Rect::new(10, 20, 80, 60))]);
    session.key(Key::Char {
        c: 'j',
        shift: false,
    });
    assert_eq!(session.state, State::Acting);
    assert!(session.pending_scroll_action().is_some());

    session.key(Key::Escape);
    assert_eq!(session.state, State::Closed);
    assert!(session.pending_scroll_action().is_none());
}

// REQ-SRC-07: IPC calls without a scope request every visible window.
#[test]
fn req_src_07_ipc_scope_defaults_to_visible() {
    for request in [
        serde_json::json!({"v": 1, "op": "start", "mode": "hints"}),
        serde_json::json!({"v": 1, "op": "targets"}),
    ] {
        let envelope: Envelope = serde_json::from_value(request).expect("request");
        match envelope.request {
            Request::Start { scope, .. } | Request::Targets { scope, .. } => {
                assert_eq!(scope, "visible");
            }
            _ => panic!("expected a target scope request"),
        }
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

// REQ-SRC-04: a later popup owning the click point makes a partly covered target unsafe.
#[test]
fn req_src_04_target_center_covered_by_popup_is_dropped() {
    let shown = || vec!["showing".into(), "visible".into()];
    let frame = Node {
        role: "frame".into(),
        states: shown(),
        extents: Some([0, 0, 400, 300]),
        children: vec![
            Node {
                role: "button".into(),
                states: shown(),
                name: "Behind".into(),
                extents: Some([20, 20, 100, 30]),
                ..Node::default()
            },
            Node {
                role: "button".into(),
                states: shown(),
                name: "Uncovered".into(),
                extents: Some([200, 20, 100, 30]),
                ..Node::default()
            },
            Node {
                role: "button".into(),
                states: shown(),
                name: "Edge".into(),
                extents: Some([20, 50, 100, 30]),
                ..Node::default()
            },
            Node {
                role: "popup menu".into(),
                states: shown(),
                extents: Some([60, 10, 80, 50]),
                children: vec![Node {
                    role: "menu item".into(),
                    states: shown(),
                    name: "Delete".into(),
                    extents: Some([70, 20, 40, 30]),
                    ..Node::default()
                }],
                ..Node::default()
            },
        ],
        ..Node::default()
    };

    let targets = tree::targets(&frame, &window(), &monitor());
    let texts: Vec<_> = targets.iter().map(|target| target.text.as_str()).collect();
    assert!(!texts.contains(&"Behind"));
    assert!(texts.contains(&"Uncovered"));
    assert!(texts.contains(&"Edge"));
    assert!(texts.contains(&"Delete"));
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

// REQ-LABEL-01: image and AT-SPI targets share the same top-left reading order.
#[test]
fn req_label_01_image_targets_sort_by_position_with_accessible_targets() {
    let mut accessible_below = target("accessible", Rect::new(100, 500, 80, 30));
    let image_above = Target {
        source: Source::Image,
        ..target("", Rect::new(100, 50, 80, 30))
    };
    let mut targets = vec![accessible_below.clone(), image_above.clone()];

    labels::reading_order(&mut targets);

    assert_eq!(targets[0].source, Source::Image);
    assert_eq!(targets[0].bounds, image_above.bounds);
    assert_eq!(targets[1].source, Source::Atspi);
    assert_eq!(targets[1].bounds, accessible_below.bounds);

    accessible_below.bounds = Rect::new(200, 50, 80, 30);
    targets = vec![accessible_below, image_above];
    labels::reading_order(&mut targets);
    assert_eq!(targets[0].bounds.x, 100);
    assert_eq!(targets[1].bounds.x, 200);
}

// REQ-SRC-06 and REQ-SESSION-06: image targets arriving after first draw do not relabel
// primary targets whose hints the user has already seen.
#[test]
fn req_session_06_fallback_keeps_displayed_labels() {
    let mut first = target("First", Rect::new(0, 100, 40, 20));
    first.id = "first".into();
    let mut second = target("Second", Rect::new(0, 300, 40, 20));
    second.id = "second".into();
    let mut session =
        Session::new(vec![first.clone(), second.clone()], true).reserving_label_prefix('z');
    let first_label = session.labels[session
        .targets
        .iter()
        .position(|item| item.id == "first")
        .unwrap()]
    .clone();
    let second_label = session.labels[session
        .targets
        .iter()
        .position(|item| item.id == "second")
        .unwrap()]
    .clone();

    let mut visual = target("", Rect::new(0, 200, 40, 20));
    visual.id = "visual".into();
    visual.source = Source::Image;
    session.merge_targets_preserving_labels(vec![first, visual, second]);

    let label_for = |id: &str| {
        let index = session
            .targets
            .iter()
            .position(|item| item.id == id)
            .unwrap();
        &session.labels[index]
    };
    assert_eq!(label_for("first"), &first_label);
    assert_eq!(label_for("second"), &second_label);
    assert!(label_for("visual").starts_with('z'));
    for (i, label) in session.labels.iter().enumerate() {
        assert!(
            !session
                .labels
                .iter()
                .enumerate()
                .any(|(j, other)| { i != j && other.starts_with(label) })
        );
    }

    let visual_label = label_for("visual").clone();
    for (i, c) in visual_label.chars().enumerate() {
        let action = session.key(Key::Char { c, shift: false });
        if i + 1 == visual_label.len() {
            assert_eq!(action.map(|act| act.target), Some(1));
        }
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

// REQ-SEARCH-01: Shift types a label; uppercase completion selects it.
#[test]
fn req_search_01_shift_types_labels_and_tab_moves() {
    let targets: Vec<Target> = (0..30)
        .map(|i| target(&format!("t{i}"), Rect::new(0, i * 40, 100, 30)))
        .collect();
    let mut s = Session::new(targets, false);
    let label = s.labels[29].clone();
    for c in label.chars() {
        assert!(
            s.key(Key::Upper {
                c: c.to_ascii_uppercase(),
            })
            .is_none()
        );
    }
    assert_eq!(s.focused, Some(29));
    assert!(s.search.is_empty());
    assert_eq!(s.state, State::Showing);
    assert!(s.label_selected);
    s.key(Key::Tab { shift: false });
    assert_eq!(s.focused, Some(0));
    s.key(Key::Tab { shift: true });
    assert_eq!(s.focused, Some(29));
}

// REQ-SEARCH-01: a label prefix continues in search-first mode.
#[test]
fn req_search_01_lowercase_continues_a_shifted_label_prefix() {
    let targets: Vec<Target> = (0..30)
        .map(|i| target(&format!("t{i}"), Rect::new(0, i * 40, 100, 30)))
        .collect();
    let mut s = Session::new(targets, false);
    let label = s
        .labels
        .iter()
        .find(|label| label.len() == 2)
        .expect("a two-letter label")
        .clone();
    let first = label.chars().next().expect("first label letter");
    s.key(Key::Upper {
        c: first.to_ascii_uppercase(),
    });
    assert_eq!(s.search, "");
    assert_eq!(s.label_prefix, first.to_string());
    assert_eq!(s.focused, None);
    let last = label.chars().nth(1).expect("second label letter");
    let act = s
        .key(Key::Char {
            c: last,
            shift: false,
        })
        .expect("a lowercase final character clicks");
    assert_eq!(Some(act.target), s.labels.iter().position(|l| l == &label));
    assert!(
        !act.linger,
        "a label click does not wait for a second Return"
    );
    assert_eq!(s.state, State::Acting);
}

// REQ-SEARCH-01: a label that is not visible in the current search cannot act.
#[test]
fn req_search_01_hidden_label_does_not_activate_or_steal_focus() {
    let mut s = Session::new(
        vec![
            target("Open", Rect::new(0, 0, 100, 30)),
            target("Save", Rect::new(0, 40, 100, 30)),
            target("Close", Rect::new(0, 80, 100, 30)),
        ],
        false,
    );
    s.key(Key::Char {
        c: 'o',
        shift: false,
    });
    assert_eq!(s.focused, Some(0));
    assert!(s.visible().contains(&0));
    assert!(!s.visible().contains(&1));

    assert!(s.key(Key::Upper { c: 'S' }).is_none());
    assert_eq!(s.focused, Some(0));
    assert_eq!(s.state, State::Showing);
    assert!(s.label_prefix.is_empty());
}

// REQ-SEARCH-01: an invalid label prefix keeps the current search focus.
#[test]
fn req_search_01_invalid_label_prefix_preserves_search_focus() {
    let mut s = Session::new(
        vec![
            target("Open", Rect::new(0, 0, 100, 30)),
            target("Save", Rect::new(0, 40, 100, 30)),
            target("Close", Rect::new(0, 80, 100, 30)),
        ],
        false,
    );
    s.key(Key::Char {
        c: 'o',
        shift: false,
    });
    let focused = s.focused;
    assert!(focused.is_some());
    assert!(s.key(Key::Upper { c: 'Z' }).is_none());
    assert_eq!(s.focused, focused);
    assert!(s.label_prefix.is_empty());

    assert!(s.key(Key::Space).is_none());
    assert!(s.grid.is_empty(), "Space does not discard the search focus");
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

// REQ-ACT-01: lowercase completion clicks at once; uppercase completion
// only selects, so no click is sent while Shift is held.
#[test]
fn req_act_01_lowercase_completion_clicks_but_uppercase_only_selects() {
    let targets: Vec<Target> = (0..30)
        .map(|i| target(&format!("t{i}"), Rect::new(0, i * 40, 100, 30)))
        .collect();
    let mut clicked = Session::new(targets.clone(), true);
    let index = clicked
        .labels
        .iter()
        .position(|label| label.len() == 2)
        .expect("a two-letter label");
    let label = clicked.labels[index].clone();
    let mut action = None;
    for c in label.chars() {
        action = clicked.key(Key::Char { c, shift: false });
    }
    let action = action.expect("lowercase completion clicks");
    assert_eq!(action.target, index);
    assert_eq!(action.button, Button::Left);
    assert!(!action.linger, "a label click ends after one click");
    assert_eq!(clicked.state, State::Acting);

    let mut selected = Session::new(targets, true);
    for (position, c) in label.chars().enumerate() {
        let key = if position + 1 == label.len() {
            Key::Upper {
                c: c.to_ascii_uppercase(),
            }
        } else {
            Key::Char { c, shift: false }
        };
        assert!(
            selected.key(key).is_none(),
            "uppercase completion only selects"
        );
    }
    assert_eq!(selected.focused, Some(index));
    assert!(selected.label_selected);
    assert_eq!(selected.state, State::Showing);
    let act = selected
        .key(Key::Return {
            shift: true,
            ctrl: false,
        })
        .expect("Shift-Return acts on the selected target");
    assert_eq!((act.target, act.button), (index, Button::Right));
}

#[test]
fn req_act_01_single_letter_d_clicks_and_uppercase_d_selects() {
    let targets: Vec<Target> = (0..20)
        .map(|i| target(&format!("t{i}"), Rect::new(0, i * 40, 100, 30)))
        .collect();
    let mut clicked = Session::new(targets.clone(), true);
    assert_eq!(clicked.labels[2], "d");
    let action = clicked
        .key(Key::Char {
            c: 'd',
            shift: false,
        })
        .expect("d clicks its single-letter target");
    assert_eq!(action.target, 2);
    assert!(!action.linger, "d does not start a double-click wait");

    let mut cancelled = Session::new(targets.clone(), true);
    assert!(cancelled.key(Key::Upper { c: 'D' }).is_none());
    assert!(
        cancelled.key(Key::Escape).is_none(),
        "Escape after a Shift selection clicks nothing"
    );
    assert_eq!(cancelled.state, State::Closed);

    let mut selected = Session::new(targets, true);
    assert!(selected.key(Key::Upper { c: 'D' }).is_none());
    assert_eq!(selected.focused, Some(2));
    let action = selected
        .key(Key::Return {
            shift: false,
            ctrl: false,
        })
        .expect("Return clicks the selected d target");
    assert_eq!(action.target, 2);
    assert!(action.linger, "Return keeps its double-click wait");
}

// REQ-SESSION-01: a new label prefix clears the previous target focus.
#[test]
fn req_session_01_new_label_prefix_clears_previous_focus() {
    let targets: Vec<Target> = (0..30)
        .map(|i| target(&format!("t{i}"), Rect::new(0, i * 40, 100, 30)))
        .collect();
    let mut s = Session::new(targets, true);
    assert!(s.key(Key::Upper { c: 'D' }).is_none());
    assert_eq!(s.focused, s.labels.iter().position(|label| label == "d"));
    assert!(s.label_selected);

    assert!(s.key(Key::Upper { c: 'A' }).is_none());
    assert_eq!(s.label_prefix, "a");
    assert_eq!(s.focused, None);
    assert!(!s.label_selected);

    assert!(s.key(Key::Backspace).is_none());
    assert_eq!(s.label_prefix, "");
    assert_eq!(s.focused, s.labels.iter().position(|label| label == "d"));
    assert!(
        s.label_selected,
        "Backspace restores the focus that preceded the prefix"
    );
}

// REQ-SESSION-01: focusing a fine grid cell keeps its bounds outline.
#[test]
fn req_session_01_fine_grid_cell_is_not_a_label_selected_target() {
    let mut s = Session::new(Vec::new(), false).with_area(Rect::new(0, 0, 10, 8));
    s.open_grid();
    let label = s.labels[0].clone();
    for c in label.chars() {
        assert!(
            s.key(Key::Upper {
                c: c.to_ascii_uppercase()
            })
            .is_none()
        );
    }
    assert_eq!(s.focused, Some(0));
    assert!(!s.label_selected);
}

// REQ-SEARCH-02: slash cancels an incomplete prefix before search begins.
#[test]
fn req_search_02_slash_clears_a_pending_label_prefix() {
    let targets: Vec<Target> = (0..30)
        .map(|i| target(&format!("item{i}"), Rect::new(0, i * 40, 100, 30)))
        .collect();
    let mut s = Session::new(targets, true);
    assert!(
        s.key(Key::Char {
            c: 'a',
            shift: false
        })
        .is_none()
    );
    assert_eq!(s.label_prefix, "a");

    assert!(
        s.key(Key::Char {
            c: '/',
            shift: false
        })
        .is_none()
    );
    assert!(s.searching);
    assert!(s.label_prefix.is_empty());
    assert!(
        s.key(Key::Char {
            c: 's',
            shift: false
        })
        .is_none()
    );
    assert_eq!(s.search, "s");
    assert!(s.label_prefix.is_empty());
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

// REQ-ACT-03: image targets are window-scoped; bar and grid targets are not.
#[test]
fn req_act_03_image_targets_are_stale_when_their_window_moves() {
    let collected = window();
    let moved = Window {
        at: (110, 200),
        ..collected.clone()
    };
    let image = Target {
        source: Source::Image,
        ..target("", Rect::new(100, 220, 80, 30))
    };
    assert!(target_is_stale(&image, &collected, Some(&moved)));
    assert!(target_is_stale(&image, &collected, None));

    let shell = Target {
        source: Source::Shell,
        window: String::new(),
        ..image.clone()
    };
    let grid = Target {
        source: Source::Grid,
        window: String::new(),
        ..image
    };
    assert!(!target_is_stale(&shell, &collected, Some(&moved)));
    assert!(!target_is_stale(&grid, &collected, Some(&moved)));
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
        parse(r#"{"v":1,"op":"key","session":"s1","generation":2,"key":"s","shift":true}"#),
        Request::Key {
            session: "s1".into(),
            generation: 2,
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
            session: "s1".into(),
            overlays: None,
        }
    );
    assert_eq!(
        serde_json::from_str::<Envelope>(
            r#"{"v":1,"op":"drawn","session":"s1","overlays":[{"x":10,"y":20,"w":30,"h":40}]}"#
        )
        .expect("drawn geometry parses")
        .request,
        Request::Drawn {
            session: "s1".into(),
            overlays: Some(vec![Rect::new(10, 20, 30, 40)]),
        }
    );
}

// REQ-SEARCH-02: with labels_only, the default, letters type labels and /
// starts a search; without it, letters search. A key "?label:N" types the
// whole label of target N, without Shift.
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
                    if c.is_uppercase() {
                        Key::Upper { c }
                    } else {
                        Key::Char { c, shift: false }
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
        if let Some(state) = case["state"].as_str() {
            assert_eq!(
                format!("{:?}", s.state).to_lowercase(),
                state,
                "{name} state"
            );
        }
    }
}

// REQ-SEARCH-02: the setting comes from config.toml, on unless set, and a
// typo is an error instead of a silent default.
#[test]
fn req_search_02_setting() {
    assert_eq!(config::parse(""), Ok(Config::default()));
    assert!(Config::default().labels_only, "labels-only is the default");
    assert_eq!(
        config::parse("labels_only = false\n"),
        Ok(Config {
            labels_only: false,
            ..Config::default()
        })
    );
    assert!(config::parse("label_only = true").is_err(), "unknown key");
    assert!(
        config::parse("labels_only = \"yes\"").is_err(),
        "wrong type"
    );
}

// REQ-ACT-05: scroll steps default to one and five detents and can be
// configured independently within bounded values.
#[test]
fn req_act_05_scroll_steps_are_configurable_and_bounded() {
    let defaults = Config::default();
    assert_eq!(defaults.scroll_step, 1);
    assert_eq!(defaults.scroll_shift_step, 5);

    let custom = config::parse("scroll_step = 4\nscroll_shift_step = 2\n").expect("valid steps");
    assert_eq!(custom.scroll_step, 4);
    assert_eq!(
        custom.scroll_shift_step, 2,
        "Shift may be slower than normal"
    );

    assert!(config::parse("scroll_step = 0").is_err());
    assert!(config::parse("scroll_shift_step = 101").is_err());
}

// REQ-SESSION-08: status detail accepts the three documented display modes.
#[test]
fn req_session_08_status_display_config_defaults_and_parses() {
    assert_eq!(
        Config::default().status_display,
        thodu_core::config::StatusDisplay::Auto
    );
    for mode in ["auto", "always", "off"] {
        let config = config::parse(&format!("status_display = \"{mode}\"\n")).expect("mode");
        assert_eq!(format!("{:?}", config.status_display).to_lowercase(), mode);
    }
    assert!(config::parse("status_display = \"sometimes\"").is_err());
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
            // The final character typed with Shift selects a fine cell
            // without clicking it, so Return clicks it below; a coarse cell
            // refines either way.
            let label = s.labels[i.expect("a cell")].clone();
            let last = label.len() - 1;
            for (position, c) in label.chars().enumerate() {
                if position == last {
                    s.key(Key::Upper { c: c.to_ascii_uppercase() });
                } else {
                    s.key(Key::Char { c, shift: false });
                }
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
        s.key(Key::Upper {
            c: c.to_ascii_uppercase(),
        });
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

// REQ-SRC-02: GTK4 file rows represent the selectable item; their grid-cell
// children must not become separate targets for the same row action.
#[test]
fn req_src_02_gtk4_file_rows_are_targets() {
    let targets = replayed("nautilus-reference");
    let mut found = summary(&targets);
    let mut marked_targets = expected("nautilus-reference");
    let order = |targets: &mut Vec<(Rect, Role, String)>| {
        targets.sort_by_key(|(bounds, _, text)| {
            (bounds.y, bounds.x, bounds.w, bounds.h, text.clone())
        });
    };
    order(&mut found);
    order(&mut marked_targets);
    assert_eq!(
        found, marked_targets,
        "GTK4 rows and their independent buttons are targets, but cells are not"
    );
    let marks = jsonl("atspi/nautilus-reference.marked.jsonl");
    assert_eq!(
        targets.len(),
        marks.len(),
        "all marked rows and nested buttons have targets"
    );
    assert_eq!(
        targets
            .iter()
            .filter(|target| target.role == Role::ListItem)
            .count(),
        7,
        "one list-item target per visible row"
    );
    for mark in marks {
        let x = mark["at"][0].as_i64().expect("x") as i32;
        let y = mark["at"][1].as_i64().expect("y") as i32;
        assert!(
            targets
                .iter()
                .any(|target| target.bounds.contains_point(x, y)),
            "the marked file row at ({x}, {y}) has a target"
        );
    }
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
                assert!(v["window"].is_string(), "target window address is present");
                assert!(v["actions"].is_array(), "target actions are present");
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
        let expected = match name {
            "an unregistered Chromium" => {
                "chromium is not on the accessibility bus: it starts its accessibility tree only with ACCESSIBILITY_ENABLED=1 in its environment; thodu install sets it, then restart the app"
            }
            "an unregistered Electron app" => {
                "md.obsidian.Obsidian is not on the accessibility bus: it starts its accessibility tree only with ACCESSIBILITY_ENABLED=1 in its environment; thodu install sets it, then restart the app"
            }
            "an unregistered GTK3 app" => {
                "org.gnome.Evince is not on the accessibility bus: GTK registers unless NO_AT_BRIDGE=1 or GTK_A11Y=none is in its environment; remove it and restart the app"
            }
            "an unregistered Qt app" => {
                "org.kde.okular is not on the accessibility bus: Qt registers only with QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1 in its environment; thodu install sets it, then restart the app"
            }
            "an app with no toolkit the bus knows" => {
                "dev.zed.Zed is not on the accessibility bus: it does not use the accessibility bus; Space opens the grid"
            }
            _ => panic!("unexpected case {name}"),
        };
        assert_eq!(message, expected, "{name}");
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
        "/usr/lib/slack/slack (deleted)",
        "/usr/lib/obsidian/obsidian",
        "/usr/lib/discord/Discord",
        "/usr/lib/code/code",
        "/opt/1Password/1password",
    ] {
        assert_eq!(unregistered::toolkit(exe, &gtk_lib), "Electron", "{exe}");
        assert!(
            unregistered::message("electron-app", exe, &gtk_lib)
                .contains("ACCESSIBILITY_ENABLED=1"),
            "{exe}"
        );
    }
    assert_eq!(
        unregistered::inspection_failed("org.example.App"),
        "org.example.App is not on the accessibility bus: could not inspect its process; Space opens the grid"
    );
}

fn stack_window(address: &str, at: (i32, i32), size: (i32, i32)) -> Window {
    Window {
        address: address.into(),
        at,
        size,
        workspace: 1,
        ..Window::default()
    }
}

fn layer(namespace: &str, level: u8, rect: Rect) -> Layer {
    Layer {
        namespace: namespace.into(),
        level,
        rect,
    }
}

// REQ-SRC-07: layers own the points where they take input. Omarchy's
// full-screen notification surface takes only its toast column; the OSD and
// Thodu's own overlay take none; the bar takes its strip; a bottom layer
// sits under the windows.
#[test]
fn req_src_07_layers_own_the_points_where_they_take_input() {
    let app = stack_window("0xapp", (0, 26), (1536, 934));
    let mut screen = monitor();
    let full = Rect::new(0, 0, 1536, 960);
    screen.layers = vec![
        layer("omarchy-notifications", 3, full),
        layer("omarchy-osd", 3, full),
        layer("thodu", 3, full),
        layer("omarchy-bar", 2, Rect::new(0, 0, 1536, 26)),
        layer("other-panel", 2, Rect::new(600, 300, 200, 200)),
        layer("wallpaper-widget", 1, Rect::new(0, 600, 400, 300)),
    ];
    let windows = [app];
    let at = |x, y| owner_at(&windows, &screen, x, y);
    assert_eq!(at(1500, 100), Owner::Layer("omarchy-notifications".into()));
    assert_eq!(
        at(500, 100),
        Owner::Window("0xapp".into()),
        "outside the toast column"
    );
    assert_eq!(at(500, 10), Owner::Layer("omarchy-bar".into()));
    assert_eq!(at(700, 400), Owner::Layer("other-panel".into()));
    assert_eq!(
        at(100, 700),
        Owner::Window("0xapp".into()),
        "a bottom layer is under the windows"
    );
}

// REQ-SRC-07: a fullscreen window covers the bar and the windows behind it;
// a window Hyprland allows over fullscreen stays above it, and overlay
// layers stay above everything.
#[test]
fn req_src_07_fullscreen_window_covers_bar_and_windows_behind_it() {
    let tiled = stack_window("0xtiled", (0, 26), (768, 934));
    let mut floating = stack_window("0xfloating", (100, 100), (300, 300));
    floating.floating = true;
    let mut fullscreen = stack_window("0xfull", (0, 0), (1536, 960));
    fullscreen.fullscreen = 2;
    let mut dialog = stack_window("0xdialog", (600, 400), (300, 200));
    dialog.floating = true;
    dialog.allowed_over_fullscreen = true;
    let mut screen = monitor();
    screen.layers = vec![
        layer("omarchy-bar", 2, Rect::new(0, 0, 1536, 26)),
        layer("omarchy-notifications", 3, Rect::new(0, 0, 1536, 960)),
    ];
    let windows = [
        tiled.clone(),
        floating.clone(),
        fullscreen.clone(),
        dialog.clone(),
    ];
    let at = |x, y| owner_at(&windows, &screen, x, y);
    assert_eq!(
        at(300, 10),
        Owner::Window("0xfull".into()),
        "the bar is under the fullscreen window"
    );
    assert_eq!(
        at(200, 200),
        Owner::Window("0xfull".into()),
        "a floating window behind it is hidden"
    );
    assert_eq!(at(700, 500), Owner::Window("0xdialog".into()));
    assert_eq!(at(1500, 100), Owner::Layer("omarchy-notifications".into()));
    let shown: Vec<_> = windows
        .iter()
        .filter(|window| shown_beside_fullscreen(window, &windows))
        .map(|window| window.address.as_str())
        .collect();
    assert_eq!(shown, ["0xfull", "0xdialog"]);
}

// REQ-SRC-07: a window that takes no pointer input owns no point.
#[test]
fn req_src_07_window_without_input_owns_nothing() {
    let below = stack_window("0xbelow", (0, 0), (500, 500));
    let mut glass = stack_window("0xglass", (0, 0), (500, 500));
    glass.floating = true;
    glass.ignores_input = true;
    let screen = monitor();
    assert_eq!(
        owner_at(&[glass.clone(), below], &screen, 100, 100),
        Owner::Window("0xbelow".into())
    );
    assert_eq!(owner_at(&[glass], &screen, 100, 100), Owner::Nothing);
}

fn live_button() -> thodu_core::tree::Node {
    thodu_core::tree::Node {
        role: "push button".into(),
        states: vec!["showing".into(), "visible".into()],
        name: "Send".into(),
        extents: Some([10, 20, 80, 30]),
        element: Some(thodu_core::model::ElementRef {
            bus: ":1.9".into(),
            path: "/a/1".into(),
        }),
        ..thodu_core::tree::Node::default()
    }
}

// REQ-ACT-03: an element read live carries the check to read it again
// before a click; the check holds for the same extents, within a unit, and
// fails when the element moved, hid or is gone.
#[test]
fn req_act_03_element_check_compares_extents_and_showing() {
    use thodu_core::tree::{Node, element_unchanged};
    let node = live_button();
    let check = node
        .check()
        .expect("a live element with extents has a check");
    assert_eq!(check.extents, [10, 20, 80, 30]);
    assert!(element_unchanged(&check, &node));
    let nudged = Node {
        extents: Some([11, 20, 80, 30]),
        ..node.clone()
    };
    assert!(element_unchanged(&check, &nudged), "within one unit");
    let moved = Node {
        extents: Some([10, 60, 80, 30]),
        ..node.clone()
    };
    assert!(!element_unchanged(&check, &moved));
    let hidden = Node {
        states: Vec::new(),
        ..node.clone()
    };
    assert!(!element_unchanged(&check, &hidden));
    assert!(
        !element_unchanged(&check, &Node::default()),
        "an element the bus no longer answers for"
    );
    let recorded = Node {
        element: None,
        ..node
    };
    assert!(
        recorded.check().is_none(),
        "recorded trees carry no element"
    );
}

// REQ-ACT-03: the targets of a tree read live carry their elements' checks.
#[test]
fn req_act_03_live_targets_carry_element_checks() {
    use thodu_core::tree::Node;
    let window = window();
    let frame = Node {
        role: "frame".into(),
        states: vec!["showing".into(), "visible".into(), "active".into()],
        extents: Some([0, 0, window.size.0, window.size.1]),
        children: vec![live_button()],
        ..Node::default()
    };
    let targets = tree::targets(&frame, &window, &monitor());
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].check, live_button().check());
}

// REQ-GRID-01: a lowercase label refines a coarse cell, and one that chooses
// a fine cell clicks its center at once, with no double-Return wait.
#[test]
fn req_grid_01_a_lowercase_label_on_a_fine_cell_clicks() {
    let mut s = Session::new(Vec::new(), false).with_area(Rect::new(0, 0, 1536, 864));
    s.open_grid();
    let type_label = |s: &mut Session, label: String| {
        let mut act = None;
        for c in label.chars() {
            act = s.key(Key::Char { c, shift: false });
        }
        act
    };
    let coarse = s.labels[0].clone();
    assert!(
        type_label(&mut s, coarse).is_none(),
        "a coarse cell refines"
    );
    assert_eq!(s.grid.len(), 2);
    assert!(s.focused.is_none());
    let fine = s.labels[0].clone();
    let cell = s.targets[0].bounds;
    assert!(cell.w <= grid::FINE && cell.h <= grid::FINE);
    let act = type_label(&mut s, fine).expect("a fine cell clicks");
    assert_eq!(act.point, cell.center());
    assert!(
        !act.linger,
        "a label click does not wait for a second Return"
    );
    assert_eq!(s.state, State::Acting);
}

// REQ-GRID-01: Return on a focused cell that is not yet fine refines into it
// instead of clicking a coarse cell's center.
#[test]
fn req_grid_01_return_on_a_coarse_cell_refines() {
    let mut s = Session::new(Vec::new(), false).with_area(Rect::new(0, 0, 1536, 864));
    s.open_grid();
    s.key(Key::Tab { shift: false });
    let coarse = s.targets[s.focused.expect("Tab focuses a cell")].bounds;
    assert!(coarse.w > grid::FINE);
    assert!(
        s.key(Key::Return {
            shift: false,
            ctrl: false
        })
        .is_none(),
        "no click on a coarse cell"
    );
    assert_eq!(
        s.grid.last(),
        Some(&coarse),
        "the coarse cell became the grid"
    );
    assert!(s.focused.is_none());
    assert_eq!(s.state, State::Showing);
}
