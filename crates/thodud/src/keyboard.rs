//! Control held around a click (REQ-ACT-02), through a uinput keyboard:
//! Hyprland reads it like any other keyboard, with GG's own keymap, so the
//! clicked app sees Control. Writing to /dev/uinput needs the uaccess rule
//! `thodu install` adds.

use std::thread;
use std::time::Duration;

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, KeyCode, KeyEvent};

/// How long the compositor takes to find a new input device.
const DEVICE_SETTLE: Duration = Duration::from_millis(300);

pub struct Keyboard {
    device: VirtualDevice,
}

fn denied(e: std::io::Error) -> String {
    format!("Control-click needs /dev/uinput ({e}); run thodu install")
}

impl Keyboard {
    pub fn open() -> Result<Self, String> {
        let mut keys = AttributeSet::<KeyCode>::new();
        keys.insert(KeyCode::KEY_LEFTCTRL);
        let device = VirtualDevice::builder()
            .map_err(denied)?
            .name("thodu")
            .with_keys(&keys)
            .map_err(denied)?
            .build()
            .map_err(denied)?;
        thread::sleep(DEVICE_SETTLE);
        Ok(Self { device })
    }

    fn emit_ctrl(&mut self, down: bool) -> Result<(), String> {
        self.device
            .emit(&[*KeyEvent::new(KeyCode::KEY_LEFTCTRL, i32::from(down))])
            .map_err(|e| format!("Control did not go through: {e}"))
    }
}

/// A keyboard that can press and release Control.
pub trait Control {
    fn ctrl(&mut self, down: bool) -> Result<(), String>;
}

impl Control for Keyboard {
    fn ctrl(&mut self, down: bool) -> Result<(), String> {
        self.emit_ctrl(down)
    }
}

/// Press or release Control on the keyboard in `slot`, opening one with
/// `open` for a press (REQ-ACT-02). Control can never stay down: a release
/// that fails twice destroys the device, and the kernel releases every key
/// a destroyed device holds, so no later key GG types becomes a Control
/// chord. The next Control-click opens a new device.
pub fn set_ctrl<K: Control>(
    slot: &mut Option<K>,
    down: bool,
    open: impl FnOnce() -> Result<K, String>,
) -> Result<(), String> {
    if down {
        if slot.is_none() {
            *slot = Some(open()?);
        }
        return slot.as_mut().ok_or("no keyboard")?.ctrl(true);
    }
    // No device holds no key.
    let Some(keyboard) = slot.as_mut() else {
        return Ok(());
    };
    let Err(first) = keyboard.ctrl(false) else {
        return Ok(());
    };
    let Err(second) = keyboard.ctrl(false) else {
        return Ok(());
    };
    *slot = None;
    eprintln!(
        "ERROR Control did not lift ({first}; then {second}); closed Thodu's Control keyboard, which lifts it; if this repeats, check /dev/uinput and run thodu install"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::{Control, set_ctrl};

    #[derive(Default)]
    struct Log {
        sent: Vec<bool>,
        dropped: bool,
    }

    struct Fake {
        failing_releases: usize,
        log: Rc<RefCell<Log>>,
    }

    impl Control for Fake {
        fn ctrl(&mut self, down: bool) -> Result<(), String> {
            self.log.borrow_mut().sent.push(down);
            if !down && self.failing_releases > 0 {
                self.failing_releases -= 1;
                return Err("busy".into());
            }
            Ok(())
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            self.log.borrow_mut().dropped = true;
        }
    }

    fn pressed(failing_releases: usize) -> (Option<Fake>, Rc<RefCell<Log>>) {
        let log = Rc::new(RefCell::new(Log::default()));
        let mut slot = None;
        let shared = Rc::clone(&log);
        set_ctrl(&mut slot, true, move || {
            Ok(Fake {
                failing_releases,
                log: shared,
            })
        })
        .expect("press");
        (slot, log)
    }

    // REQ-ACT-02: a Control release that fails twice still leaves Control
    // up, by destroying the device that holds it.
    #[test]
    fn req_act_02_a_release_that_fails_twice_destroys_the_keyboard() {
        let (mut slot, log) = pressed(usize::MAX);
        set_ctrl(&mut slot, false, || unreachable!()).expect("Control is up");
        assert!(slot.is_none(), "the device is gone");
        assert!(log.borrow().dropped, "the device was destroyed");
        assert_eq!(log.borrow().sent, [true, false, false]);
        // pointer.rs's cleanup releases again: nothing holds Control now.
        set_ctrl(&mut slot, false, || unreachable!()).expect("nothing to release");
    }

    // REQ-ACT-02: one failed release is retried on the same device, which
    // stays open for the next Control-click.
    #[test]
    fn req_act_02_a_release_that_fails_once_is_retried() {
        let (mut slot, log) = pressed(1);
        set_ctrl(&mut slot, false, || unreachable!()).expect("released");
        assert!(slot.is_some());
        assert!(!log.borrow().dropped);
        assert_eq!(log.borrow().sent, [true, false, false]);
    }
}
