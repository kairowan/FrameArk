//! Random-access bytes for OS demuxers. No platform URL fetching or ambient DNS.

use super::*;
use std::net::Shutdown;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Maximum bytes returned or cached by one random-access read.
pub const MAX_SOURCE_READ_BYTES: usize = 64 * 1024;
const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const SOURCE_IO_POLL: Duration = Duration::from_millis(1);

/// Payload-free source I/O counters, not evidence of decoded/rendered output.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RemoteSourceReport {
    /// HTTP requests attempted, including the initial byte-range probe.
    pub requests: u64,
    /// Validated media response bytes received.
    pub received_bytes: u64,
    /// Reads served from the one-chunk cache.
    pub cache_hits: u64,
    /// Failed read operations (excluding normal EOF).
    pub read_failures: u64,
}

#[derive(Default)]
struct Counters {
    requests: AtomicU64,
    bytes: AtomicU64,
    hits: AtomicU64,
    failures: AtomicU64,
}

fn add(counter: &AtomicU64, value: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
        Some(old.saturating_add(value))
    });
}

struct Endpoint {
    scheme: String,
    authority: String,
    target: String,
    host: String,
    addresses: Vec<SocketAddr>,
    roots: Option<Arc<rustls::RootCertStore>>,
    timeout: Duration,
    deadline: Option<Instant>,
    closed: Arc<AtomicBool>,
    socket: Arc<Mutex<Option<TcpStream>>>,
    counters: Counters,
}

impl Endpoint {
    fn connect(&self) -> Result<DeadlineStream, DlnaError> {
        self.check_open()?;
        let deadline = self
            .deadline
            .map_or(Instant::now() + self.timeout, |limit| {
                limit.min(Instant::now() + self.timeout)
            });
        let mut last_error = std::io::ErrorKind::ConnectionRefused;
        for address in &self.addresses {
            self.check_open()?;
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(DlnaError::Io(std::io::ErrorKind::TimedOut))?;
            match TcpStream::connect_timeout(address, remaining) {
                Ok(socket) => {
                    socket
                        .set_nonblocking(true)
                        .map_err(|error| DlnaError::Io(error.kind()))?;
                    let clone = socket
                        .try_clone()
                        .map_err(|error| DlnaError::Io(error.kind()))?;
                    *self
                        .socket
                        .lock()
                        .map_err(|_| DlnaError::Invalid("source lock"))? = Some(clone);
                    if self.closed.load(Ordering::Acquire) {
                        let _ = socket.shutdown(Shutdown::Both);
                        return Err(DlnaError::Invalid("source closed"));
                    }
                    let tcp = DeadlineTcp {
                        socket,
                        deadline,
                        closed: self.closed.clone(),
                    };
                    let transport = if self.scheme == "https" {
                        let roots = self
                            .roots
                            .as_ref()
                            .ok_or(DlnaError::Invalid("source TLS roots"))?;
                        let config = rustls::ClientConfig::builder()
                            .with_root_certificates((**roots).clone())
                            .with_no_client_auth();
                        let name = ServerName::try_from(self.host.clone())
                            .map_err(|_| DlnaError::Invalid("source TLS name"))?;
                        let connection = rustls::ClientConnection::new(Arc::new(config), name)
                            .map_err(|_| DlnaError::Invalid("source TLS config"))?;
                        SourceTransport::Tls(Box::new(rustls::StreamOwned::new(connection, tcp)))
                    } else {
                        SourceTransport::Http(tcp)
                    };
                    return Ok(DeadlineStream {
                        transport,
                        socket: self.socket.clone(),
                    });
                }
                Err(error) => last_error = error.kind(),
            }
        }
        Err(DlnaError::Io(last_error))
    }

