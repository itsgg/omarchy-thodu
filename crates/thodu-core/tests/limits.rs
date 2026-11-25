//! Task 0039: the scroll prefix and its focus, the label limit for targets
//! that join a session late, and settings errors that say what to fix.

use std::collections::HashSet;

use thodu_core::config::{self, Config};
use thodu_core::labels::{self, BENEATH_CAPACITY};
use thodu_core::model::{Rect, Role, Source, Target};
use thodu_core::session::{Key, Session};

fn target(id: &str, bounds: Rect, source: Source) -> Target {
    Target {
        id: id.into(),
        bounds,
        role: Role::Button,
        text: id.into(),
        source,
        actions: Vec::new(),
        window: "0x1".into(),
        check: None,
    }
}

fn areas(n: i32) -> Vec<Target> {
    (0..n)
        .map(|i| {
            target(
                &format!("area {i}"),
                Rect::new(0, i * 30, 100, 24),
                Source::Atspi,
            )
        })
        .collect()
}

// REQ-SESSION-07: Backspace leaves the focus the shown prefix names. From
// "05" back to "0" the prefix means area 10, so area 10 is focused.
#[test]
fn req_session_07_backspace_focuses_the_area_its_prefix_names() {
    let mut session = Session::new_scroll(areas(12));
    session.key(Key::Digit { digit: 0 });
    assert_eq!(session.focused, Some(9), "0 selects area 10");
    session.key(Key::Digit { digit: 5 });
    assert_eq!(session.scroll_number, "05");
    assert_eq!(session.focused, Some(4), "05 selects area 5");
    session.key(Key::Backspace);
    assert_eq!(session.scroll_number, "0");
    assert_eq!(session.focused, Some(9), "the prefix 0 names area 10");
    session.key(Key::Backspace);
    assert!(session.scroll_number.is_empty());
    assert_eq!(
        session.focused,
        Some(0),
        "an empty prefix restores the focus"
    );
}

// REQ-SESSION-07: every prefix left by Backspace focuses what typing that
// prefix afresh would focus.
#[test]
fn req_session_07_backspace_matches_typing_the_shorter_prefix() {
    for count in [3, 12, 25] {
        for digits in ["05", "11", "123", "012", "20", "9", "250"] {
            let mut typed = Session::new_scroll(areas(count));
            typed.key(Key::Tab { shift: false });
            for d in digits.bytes() {
                typed.key(Key::Digit { digit: d - b'0' });
            }
            typed.key(Key::Backspace);
            let mut fresh = Session::new_scroll(areas(count));
            fresh.key(Key::Tab { shift: false });
            for d in typed.scroll_number.clone().bytes() {
                fresh.key(Key::Digit { digit: d - b'0' });
            }
            assert_eq!(
                (typed.focused, &typed.scroll_number),
                (fresh.focused, &fresh.scroll_number),
                "{count} areas, {digits} then Backspace"
            );
        }
    }
}

// REQ-LABEL-01: targets that join a session late, as image targets do, get
// labels of at most three letters beneath the reserved prefix; past that
// capacity the farthest are left out and counted, never given a fourth
// letter.
#[test]
fn req_label_01_late_targets_stay_within_three_letters() {
    let primary: Vec<Target> = (0..30)
        .map(|i| target(&format!("p{i}"), Rect::new(0, i * 10, 40, 8), Source::Atspi))
        .collect();
    let mut session = Session::new(primary.clone(), true).reserving_label_prefix('z');
    let shown: Vec<(String, String)> = session
        .targets
        .iter()
        .map(|t| t.id.clone())
        .zip(session.labels.clone())
        .collect();
    let late = BENEATH_CAPACITY + 24;
    let mut all = primary;
    all.extend((0..late).map(|i| {
        target(
            &format!("img{i}"),
            Rect::new(100, 1000 + i as i32, 10, 1),
            Source::Image,
        )
    }));
    let left_out = session.merge_targets_preserving_labels(all);
    assert_eq!(left_out, 24);
    assert_eq!(session.targets.len(), 30 + BENEATH_CAPACITY);
    assert_eq!(session.labels.len(), session.targets.len());
    assert!(
        session.labels.iter().all(|l| l.len() <= 3),
        "a label past three letters"
    );
    let distinct: HashSet<_> = session.labels.iter().collect();
    assert_eq!(distinct.len(), session.labels.len());
    for a in &session.labels {
        for b in &session.labels {
            assert!(
                a == b || !b.starts_with(a.as_str()),
                "{a} is a prefix of {b}"
            );
        }
    }
    for (id, label) in shown {
        let at = session
            .targets
            .iter()
            .position(|t| t.id == id)
            .expect("kept");
        assert_eq!(session.labels[at], label, "{id} keeps its label");
    }
    assert!(
        session.targets.iter().any(|t| t.id == "img0")
            && !session
                .targets
                .iter()
                .any(|t| t.id == format!("img{}", late - 1)),
        "the nearest late targets are kept"
    );
    assert_eq!(
        labels::assign_beneath(BENEATH_CAPACITY * 2, 'z').len(),
        BENEATH_CAPACITY
    );
}

// REQ-ACT-05: a settings error names the key and what it takes.
#[test]
fn req_act_05_settings_errors_name_their_key() {
    for (text, key) in [
        ("labels_only = \"yes\"\n", "labels_only"),
        ("status_display = 3\n", "status_display"),
        ("status_display = \"sometimes\"\n", "status_display"),
        ("scroll_step = \"two\"\n", "scroll_step"),
        ("scroll_shift_step = 1.5\n", "scroll_shift_step"),
        ("scroll_step = 0\n", "scroll_step"),
        ("scroll_shift_step = 70000\n", "scroll_shift_step"),
        ("label_only = true\n", "label_only"),
    ] {
        let err = config::parse(text).expect_err(text);
        assert!(err.contains(key), "{text}: {err}");
    }
    let err = config::parse("scroll_step = 2\nlabels_only = \n").expect_err("syntax");
    assert!(err.starts_with("line 2:"), "{err}");
    assert_eq!(
        config::parse("labels_only = false\nscroll_step = 3\n")
            .map(|c| (c.labels_only, c.scroll_step)),
        Ok((false, 3))
    );
}

// REQ-ACT-05: a settings file that cannot be read or is not UTF-8 gives the
// defaults and says why, as one that does not parse does; a missing file is
// the defaults with nothing to say.
#[test]
fn req_act_05_an_unreadable_settings_file_is_reported() {
    let dir = std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME is set"))
        .join(".local/share/thodu/tests")
        .join(format!("config-load-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test directory");
    let path = dir.join("config.toml");

    assert_eq!(config::load(&path), (Config::default(), None));

    std::fs::write(&path, b"scroll_step = 3\n# \xff\n").expect("write");
    let (settings, problem) = config::load(&path);
    assert_eq!(settings, Config::default());
    let problem = problem.expect("a file that is not UTF-8 is reported");
    assert!(problem.contains("not UTF-8"), "{problem}");
    assert!(problem.contains("using the defaults"), "{problem}");

    std::fs::remove_file(&path).expect("remove");
    std::fs::create_dir(&path).expect("a directory where the file should be");
    let (settings, problem) = config::load(&path);
    assert_eq!(settings, Config::default());
    assert!(
        problem
            .as_deref()
            .is_some_and(|p| p.starts_with("cannot read")),
        "{problem:?}"
    );

    std::fs::remove_dir(&path).expect("remove");
    std::fs::write(&path, "scroll_step = 4\n").expect("write");
    assert_eq!(config::load(&path).0.scroll_step, 4);
    let _ = std::fs::remove_dir_all(&dir);
}
