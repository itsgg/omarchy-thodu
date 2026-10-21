//! The overlay listening for sessions (SPEC section 7, `listen`).

use std::io::Write;

/// The newest overlay to say listen, which is the one sessions go to. One
/// shell runs at a time, so an older connection belongs to an overlay that
/// was reloaded or stopped; it is dropped, and so closed, when a newer one
/// arrives, and a connection that cannot take a line is dropped too.
pub struct Listeners<W> {
    current: Option<W>,
}

impl<W: Write> Listeners<W> {
    pub const fn new() -> Self {
        Self { current: None }
    }

    pub fn add(&mut self, stream: W) {
        self.current = Some(stream);
    }

    /// Write `line` to the listening overlay; false when none listens.
    pub fn push(&mut self, line: &[u8]) -> bool {
        let sent = self
            .current
            .as_mut()
            .is_some_and(|s| s.write_all(line).and_then(|()| s.flush()).is_ok());
        if !sent {
            self.current = None;
        }
        sent
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::io;
    use std::rc::Rc;

    use super::Listeners;

    struct Fake {
        works: bool,
        lines: Rc<Cell<usize>>,
        dropped: Rc<Cell<bool>>,
    }

    impl Fake {
        fn new(works: bool) -> (Self, Rc<Cell<usize>>, Rc<Cell<bool>>) {
            let lines = Rc::new(Cell::new(0));
            let dropped = Rc::new(Cell::new(false));
            let fake = Self {
                works,
                lines: Rc::clone(&lines),
                dropped: Rc::clone(&dropped),
            };
            (fake, lines, dropped)
        }
    }

    impl io::Write for Fake {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if !self.works {
                return Err(io::Error::from(io::ErrorKind::BrokenPipe));
            }
            self.lines.set(self.lines.get() + 1);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            self.dropped.set(true);
        }
    }

    // A reloaded overlay's old connection is closed when the new one
    // listens, and sessions go to the new one.
    #[test]
    fn a_newer_overlay_replaces_and_closes_the_older() {
        let mut listeners = Listeners::new();
        let (old, old_lines, old_dropped) = Fake::new(true);
        listeners.add(old);
        let (new, new_lines, _) = Fake::new(true);
        listeners.add(new);
        assert!(old_dropped.get(), "the older connection is closed");
        assert!(listeners.push(b"open\n"));
        assert_eq!((old_lines.get(), new_lines.get()), (0, 1));
    }

    // A connection that cannot take a line is closed, and the session
    // falls back to a summon.
    #[test]
    fn a_dead_overlay_is_dropped() {
        let mut listeners = Listeners::new();
        let (dead, _, dropped) = Fake::new(false);
        listeners.add(dead);
        assert!(!listeners.push(b"open\n"));
        assert!(dropped.get());
        assert!(!listeners.push(b"open\n"), "nothing listens now");
    }
}
