//! One accepted RTSP connection, one frame, one monotonic I/O budget.
use super::{AirplayError, MAX_RTSP_BODY_BYTES, MAX_RTSP_BYTES, RtspMessage, parse_rtsp_head};
use std::io::{self, ErrorKind, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

pub(super) struct Connection {
    stream: TcpStream,
    deadline: Instant,
}

impl Connection {
    pub(super) fn new(stream: TcpStream, deadline: Instant) -> Result<Self, AirplayError> {
        check_deadline(deadline)?;
        // Accepted sockets inherit different mode flags on different platforms.
        // Never assume that an OS read timeout changes a nonblocking socket.
        stream
            .set_nonblocking(true)
            .map_err(|e| AirplayError::Io(e.kind()))?;
        Ok(Self { stream, deadline })
    }

    pub(super) fn read_request(&mut self) -> Result<RtspMessage, AirplayError> {
        let mut bytes = Vec::with_capacity(4096);
        let mut chunk = [0; 4096];
        let mut scan_from = 0;
        let mut expected = None;
        loop {
            check_deadline(self.deadline)?;
            if expected.is_none() {
                if let Some(offset) = bytes[scan_from..].windows(4).position(|w| w == b"\r\n\r\n") {
                    expected = Some(frame_length(&bytes[..scan_from + offset])?);
                }
                // Scan each header byte once, preserving a delimiter split across reads.
                scan_from = bytes.len().saturating_sub(3);
            }
            if let Some(length) = expected {
                if bytes.len() > length {
                    return Err(AirplayError::Invalid("RTSP trailing bytes"));
                }
                if bytes.len() == length {
                    let request = RtspMessage::parse(&bytes)?;
                    check_deadline(self.deadline)?;
                    return Ok(request);
                }
            }
            let remaining = expected.unwrap_or(MAX_RTSP_BYTES) - bytes.len();
            if remaining == 0 {
                return Err(AirplayError::TooLarge("RTSP request"));
            }
            let limit = remaining.min(chunk.len());
            let count = retry_io(self.deadline, || self.stream.read(&mut chunk[..limit]))?;
            if count == 0 {
                return Err(AirplayError::Invalid("RTSP truncated request"));
            }
            bytes.extend_from_slice(&chunk[..count]);
        }
    }

    pub(super) fn write_response(&mut self, bytes: &[u8]) -> Result<(), AirplayError> {
        write_response(&mut self.stream, bytes, self.deadline)
    }
}

fn frame_length(head: &[u8]) -> Result<usize, AirplayError> {
    if head.len() > MAX_RTSP_BYTES - 4 {
        return Err(AirplayError::TooLarge("RTSP request"));
    }
    let parsed = parse_rtsp_head(head)?;
    let length = parsed
        .headers
        .get("content-length")
        .map(|v| v.parse::<usize>())
        .transpose()
        .map_err(|_| AirplayError::Invalid("RTSP content length"))?
        .unwrap_or(0);
    if length > MAX_RTSP_BODY_BYTES {
        return Err(AirplayError::TooLarge("RTSP body"));
    }
    head.len()
        .checked_add(4)
        .and_then(|n| n.checked_add(length))
        .filter(|n| *n <= MAX_RTSP_BYTES)
        .ok_or(AirplayError::TooLarge("RTSP request"))
}

fn check_deadline(deadline: Instant) -> Result<(), AirplayError> {
    if Instant::now() >= deadline {
        Err(AirplayError::Io(ErrorKind::TimedOut))
    } else {
        Ok(())
    }
}

fn retry_io<T>(
    deadline: Instant,
    mut action: impl FnMut() -> io::Result<T>,
) -> Result<T, AirplayError> {
    loop {
        check_deadline(deadline)?;
        match action() {
            Ok(value) => {
                check_deadline(deadline)?;
                return Ok(value);
            }
            Err(error)
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) =>
            {
                let remaining = deadline.saturating_duration_since(Instant::now());
                std::thread::sleep(remaining.min(Duration::from_millis(1)));
            }
            Err(error) => return Err(AirplayError::Io(error.kind())),
        }
    }
}

