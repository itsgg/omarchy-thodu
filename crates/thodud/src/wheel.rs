//! Wheel events through uinput, so applications receive the same pointer
//! input whether they use AT-SPI or browser accessibility (REQ-ACT-04).

use std::thread;
use std::time::Duration;

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, RelativeAxisCode};
use thodu_core::session::{ScrollAction, ScrollAxis};

const DEVICE_SETTLE: Duration = Duration::from_millis(300);
const HI_RES_PER_DETENT: i32 = 120;

pub struct Wheel {
    device: VirtualDevice,
}

impl Wheel {
    pub fn open() -> Result<Self, String> {
        let mut axes = AttributeSet::<RelativeAxisCode>::new();
        axes.insert(RelativeAxisCode::REL_WHEEL);
        axes.insert(RelativeAxisCode::REL_HWHEEL);
        axes.insert(RelativeAxisCode::REL_WHEEL_HI_RES);
        axes.insert(RelativeAxisCode::REL_HWHEEL_HI_RES);
        let device = VirtualDevice::builder()
            .map_err(|e| format!("scroll input needs /dev/uinput ({e}); run thodu install"))?
            .name("thodu wheel")
            .with_relative_axes(&axes)
            .map_err(|e| format!("scroll input needs /dev/uinput ({e}); run thodu install"))?
            .build()
            .map_err(|e| format!("scroll input needs /dev/uinput ({e}); run thodu install"))?;
        thread::sleep(DEVICE_SETTLE);
        Ok(Self { device })
    }

    pub fn emit(&mut self, action: ScrollAction) -> Result<(), String> {
        let (legacy, high_res) = event_codes(action.axis);
        let events = [
            InputEvent::new(EventType::RELATIVE.0, legacy.0, action.value),
            InputEvent::new(
                EventType::RELATIVE.0,
                high_res.0,
                action.value.saturating_mul(HI_RES_PER_DETENT),
            ),
        ];
        self.device
            .emit(&events)
            .map_err(|e| format!("scroll input failed: {e}"))
    }
}

fn event_codes(axis: ScrollAxis) -> (RelativeAxisCode, RelativeAxisCode) {
    match axis {
        ScrollAxis::Vertical => (
            RelativeAxisCode::REL_WHEEL,
            RelativeAxisCode::REL_WHEEL_HI_RES,
        ),
        ScrollAxis::Horizontal => (
            RelativeAxisCode::REL_HWHEEL,
            RelativeAxisCode::REL_HWHEEL_HI_RES,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::event_codes;
    use evdev::RelativeAxisCode;
    use thodu_core::session::ScrollAxis;

    // REQ-ACT-04: wheel axes map to both legacy and high-resolution events.
    #[test]
    fn req_act_04_wheel_axes_cover_vertical_and_horizontal_scrolling() {
        assert_eq!(
            event_codes(ScrollAxis::Vertical),
            (
                RelativeAxisCode::REL_WHEEL,
                RelativeAxisCode::REL_WHEEL_HI_RES
            )
        );
        assert_eq!(
            event_codes(ScrollAxis::Horizontal),
            (
                RelativeAxisCode::REL_HWHEEL,
                RelativeAxisCode::REL_HWHEEL_HI_RES
            )
        );
    }
}
