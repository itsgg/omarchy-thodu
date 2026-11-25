//! The overlay listening for sessions (SPEC section 7, `listen`).

use std::io::Write;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;

/// A connection that can be closed for every copy of it at once.
pub trait Hangup {
    fn hang_up(&self);
}

impl Hangup for UnixStream {
    /// The listener holds a copy of the overlay's socket while serve() reads
    /// from another, so dropping the copy alone closes nothing: shutting the
    /// socket down ends serve()'s read and shows the overlay an end of file.
    fn hang_up(&self) {
        let _ = self.shutdown(Shutdown::Both);
    }
}

/// The newest overlay to say listen, which is the one sessions go to. One
/// shell runs at a time, so an older connection belongs to an overlay that
/// was reloaded or stopped; it is hung up when a newer one arrives, and a
/// connection that cannot take a line is hung up too, so its overlay sees
/// the end of file and listens again.
pub struct Listeners<W> {
    current: Option<W>,
}

impl<W: Write + Hangup> Listeners<W> {
    pub const fn new() -> Self {
        Self { current: None }
    }

    pub fn add(&mut self, stream: W) {
        if let Some(old) = self.current.replace(stream) {
            old.hang_up();
        }
    }

    /// Write `line` to the listening overlay; false when none listens.
    pub fn push(&mut self, line: &[u8]) -> bool {
        let sent = self
            .current
            .as_mut()
            .is_some_and(|s| s.write_all(line).and_then(|()| s.flush()).is_ok());
        if !sent && let Some(dead) = self.current.take() {
            dead.hang_up();
        }
        sent
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::io;
    use std::rc::Rc;

    use super::{Hangup, Listeners};

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

    impl Hangup for Fake {
        fn hang_up(&self) {}
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

    // REQ-SESSION-03, REQ-PERF-01: a listener dropped after a failed push is
    // a copy of the socket serve() still reads from; the overlay must still
    // see the end of file, so it listens again instead of every later
    // session going through a slow summon.
    #[test]
    fn req_perf_01_a_dropped_listener_hangs_up_every_copy() {
        use std::io::Read;
        use std::os::unix::net::UnixStream;
        use std::time::Duration;

        let (mut overlay, daemon_side) = UnixStream::pair().unwrap();
        // serve() keeps reading from its own copy.
        let serving = daemon_side.try_clone().unwrap();
        daemon_side
            .set_write_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        let mut listeners = Listeners::new();
        listeners.add(daemon_side);
        // The overlay reads nothing, so a big push fills the socket and
        // times out.
        assert!(!listeners.push(&vec![b'x'; 4 << 20]));
        overlay
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut seen = Vec::new();
        overlay
            .read_to_end(&mut seen)
            .expect("the overlay sees the end of file, not a hang");
        drop(serving);
    }

    // REQ-PERF-01: an overlay replaced by a newer one sees the end of file
    // too, though serve() still holds a copy of its socket.
    #[test]
    fn req_perf_01_a_replaced_listener_hangs_up_every_copy() {
        use std::io::Read;
        use std::os::unix::net::UnixStream;
        use std::time::Duration;

        let (mut old, old_side) = UnixStream::pair().unwrap();
        let serving = old_side.try_clone().unwrap();
        let (_new, new_side) = UnixStream::pair().unwrap();
        let mut listeners = Listeners::new();
        listeners.add(old_side);
        listeners.add(new_side);
        old.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut seen = Vec::new();
        old.read_to_end(&mut seen)
            .expect("the replaced overlay sees the end of file");
        drop(serving);
    }
}