fn write_response(
    writer: &mut impl Write,
    bytes: &[u8],
    deadline: Instant,
) -> Result<(), AirplayError> {
    if bytes.len() > MAX_RTSP_BYTES {
        return Err(AirplayError::TooLarge("RTSP response"));
    }
    check_deadline(deadline)?;
    let mut remaining = bytes;
    while !remaining.is_empty() {
        let count = retry_io(deadline, || writer.write(remaining))?;
        if count == 0 {
            return Err(AirplayError::Io(ErrorKind::WriteZero));
        }
        remaining = &remaining[count..];
    }
    // TcpStream has no userspace write buffer and does not need a separate flush.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct Writer {
        steps: VecDeque<Result<usize, ErrorKind>>,
        bytes: Vec<u8>,
        calls: usize,
    }
    impl Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            let n = self
                .steps
                .pop_front()
                .unwrap_or(Ok(bytes.len()))
                .map_err(io::Error::from)?
                .min(bytes.len());
            self.bytes.extend_from_slice(&bytes[..n]);
            Ok(n)
        }
        fn flush(&mut self) -> io::Result<()> {
            panic!("TCP must not invent a second flush budget")
        }
    }
    fn writer(steps: &[Result<usize, ErrorKind>]) -> Writer {
        Writer {
            steps: steps.iter().copied().collect(),
            bytes: Vec::new(),
            calls: 0,
        }
    }

    #[test]
    fn partial_response_writes_retry_only_transient_errors() {
        let mut sink = writer(&[
            Ok(1),
            Err(ErrorKind::WouldBlock),
            Err(ErrorKind::Interrupted),
            Ok(2),
        ]);
        write_response(
            &mut sink,
            b"abcdef",
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(sink.bytes, b"abcdef");
        assert_eq!(sink.calls, 5);
        for error in [
            ErrorKind::ConnectionReset,
            ErrorKind::TimedOut,
            ErrorKind::PermissionDenied,
        ] {
            let mut sink = writer(&[Err(error)]);
            assert_eq!(
                write_response(
                    &mut sink,
                    b"abcdef",
                    Instant::now() + Duration::from_secs(1)
                ),
                Err(AirplayError::Io(error))
            );
            assert_eq!(sink.calls, 1);
        }
        let mut sink = writer(&[Ok(0)]);
        assert_eq!(
            write_response(&mut sink, b"x", Instant::now() + Duration::from_secs(1)),
            Err(AirplayError::Io(ErrorKind::WriteZero))
        );
    }

    #[test]
    fn deadline_does_not_reset_for_response_or_retry_and_expiry_is_terminal() {
        let mut sink = writer(&[]);
        assert_eq!(
            write_response(&mut sink, b"response", Instant::now()),
            Err(AirplayError::Io(ErrorKind::TimedOut))
        );
        assert_eq!(sink.calls, 0);
        let deadline = Instant::now() + Duration::from_millis(10);
        let result: Result<(), _> =
            retry_io(deadline, || Err(io::Error::from(ErrorKind::WouldBlock)));
        assert_eq!(result, Err(AirplayError::Io(ErrorKind::TimedOut)));
        assert_eq!(
            write_response(&mut sink, b"response", deadline),
            Err(AirplayError::Io(ErrorKind::TimedOut))
        );
        assert_eq!(sink.calls, 0);
        let result = retry_io(Instant::now() + Duration::from_millis(5), || {
            std::thread::sleep(Duration::from_millis(15));
            Ok(1)
        });
        assert_eq!(result, Err(AirplayError::Io(ErrorKind::TimedOut)));
    }

    #[test]
    fn framing_rejects_bad_lengths_before_body_allocation_and_arithmetic() {
        for length in [MAX_RTSP_BODY_BYTES + 1, usize::MAX] {
            assert!(
                frame_length(format!("OPTIONS * RTSP/1.0\r\nContent-Length: {length}").as_bytes())
                    .is_err()
            );
        }
        for head in [
            &b"OPTIONS * RTSP/1.0\r\nContent-Length: -1"[..],
            &b"OPTIONS * RTSP/1.0\r\nContent-Length: a"[..],
            &b"OPTIONS * RTSP/1.0\r\nContent-Length: 0\r\ncontent-length: 1"[..],
            &b"OPTIONS * RTSP/1.0\r\n\xff: x"[..],
        ] {
            assert!(frame_length(head).is_err());
        }
        let head = b"OPTIONS * RTSP/1.0\r\nCSeq: 1";
        assert_eq!(frame_length(head), Ok(head.len() + 4));
        assert!(frame_length(&vec![b'x'; MAX_RTSP_BYTES]).is_err());
        let mut sink = writer(&[]);
        assert_eq!(
            write_response(
                &mut sink,
                &vec![b'x'; MAX_RTSP_BYTES + 1],
                Instant::now() + Duration::from_secs(1)
            ),
            Err(AirplayError::TooLarge("RTSP response"))
        );
        assert_eq!(sink.calls, 0);
    }
}
