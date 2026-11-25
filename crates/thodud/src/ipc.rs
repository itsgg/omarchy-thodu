//! Reading a client's requests (SPEC section 7) within bounds: a client that
//! connects must speak, a line that starts must end, and no line may grow
//! without limit, so no client can hold a thread or memory forever.
//!
//! Once a client has sent a request it may stay quiet as long as it likes:
//! the overlay's request and listen connections wait between sessions.

use std::io::{BufRead, BufReader, ErrorKind};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

/// How a connection's reads are bounded.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// The longest a new connection may wait before its first byte.
    pub first: Duration,
    /// The longest one line may take from its first byte to its newline.
    pub line: Duration,
    /// The most bytes one line may hold, newline excluded.
    pub max: usize,
}

/// The bounds thodud serves with.
pub const LIMITS: Limits = Limits {
    first: Duration::from_secs(5),
    line: Duration::from_secs(5),
    max: 1 << 20,
};

/// What reading the next request found.
#[derive(Debug, PartialEq, Eq)]
pub enum Next {
    Line(String),
    /// The client closed the connection, or reading it failed.
    Closed,
    /// The client did not send its first request, or finish a line, in time.
    Silent,
    /// The line is longer than the limit.
    TooLong,
}

/// A client's requests, one line each.
pub struct Requests {
    reader: BufReader<UnixStream>,
    limits: Limits,
    spoken: bool,
}

impl Requests {
    pub fn new(stream: UnixStream, limits: Limits) -> Self {
        Self {
            reader: BufReader::new(stream),
            limits,
            spoken: false,
        }
    }

    /// The next line, without its newline.
    pub fn next_line(&mut self) -> Next {
        let mut line: Vec<u8> = Vec::new();
        let mut deadline = (!self.spoken).then(|| Instant::now() + self.limits.first);
        let mut started = false;
        loop {
            let wait = match deadline {
                None => None,
                Some(at) => {
                    let left = at.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Next::Silent;
                    }
                    Some(left)
                }
            };
            if self.reader.get_ref().set_read_timeout(wait).is_err() {
                return Next::Closed;
            }
            let chunk = match self.reader.fill_buf() {
                Ok(chunk) => chunk,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    return Next::Silent;
                }
                Err(_) => return Next::Closed,
            };
            if chunk.is_empty() {
                // A last line without a newline still counts, as with
                // BufRead::lines.
                if line.is_empty() {
                    return Next::Closed;
                }
                self.spoken = true;
                return Next::Line(text(line));
            }
            let newline = chunk.iter().position(|&b| b == b'\n');
            let take = newline.unwrap_or(chunk.len());
            if line.len() + take > self.limits.max {
                return Next::TooLong;
            }
            line.extend_from_slice(&chunk[..take]);
            self.reader.consume(newline.map_or(take, |i| i + 1));
            if newline.is_some() {
                self.spoken = true;
                return Next::Line(text(line));
            }
            if !started {
                started = true;
                deadline = Some(Instant::now() + self.limits.line);
            }
        }
    }
}

impl Requests {
    /// Read and discard what the client still sends, until it closes or
    /// `wait` passes, in a fixed buffer. Closing a Unix socket with unread
    /// data resets it, and the client could lose the reply just written.
    pub fn discard(&mut self, wait: Duration) {
        let deadline = Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() || self.reader.get_ref().set_read_timeout(Some(left)).is_err() {
                return;
            }
            match self.reader.fill_buf() {
                Ok([]) => return,
                Ok(chunk) => {
                    let used = chunk.len();
                    self.reader.consume(used);
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => return,
            }
        }
    }
}

fn text(mut line: Vec<u8>) -> String {
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    String::from_utf8_lossy(&line).into_owned()
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    use super::{Limits, Next, Requests};

    /// The next line read on another thread, or None if it takes over two
    /// seconds, so a reader that never gives up fails instead of hanging.
    fn within_two_seconds(mut requests: Requests) -> Option<(Next, Duration)> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let start = Instant::now();
            let _ = tx.send((requests.next_line(), start.elapsed()));
        });
        rx.recv_timeout(Duration::from_secs(2)).ok()
    }

    const QUICK: Limits = Limits {
        first: Duration::from_millis(200),
        line: Duration::from_millis(200),
        max: 64,
    };

    // REQ-PERF-02: a client that connects and never sends is cut off, so it
    // cannot hold a thread and its descriptors forever.
    #[test]
    fn req_perf_02_a_silent_client_is_cut_off() {
        let (_client, daemon) = UnixStream::pair().unwrap();
        let requests = Requests::new(daemon, QUICK);
        let (next, _) = within_two_seconds(requests).expect("a silent client is kept");
        assert_eq!(next, Next::Silent);
    }

    // REQ-PERF-02: a line that starts must end in time; trickling bytes do
    // not extend it.
    #[test]
    fn req_perf_02_a_line_that_never_ends_is_cut_off() {
        let (mut client, daemon) = UnixStream::pair().unwrap();
        let requests = Requests::new(daemon, QUICK);
        let writer = std::thread::spawn(move || {
            for _ in 0..60 {
                if client.write_all(b"x").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            client
        });
        let (next, took) = within_two_seconds(requests).expect("a trickling line is kept");
        assert_eq!(next, Next::Silent);
        assert!(took < Duration::from_millis(600), "cut off after {took:?}");
        drop(writer.join());
    }

    // REQ-PERF-02: one overlong line is refused before it grows memory.
    #[test]
    fn req_perf_02_an_overlong_line_is_refused() {
        let (mut client, daemon) = UnixStream::pair().unwrap();
        let mut requests = Requests::new(daemon, QUICK);
        client.write_all(&[b'x'; 65]).unwrap();
        client.write_all(b"\n").unwrap();
        assert_eq!(requests.next_line(), Next::TooLong);
    }

    // A line at the limit is kept, and a client that has spoken may then
    // stay quiet between requests, as the overlay's connections do.
    #[test]
    fn req_perf_02_a_client_that_spoke_may_wait() {
        let (mut client, daemon) = UnixStream::pair().unwrap();
        let mut requests = Requests::new(daemon, QUICK);
        let full = "y".repeat(64);
        client.write_all(format!("{full}\n").as_bytes()).unwrap();
        assert_eq!(requests.next_line(), Next::Line(full));
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            client.write_all(b"second\r\nlast").unwrap();
        });
        assert_eq!(requests.next_line(), Next::Line("second".into()));
        writer.join().unwrap();
        assert_eq!(requests.next_line(), Next::Line("last".into()));
        assert_eq!(requests.next_line(), Next::Closed);
    }
}