    fn check_open(&self) -> Result<(), DlnaError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(DlnaError::Invalid("source closed"));
        }
        if self.deadline.is_some_and(|limit| Instant::now() >= limit) {
            return Err(DlnaError::Io(std::io::ErrorKind::TimedOut));
        }
        Ok(())
    }

    fn fetch(
        &self,
        start: u64,
        end: u64,
        validator: Option<&str>,
    ) -> Result<HttpResponse, DlnaError> {
        self.check_open()?;
        add(&self.counters.requests, 1);
        let mut stream = self.connect()?;
        let conditional = validator
            .map(|value| format!("If-Match: {value}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nRange: bytes={start}-{end}\r\nAccept-Encoding: identity\r\nConnection: close\r\n{conditional}\r\n",
            self.target, self.authority
        );
        stream
            .write_all(request.as_bytes())
            .and_then(|_| stream.flush())
            .map_err(|error| DlnaError::Io(error.kind()))?;
        let response = read_http_response(&mut stream)?;
        self.check_open()?;
        if response.status != 206
            || response
                .headers
                .get("content-encoding")
                .is_some_and(|encoding| !encoding.eq_ignore_ascii_case("identity"))
        {
            return Err(DlnaError::Invalid("source range response"));
        }
        let (actual_start, actual_end, _) = parse_content_range(
            response
                .headers
                .get("content-range")
                .ok_or(DlnaError::Invalid("source content range"))?,
        )?;
        if actual_start as u64 != start
            || actual_end as u64 != end
            || response.body.len() as u64 != end - start + 1
            || response.body.len() > MAX_SOURCE_READ_BYTES
        {
            return Err(DlnaError::Invalid("source content range"));
        }
        add(&self.counters.bytes, response.body.len() as u64);
        Ok(response)
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        if let Ok(mut socket) = self.socket.lock()
            && let Some(socket) = socket.take()
        {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
}

struct DeadlineTcp {
    socket: TcpStream,
    deadline: Instant,
    closed: Arc<AtomicBool>,
}

impl DeadlineTcp {
    // Do not rely on shutdown of a cloned handle waking an already-blocked
    // recv on every platform. Retry only nonblocking WouldBlock/Interrupted,
    // always against the original deadline and revocation flag. In particular,
    // this never retries a Windows SO_RCVTIMEO failure on an uncertain socket.
    fn io(
        &mut self,
        mut operation: impl FnMut(&mut TcpStream) -> std::io::Result<usize>,
    ) -> std::io::Result<usize> {
        loop {
            self.remaining()?;
            let result = operation(&mut self.socket);
            let remaining = self.remaining()?;
            match result {
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(SOURCE_IO_POLL.min(remaining));
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                other => return other,
            }
        }
    }

    fn remaining(&self) -> std::io::Result<Duration> {
        if self.closed.load(Ordering::Acquire) {
            // `write_all` and TLS may retry Interrupted indefinitely. Revocation
            // is terminal, including when it races a successful partial write.
            return Err(std::io::ErrorKind::ConnectionAborted.into());
        }
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|value| !value.is_zero())
            .ok_or_else(|| std::io::ErrorKind::TimedOut.into())
    }
}

impl Read for DeadlineTcp {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.io(|socket| socket.read(buffer))
    }
}

impl Write for DeadlineTcp {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.io(|socket| socket.write(buffer))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.remaining()?;
        self.socket.flush()
    }
}

enum SourceTransport {
    Http(DeadlineTcp),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, DeadlineTcp>>),
}

