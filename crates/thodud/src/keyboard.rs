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

    pub fn ctrl(&mut self, down: bool) -> Result<(), String> {
        self.device
            .emit(&[*KeyEvent::new(KeyCode::KEY_LEFTCTRL, i32::from(down))])
            .map_err(|e| format!("Control did not go through: {e}"))
    }
}