struct DeadlineStream {
    transport: SourceTransport,
    socket: Arc<Mutex<Option<TcpStream>>>,
}
impl Read for DeadlineStream {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match &mut self.transport {
            SourceTransport::Http(stream) => stream.read(buffer),
            SourceTransport::Tls(stream) => stream.read(buffer),
        }
    }
}
impl Write for DeadlineStream {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        match &mut self.transport {
            SourceTransport::Http(stream) => stream.write(buffer),
            SourceTransport::Tls(stream) => stream.write(buffer),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match &mut self.transport {
            SourceTransport::Http(stream) => stream.flush(),
            SourceTransport::Tls(stream) => stream.flush(),
        }
    }
}
impl Drop for DeadlineStream {
    fn drop(&mut self) {
        if let Ok(mut socket) = self.socket.lock()
            && let Some(socket) = socket.take()
        {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
}

/// Revocable, known-length HTTP(S) bytes for an OS demuxer.
///
/// The Rust host must authorize the URL and supply an already-resolved, bounded
/// address set. Every connection uses that exact set, never ambient DNS. The
/// network class is checked again here; it is not a substitute for authorization.
/// Redirects, compressed/chunked bodies, unknown length and range-ignoring origins
/// are rejected. HTTPS requires explicit trusted roots and validates the original
/// hostname. No URL, metadata or content appears in Debug or diagnostics.
///
/// One read is allowed at a time; concurrent reads return a bounded error. Close
/// may run concurrently and interrupts active socket I/O without waiting on the
/// read/cache lock. Connected I/O retries nonblocking readiness with at most a
/// one-millisecond sleep, rechecking revocation and the original total deadline.
/// This is not a real-time scheduling guarantee. A connect already in progress
/// has the same total deadline.
pub struct RemoteMediaSource {
    endpoint: Endpoint,
    length: u64,
    mime_type: String,
    etag: Option<String>,
    cache: Mutex<(u64, Vec<u8>)>,
}

impl RemoteMediaSource {
    /// Probes byte 0 after checking the supplied pinned addresses and policy.
    pub fn open(
        uri: &str,
        policy: MediaUriNetworkPolicy,
        addresses: Vec<IpAddr>,
        roots: Option<Arc<rustls::RootCertStore>>,
        timeout: Duration,
    ) -> Result<Self, DlnaError> {
        Self::open_with_deadline(uri, policy, addresses, roots, timeout, None)
    }

    fn open_with_deadline(
        uri: &str,
        policy: MediaUriNetworkPolicy,
        addresses: Vec<IpAddr>,
        roots: Option<Arc<rustls::RootCertStore>>,
        timeout: Duration,
        deadline: Option<Instant>,
    ) -> Result<Self, DlnaError> {
        if timeout.is_zero()
            || timeout > Duration::from_secs(5)
            || addresses.is_empty()
            || addresses.len() > MAX_MEDIA_RESOLVED_ADDRESSES
        {
            return Err(DlnaError::Invalid("source options"));
        }
        let (scheme, authority, target) = split_media_uri(uri)?;
        let host = media_uri_host(uri)?.to_string();
        if let Ok(literal) = host.parse::<IpAddr>()
            && addresses.iter().any(|address| *address != literal)
        {
            return Err(DlnaError::Invalid("source address binding"));
        }
        validate_media_uri_network(uri, policy, |_| Ok(addresses.clone()))?;
        // Literal authorities bypass the resolver in the validator; this check
        // also covers caller-supplied address sets before any socket is opened.
        for address in &addresses {
            let literal_uri = match address {
                IpAddr::V4(ip) => format!("http://{ip}/"),
                IpAddr::V6(ip) => format!("http://[{ip}]/"),
            };
            validate_media_uri_network(&literal_uri, policy, |_| unreachable!())?;
        }
        if scheme == "https"
            && roots.as_ref().is_none_or(|roots| {
                roots.is_empty() || roots.len() > MAX_MEDIA_TLS_ROOT_CERTIFICATES
            })
        {
            return Err(DlnaError::Invalid("source TLS roots"));
        }
        let (_, mut port) = split_media_authority(&authority)?;
        let explicit_port = if authority.starts_with('[') {
            !authority.ends_with(']')
        } else {
            authority.contains(':')
        };
        if scheme == "http" && !explicit_port {
            port = 80;
        }
        let endpoint = Endpoint {
            scheme,
            authority,
            target,
            host,
            addresses: addresses
                .into_iter()
                .map(|address| SocketAddr::new(address, port))
                .collect(),
            roots,
            timeout,
            deadline,
            closed: Arc::new(AtomicBool::new(false)),
            socket: Arc::new(Mutex::new(None)),
            counters: Counters::default(),
        };
        let response = endpoint.fetch(0, 0, None)?;
        let probe = MediaFormatProbe::from_response(&response)?;
        let (_, _, total) = parse_content_range(
            response
                .headers
                .get("content-range")
                .ok_or(DlnaError::Invalid("source content range"))?,
        )?;
        let length = total
            .filter(|length| *length > 0 && *length <= MAX_SOURCE_BYTES)
            .ok_or(DlnaError::Invalid("source length"))?;
        let etag = response
            .headers
            .get("etag")
            .filter(|value| {
                value.len() >= 2
                    && value.starts_with('"')
                    && value.ends_with('"')
                    && value.as_bytes()[1..value.len() - 1]
                        .iter()
                        .all(|byte| *byte == 0x21 || (0x23..=0x7e).contains(byte) || *byte >= 0x80)
            })
            .cloned();
        Ok(Self {
            endpoint,
            length,
            mime_type: probe.mime_type,
            etag,
            cache: Mutex::new((0, response.body)),
        })
    }

    /// Opens a short-lived source whose probe and all subsequent reads share an
    /// absolute deadline. Useful for bounded sidecars, not long-lived OS playback.
    /// The per-operation timeout still caps each connection; neither a new read
    /// nor a cached read renews the supplied total budget.
    pub fn open_until(
        uri: &str,
        policy: MediaUriNetworkPolicy,
        addresses: Vec<IpAddr>,
        roots: Option<Arc<rustls::RootCertStore>>,
        timeout: Duration,
        deadline: Instant,
    ) -> Result<Self, DlnaError> {
        Self::open_with_deadline(uri, policy, addresses, roots, timeout, Some(deadline))
    }

    /// Known byte length, not duration or decoder capability evidence.
    pub fn len(&self) -> u64 {
        self.length
    }
    /// Sources are never accepted empty.
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
    /// Validated MIME metadata from the origin, not decoder inspection.
    pub fn mime_type(&self) -> &str {
        &self.mime_type
    }

    /// Returns up to 64 KiB from an absolute byte offset; empty means EOF/size 0.
    pub fn read_at(&self, position: u64, requested: usize) -> Result<Vec<u8>, DlnaError> {
        let result = self.read_inner(position, requested);
        if result.is_err() {
            add(&self.endpoint.counters.failures, 1);
        }
        result
    }

    fn read_inner(&self, position: u64, requested: usize) -> Result<Vec<u8>, DlnaError> {
        self.endpoint.check_open()?;
        if requested == 0 || position >= self.length {
            return Ok(Vec::new());
        }
        let mut cache = self
            .cache
            .try_lock()
            .map_err(|_| DlnaError::Invalid("source read busy"))?;
        let requested = requested
            .min(MAX_SOURCE_READ_BYTES)
            .min(usize::try_from(self.length - position).unwrap_or(usize::MAX));
        if position >= cache.0 && position - cache.0 < cache.1.len() as u64 {
            let offset = (position - cache.0) as usize;
            let end = (offset + requested).min(cache.1.len());
            self.endpoint.check_open()?;
            add(&self.endpoint.counters.hits, 1);
            return Ok(cache.1[offset..end].to_vec());
        }
        let end = (position + MAX_SOURCE_READ_BYTES as u64 - 1).min(self.length - 1);
        let response = self.endpoint.fetch(position, end, self.etag.as_deref())?;
        let (_, _, total) = parse_content_range(
            response
                .headers
                .get("content-range")
                .ok_or(DlnaError::Invalid("source content range"))?,
        )?;
        let probe = MediaFormatProbe::from_response(&response)?;
        if total != Some(self.length)
            || probe.mime_type != self.mime_type
            || self
                .etag
                .as_ref()
                .is_some_and(|etag| response.headers.get("etag") != Some(etag))
        {
            self.close();
            return Err(DlnaError::Invalid("source changed"));
        }
        *cache = (position, response.body);
        self.endpoint.check_open()?;
        Ok(cache.1[..requested].to_vec())
    }

    /// Revokes future reads and interrupts an in-flight read. Idempotent.
    pub fn close(&self) {
        self.endpoint.close();
    }

    /// Snapshot of saturating counters; no addresses or media are included.
    pub fn report(&self) -> RemoteSourceReport {
        let counters = &self.endpoint.counters;
        RemoteSourceReport {
            requests: counters.requests.load(Ordering::Relaxed),
            received_bytes: counters.bytes.load(Ordering::Relaxed),
            cache_hits: counters.hits.load(Ordering::Relaxed),
            read_failures: counters.failures.load(Ordering::Relaxed),
        }
    }
}

impl Drop for RemoteMediaSource {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_read_observes_revocation_without_socket_shutdown() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (_peer, _) = listener.accept().unwrap();
        socket.set_nonblocking(true).unwrap();
        let closed = Arc::new(AtomicBool::new(false));
        let mut stream = DeadlineTcp {
            socket,
            deadline: Instant::now() + Duration::from_secs(5),
            closed: closed.clone(),
        };
        let (started, ready) = std::sync::mpsc::channel();
        let (resume, resumed) = std::sync::mpsc::channel();
        let (done, finished) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut first = true;
            let result = stream.io(|socket| {
                let result = socket.read(&mut [0]);
                if first {
                    assert_eq!(
                        result.as_ref().unwrap_err().kind(),
                        std::io::ErrorKind::WouldBlock
                    );
                    first = false;
                    started.send(()).unwrap();
                    resumed.recv_timeout(Duration::from_secs(2)).unwrap();
                }
                result
            });
            done.send(result.unwrap_err().kind()).unwrap();
        });
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        // Deliberately do not shutdown either socket: progress must depend on
        // our cancellation flag, not platform-specific wakeup behavior.
        closed.store(true, Ordering::Release);
        resume.send(()).unwrap();
        assert_eq!(
            finished.recv_timeout(Duration::from_secs(1)).unwrap(),
            std::io::ErrorKind::ConnectionAborted
        );
        worker.join().unwrap();
    }

    #[test]
    fn pending_io_retries_keep_original_deadline_and_terminal_errors() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (_peer, _) = listener.accept().unwrap();
        socket.set_nonblocking(true).unwrap();
        let mut stream = DeadlineTcp {
            socket,
            deadline: Instant::now() + Duration::from_secs(1),
            closed: Arc::new(AtomicBool::new(false)),
        };
        for kind in [
            std::io::ErrorKind::ConnectionReset,
            std::io::ErrorKind::TimedOut,
        ] {
            let mut calls = 0;
            assert_eq!(
                stream
                    .io(|_| {
                        calls += 1;
                        Err(kind.into())
                    })
                    .unwrap_err()
                    .kind(),
                kind
            );
            assert_eq!(calls, 1);
        }
        for kind in [
            std::io::ErrorKind::WouldBlock,
            std::io::ErrorKind::Interrupted,
        ] {
            stream.deadline = Instant::now() + Duration::from_millis(30);
            let start = Instant::now();
            let mut calls = 0;
            assert_eq!(
                stream
                    .io(|_| {
                        calls += 1;
                        Err(kind.into())
                    })
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::TimedOut
            );
            assert!(calls > 1);
            assert!(start.elapsed() < Duration::from_secs(1));
        }
    }

    #[test]
    fn revoked_io_is_terminal_not_retryable_interrupted() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let mut stream = DeadlineTcp {
            socket,
            deadline: Instant::now() + Duration::from_secs(1),
            closed: Arc::new(AtomicBool::new(true)),
        };
        assert_eq!(
            stream.write(b"request").unwrap_err().kind(),
            std::io::ErrorKind::ConnectionAborted
        );
        assert_eq!(
            stream.read(&mut [0]).unwrap_err().kind(),
            std::io::ErrorKind::ConnectionAborted
        );
    }
}
