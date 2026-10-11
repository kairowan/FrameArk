//! Bounded DLNA/UPnP protocol contracts.
//!
//! This crate starts M3 with deterministic SSDP parsing and XML description
//! generation. It provides a synchronous unicast-testable UDP publisher, but
//! deliberately does not join multicast groups, expose an HTTP server, or
//! claim AVTransport/GENA interoperability until those layers have their own
//! bounded parsers and compatibility fixtures.

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Maximum complete SSDP message accepted by the parser.
pub const MAX_SSDP_BYTES: usize = 64 * 1024;
/// Maximum SSDP header value or XML field accepted by this foundation.
pub const MAX_FIELD_BYTES: usize = 1024;
/// Maximum SSDP body accepted by this foundation.
pub const MAX_BODY_BYTES: usize = 16 * 1024;
/// Maximum UPnP services emitted in one device description.
pub const MAX_SERVICES: usize = 8;
/// Maximum complete HTTP request or response accepted by the renderer.
pub const MAX_HTTP_BYTES: usize = 128 * 1024;
/// Maximum HTTP headers accepted by one request.
pub const MAX_HTTP_HEADERS: usize = 64;
/// Maximum bytes used for one SOAP argument value.
pub const MAX_SOAP_ARGUMENT_BYTES: usize = 16 * 1024;
/// Maximum in-memory media resource accepted by the bounded response helper.
pub const MAX_MEDIA_RESOURCE_BYTES: usize = 8 * 1024 * 1024;
/// Maximum file size accepted by the explicit streaming media backend.
pub const MAX_MEDIA_FILE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// Maximum bytes copied per file-response write before yielding to the socket.
pub const MAX_MEDIA_STREAM_CHUNK_BYTES: usize = 64 * 1024;
/// Maximum resources registered on one handler.
pub const MAX_MEDIA_RESOURCES: usize = 16;
/// Maximum DIDL-Lite items emitted by one metadata document.
pub const MAX_DIDL_ITEMS: usize = 8;
/// Maximum active GENA subscriptions on one renderer.
pub const MAX_GENA_SUBSCRIPTIONS: usize = 16;
/// Maximum queued GENA callback events awaiting the caller's HTTP client.
pub const MAX_GENA_PENDING_EVENTS: usize = 32;
/// Maximum requests handled by one bounded synchronous accept loop.
pub const MAX_TCP_REQUESTS_PER_RUN: usize = 1024;
/// Default GENA lease when a subscriber omits or gives an invalid timeout.
pub const DEFAULT_GENA_TIMEOUT_SECONDS: u32 = 1800;
/// Maximum GENA lease accepted from an untrusted subscriber.
pub const MAX_GENA_TIMEOUT_SECONDS: u32 = 86_400;

/// Standard UPnP service type used by the renderer control plane.
pub const AVTRANSPORT_SERVICE: &str = "urn:schemas-upnp-org:service:AVTransport:1";
/// Standard UPnP service type used by renderer volume control.
pub const RENDERING_CONTROL_SERVICE: &str = "urn:schemas-upnp-org:service:RenderingControl:1";
/// Standard UPnP service type used by connection negotiation.
pub const CONNECTION_MANAGER_SERVICE: &str = "urn:schemas-upnp-org:service:ConnectionManager:1";

/// Errors raised before untrusted SSDP/XML data is exposed to callers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DlnaError {
    /// The input exceeds a bounded protocol limit.
    TooLarge(&'static str),
    /// The input violates the protocol grammar.
    Invalid(&'static str),
    /// A required description field is not valid.
    InvalidField(&'static str),
    /// The operating system rejected a bounded UDP operation.
    Io(std::io::ErrorKind),
}

impl Display for DlnaError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge(field) => write!(formatter, "DLNA {field} exceeds its bound"),
            Self::Invalid(field) => write!(formatter, "invalid DLNA {field}"),
            Self::InvalidField(field) => {
                write!(formatter, "invalid DLNA description field {field}")
            }
            Self::Io(kind) => write!(formatter, "DLNA UDP operation failed: {kind}"),
        }
    }
}

impl std::error::Error for DlnaError {}

/// A bounded HTTP/1.1 request used by the caller-owned TCP server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpRequest {
    /// Request method, retained in uppercase for known methods.
    pub method: String,
    /// Origin-form request target, including an optional query string.
    pub target: String,
    /// HTTP version. Only `HTTP/1.1` is accepted by the parser.
    pub version: String,
    /// Lowercase header names and trimmed values.
    pub headers: BTreeMap<String, String>,
    /// Exact body bytes declared by `Content-Length`.
    pub body: Vec<u8>,
}

impl HttpRequest {
    /// Parses one complete, non-chunked HTTP request within the configured bounds.
    pub fn parse(bytes: &[u8]) -> Result<Self, DlnaError> {
        if bytes.len() > MAX_HTTP_BYTES {
            return Err(DlnaError::TooLarge("HTTP request"));
        }
        let header_end = bytes
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .ok_or(DlnaError::Invalid("HTTP terminator"))?;
        let header_bytes = &bytes[..header_end];
        let body = &bytes[header_end + 4..];
        let header_text =
            std::str::from_utf8(header_bytes).map_err(|_| DlnaError::Invalid("HTTP UTF-8"))?;
        let mut lines = header_text.split("\r\n");
        let request_line = lines
            .next()
            .ok_or(DlnaError::Invalid("HTTP request line"))?;
        let parts = request_line.split(' ').collect::<Vec<_>>();
        if parts.len() != 3 || parts[0].is_empty() || parts[1].is_empty() {
            return Err(DlnaError::Invalid("HTTP request line"));
        }
        if parts[2] != "HTTP/1.1" {
            return Err(DlnaError::Invalid("HTTP version"));
        }
        validate_field(parts[0], "HTTP method")?;
        validate_field(parts[1], "HTTP target")?;
        let mut headers = BTreeMap::new();
        for line in lines {
            let (name, value) = line
                .split_once(':')
                .ok_or(DlnaError::Invalid("HTTP header"))?;
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            {
                return Err(DlnaError::Invalid("HTTP header name"));
            }
            let value = value.trim();
            if value.is_empty() || value.len() > MAX_FIELD_BYTES {
                return Err(DlnaError::Invalid("HTTP header value"));
            }
            if headers
                .insert(name.to_ascii_lowercase(), value.to_string())
                .is_some()
                || headers.len() > MAX_HTTP_HEADERS
            {
                return Err(DlnaError::Invalid("HTTP duplicate or oversized header"));
            }
        }
        if headers.contains_key("transfer-encoding") {
            return Err(DlnaError::Invalid("HTTP transfer encoding"));
        }
        let declared = headers
            .get("content-length")
            .map(|value| {
                value
                    .parse::<usize>()
                    .map_err(|_| DlnaError::Invalid("HTTP content length"))
            })
            .transpose()?
            .unwrap_or(0);
        if declared != body.len() || body.len() > MAX_SOAP_ARGUMENT_BYTES {
            return Err(DlnaError::Invalid("HTTP content length"));
        }
        Ok(Self {
            method: parts[0].to_ascii_uppercase(),
            target: parts[1].to_string(),
            version: parts[2].to_string(),
            headers,
            body: body.to_vec(),
        })
    }

    /// Returns a header by case-insensitive name.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

/// A bounded HTTP response produced by the renderer handler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// HTTP reason phrase.
    pub reason: String,
    /// Lowercase response headers.
    pub headers: BTreeMap<String, String>,
    /// Response body bytes.
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Parses one complete, non-chunked HTTP/1.1 response within the configured bounds.
    pub fn parse(bytes: &[u8]) -> Result<Self, DlnaError> {
        if bytes.len() > MAX_HTTP_BYTES {
            return Err(DlnaError::TooLarge("HTTP response"));
        }
        let header_end = bytes
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .ok_or(DlnaError::Invalid("HTTP terminator"))?;
        let header_bytes = &bytes[..header_end];
        let body = &bytes[header_end + 4..];
        let header_text =
            std::str::from_utf8(header_bytes).map_err(|_| DlnaError::Invalid("HTTP UTF-8"))?;
        let mut lines = header_text.split("\r\n");
        let status_line = lines.next().ok_or(DlnaError::Invalid("HTTP status line"))?;
        let mut parts = status_line.splitn(3, ' ');
        let version = parts.next().unwrap_or_default();
        let status = parts.next().unwrap_or_default();
        let reason = parts.next().unwrap_or_default();
        if version != "HTTP/1.1" || status.len() != 3 || reason.is_empty() {
            return Err(DlnaError::Invalid("HTTP status line"));
        }
        let status = status
            .parse::<u16>()
            .map_err(|_| DlnaError::Invalid("HTTP status code"))?;
        validate_field(reason, "HTTP reason")?;
        let mut headers = BTreeMap::new();
        for line in lines {
            let (name, value) = line
                .split_once(':')
                .ok_or(DlnaError::Invalid("HTTP response header"))?;
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            {
                return Err(DlnaError::Invalid("HTTP response header name"));
            }
            let value = value.trim();
            if value.is_empty() || value.len() > MAX_FIELD_BYTES {
                return Err(DlnaError::Invalid("HTTP response header value"));
            }
            if headers
                .insert(name.to_ascii_lowercase(), value.to_string())
                .is_some()
                || headers.len() > MAX_HTTP_HEADERS
            {
                return Err(DlnaError::Invalid("HTTP duplicate or oversized header"));
            }
        }
        if headers.contains_key("transfer-encoding") {
            return Err(DlnaError::Invalid("HTTP transfer encoding"));
        }
        let declared = headers
            .get("content-length")
            .map(|value| {
                value
                    .parse::<usize>()
                    .map_err(|_| DlnaError::Invalid("HTTP content length"))
            })
            .transpose()?
            .unwrap_or(0);
        if declared != body.len() || body.len() > MAX_HTTP_BYTES {
            return Err(DlnaError::Invalid("HTTP content length"));
        }
        Ok(Self {
            status,
            reason: reason.to_string(),
            headers,
            body: body.to_vec(),
        })
    }

    /// Encodes a deterministic HTTP/1.1 response.
    pub fn encode(&self) -> Result<Vec<u8>, DlnaError> {
        if self.status < 100 || self.reason.is_empty() || self.body.len() > MAX_HTTP_BYTES {
            return Err(DlnaError::Invalid("HTTP response"));
        }
        let mut output = format!("HTTP/1.1 {} {}\r\n", self.status, self.reason).into_bytes();
        let mut headers = self.headers.clone();
        headers.insert("content-length".to_string(), self.body.len().to_string());
        headers.insert("connection".to_string(), "close".to_string());
        if headers.len() > MAX_HTTP_HEADERS {
            return Err(DlnaError::TooLarge("HTTP response headers"));
        }
        for (name, value) in headers {
            validate_field(&name, "HTTP response header name")?;
            validate_field(&value, "HTTP response header value")?;
            output.extend_from_slice(name.as_bytes());
            output.extend_from_slice(b": ");
            output.extend_from_slice(value.as_bytes());
            output.extend_from_slice(b"\r\n");
        }
        output.extend_from_slice(b"\r\n");
        output.extend_from_slice(&self.body);
        if output.len() > MAX_HTTP_BYTES {
            return Err(DlnaError::TooLarge("HTTP response"));
        }
        Ok(output)
    }

    fn xml(status: u16, reason: &str, body: String) -> Self {
        let mut headers = BTreeMap::new();
        headers.insert(
            "content-type".to_string(),
            "text/xml; charset=\"utf-8\"".to_string(),
        );
        Self {
            status,
            reason: reason.to_string(),
            headers,
            body: body.into_bytes(),
        }
    }

    fn empty(status: u16, reason: &str) -> Self {
        Self {
            status,
            reason: reason.to_string(),
            headers: BTreeMap::new(),
            body: Vec::new(),
        }
    }
}

/// Default IPv4 SSDP multicast destination.
pub const SSDP_MULTICAST_ADDR: &str = "239.255.255.250:1900";

/// Bounded advertisement metadata used by NOTIFY and M-SEARCH responses.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SsdpAdvertisement {
    /// Device or service type advertised in `NT`/`ST`.
    pub service_type: String,
    /// Stable unique service name.
    pub usn: String,
    /// HTTP URL of the device description.
    pub location: String,
    /// Cache lifetime in seconds.
    pub max_age_seconds: u32,
    /// Product/server token sent in the response.
    pub server: String,
}

impl SsdpAdvertisement {
    /// Validates advertisement fields before any UDP packet is emitted.
    pub fn new(
        service_type: impl Into<String>,
        usn: impl Into<String>,
        location: impl Into<String>,
        max_age_seconds: u32,
        server: impl Into<String>,
    ) -> Result<Self, DlnaError> {
        let advertisement = Self {
            service_type: service_type.into(),
            usn: usn.into(),
            location: location.into(),
            max_age_seconds,
            server: server.into(),
        };
        for (value, field) in [
            (&advertisement.service_type, "service type"),
            (&advertisement.usn, "USN"),
            (&advertisement.location, "LOCATION"),
            (&advertisement.server, "SERVER"),
        ] {
            validate_field(value, field)?;
        }
        if max_age_seconds == 0 {
            return Err(DlnaError::InvalidField("max-age"));
        }
        Ok(advertisement)
    }

    fn matches(&self, search_target: &str) -> bool {
        search_target == "ssdp:all"
            || search_target == self.service_type
            || search_target == self.usn
    }

    fn notify(&self, alive: bool) -> SsdpMessage {
        let mut headers = BTreeMap::new();
        headers.insert(
            "cache-control".to_string(),
            format!("max-age={}", self.max_age_seconds),
        );
        headers.insert("location".to_string(), self.location.clone());
        headers.insert("nt".to_string(), self.service_type.clone());
        headers.insert(
            "nts".to_string(),
            if alive { "ssdp:alive" } else { "ssdp:byebye" }.to_string(),
        );
        headers.insert("server".to_string(), self.server.clone());
        headers.insert("usn".to_string(), self.usn.clone());
        SsdpMessage {
            start_line: SsdpStartLine::Request {
                method: "NOTIFY".to_string(),
                target: "*".to_string(),
            },
            headers,
            body: Vec::new(),
        }
    }

    fn response(&self) -> SsdpMessage {
        let mut headers = BTreeMap::new();
        headers.insert(
            "cache-control".to_string(),
            format!("max-age={}", self.max_age_seconds),
        );
        headers.insert("location".to_string(), self.location.clone());
        headers.insert("server".to_string(), self.server.clone());
        headers.insert("st".to_string(), self.service_type.clone());
        headers.insert("usn".to_string(), self.usn.clone());
        SsdpMessage {
            start_line: SsdpStartLine::Response {
                status: 200,
                reason: "OK".to_string(),
            },
            headers,
            body: Vec::new(),
        }
    }
}

/// Synchronous, bounded SSDP UDP publisher for one service type.
pub struct SsdpPublisher {
    socket: UdpSocket,
    advertisement: SsdpAdvertisement,
}

impl SsdpPublisher {
    /// Binds a caller-selected interface/port. Multicast join is left to the
    /// platform service layer so tests and restricted networks can use unicast.
    pub fn bind(
        bind_address: SocketAddr,
        advertisement: SsdpAdvertisement,
    ) -> Result<Self, DlnaError> {
        let socket = UdpSocket::bind(bind_address).map_err(|error| DlnaError::Io(error.kind()))?;
        Ok(Self {
            socket,
            advertisement,
        })
    }

    /// Returns the local UDP endpoint selected by the operating system.
    pub fn local_addr(&self) -> Result<SocketAddr, DlnaError> {
        self.socket
            .local_addr()
            .map_err(|error| DlnaError::Io(error.kind()))
    }

    /// Sends one bounded `ssdp:alive` or `ssdp:byebye` packet.
    pub fn notify(&self, target: SocketAddr, alive: bool) -> Result<usize, DlnaError> {
        let bytes = self.advertisement.notify(alive).encode()?;
        self.socket
            .send_to(&bytes, target)
            .map_err(|error| DlnaError::Io(error.kind()))
    }

    /// Sends one bounded announcement to the standard IPv4 SSDP multicast group.
    ///
    /// The method is intentionally one-shot. A daemon owns the advertisement
    /// interval, interface selection, startup burst, and shutdown sequencing.
    pub fn notify_multicast(&self, alive: bool) -> Result<usize, DlnaError> {
        let target = SSDP_MULTICAST_ADDR
            .parse::<SocketAddr>()
            .map_err(|_| DlnaError::Invalid("SSDP multicast address"))?;
        self.socket
            .set_multicast_ttl_v4(2)
            .map_err(|error| DlnaError::Io(error.kind()))?;
        self.notify(target, alive)
    }

    /// Responds to a valid M-SEARCH request when its ST matches this service.
    /// Returns `Ok(None)` for a valid but unrelated search target.
    pub fn respond_to_search(
        &self,
        request: &SsdpMessage,
        target: SocketAddr,
    ) -> Result<Option<usize>, DlnaError> {
        let SsdpStartLine::Request { method, .. } = &request.start_line else {
            return Ok(None);
        };
        if method != "M-SEARCH"
            || request.headers.get("man").map(String::as_str) != Some("\"ssdp:discover\"")
        {
            return Ok(None);
        }
        let Some(search_target) = request.headers.get("st") else {
            return Ok(None);
        };
        if !self.advertisement.matches(search_target) {
            return Ok(None);
        }
        let bytes = self.advertisement.response().encode()?;
        self.socket
            .send_to(&bytes, target)
            .map(Some)
            .map_err(|error| DlnaError::Io(error.kind()))
    }
}

/// SSDP request or response start line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SsdpStartLine {
    /// `M-SEARCH * HTTP/1.1` or another explicitly supported method.
    Request { method: String, target: String },
    /// `HTTP/1.1 200 OK` response.
    Response { status: u16, reason: String },
}

/// A bounded SSDP message with normalized lowercase header names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SsdpMessage {
    /// Request or response start line.
    pub start_line: SsdpStartLine,
    /// Header names are lowercase ASCII; values retain their meaningful case.
    pub headers: BTreeMap<String, String>,
    /// Optional bounded body.
    pub body: Vec<u8>,
}

impl SsdpMessage {
    /// Parses one complete CRLF-delimited SSDP message.
    pub fn parse(bytes: &[u8]) -> Result<Self, DlnaError> {
        if bytes.len() > MAX_SSDP_BYTES {
            return Err(DlnaError::TooLarge("SSDP message"));
        }
        let header_end = bytes
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .ok_or(DlnaError::Invalid("SSDP terminator"))?;
        let header_bytes = &bytes[..header_end];
        let body = &bytes[header_end + 4..];
        if body.len() > MAX_BODY_BYTES {
            return Err(DlnaError::TooLarge("SSDP body"));
        }
        let header_text =
            std::str::from_utf8(header_bytes).map_err(|_| DlnaError::Invalid("SSDP UTF-8"))?;
        let mut lines = header_text.split("\r\n");
        let start_line =
            parse_start_line(lines.next().ok_or(DlnaError::Invalid("SSDP start line"))?)?;
        let mut headers = BTreeMap::new();
        for line in lines {
            let (name, value) = line
                .split_once(':')
                .ok_or(DlnaError::Invalid("SSDP header"))?;
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-".contains(&byte))
            {
                return Err(DlnaError::Invalid("SSDP header name"));
            }
            let name = name.to_ascii_lowercase();
            let value = value.trim();
            if value.is_empty() {
                return Err(DlnaError::Invalid("SSDP header value"));
            }
            if value.len() > MAX_FIELD_BYTES || headers.insert(name, value.to_string()).is_some() {
                return Err(DlnaError::Invalid("SSDP duplicate or oversized header"));
            }
        }
        if let Some(length) = headers.get("content-length") {
            let declared = length
                .parse::<usize>()
                .map_err(|_| DlnaError::Invalid("SSDP content length"))?;
            if declared != body.len() {
                return Err(DlnaError::Invalid("SSDP content length"));
            }
        }
        Ok(Self {
            start_line,
            headers,
            body: body.to_vec(),
        })
    }

    /// Encodes the message with deterministic header order.
    pub fn encode(&self) -> Result<Vec<u8>, DlnaError> {
        if self.body.len() > MAX_BODY_BYTES {
            return Err(DlnaError::TooLarge("SSDP body"));
        }
        let start = match &self.start_line {
            SsdpStartLine::Request { method, target } => {
                validate_field(method, "SSDP method")?;
                validate_field(target, "SSDP target")?;
                format!("{method} {target} HTTP/1.1")
            }
            SsdpStartLine::Response { status, reason } => {
                if *status < 100 {
                    return Err(DlnaError::Invalid("SSDP status"));
                }
                validate_field(reason, "SSDP reason")?;
                format!("HTTP/1.1 {status} {reason}")
            }
        };
        let mut output = start.into_bytes();
        output.extend_from_slice(b"\r\n");
        for (name, value) in &self.headers {
            validate_field(name, "SSDP header name")?;
            validate_field(value, "SSDP header value")?;
            output.extend_from_slice(name.as_bytes());
            output.extend_from_slice(b": ");
            output.extend_from_slice(value.as_bytes());
            output.extend_from_slice(b"\r\n");
        }
        output.extend_from_slice(b"\r\n");
        output.extend_from_slice(&self.body);
        if output.len() > MAX_SSDP_BYTES {
            return Err(DlnaError::TooLarge("SSDP message"));
        }
        Ok(output)
    }
}

fn parse_start_line(line: &str) -> Result<SsdpStartLine, DlnaError> {
    if line.starts_with("HTTP/1.1 ") {
        let parts = line.splitn(3, ' ').collect::<Vec<_>>();
        if parts.len() != 3 || parts[2].is_empty() {
            return Err(DlnaError::Invalid("SSDP start line"));
        }
        let status = parts[1]
            .parse::<u16>()
            .map_err(|_| DlnaError::Invalid("SSDP status"))?;
        return Ok(SsdpStartLine::Response {
            status,
            reason: parts[2].to_string(),
        });
    }
    let parts = line.splitn(3, ' ').collect::<Vec<_>>();
    if parts.len() != 3 || parts[2] != "HTTP/1.1" {
        return Err(DlnaError::Invalid("SSDP start line"));
    }
    if !matches!(parts[0], "M-SEARCH" | "NOTIFY") || parts[1].is_empty() {
        return Err(DlnaError::Invalid("SSDP request"));
    }
    Ok(SsdpStartLine::Request {
        method: parts[0].to_string(),
        target: parts[1].to_string(),
    })
}

fn validate_field(value: &str, field: &'static str) -> Result<(), DlnaError> {
    if value.is_empty() || value.len() > MAX_FIELD_BYTES || value.contains(['\r', '\n']) {
        return Err(DlnaError::InvalidField(field));
    }
    Ok(())
}

/// One UPnP service entry in a device description.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceDescription {
    /// UPnP service type URN.
    pub service_type: String,
    /// Stable service identifier.
    pub service_id: String,
    /// Relative or absolute control URL.
    pub control_url: String,
    /// Relative or absolute event URL.
    pub event_sub_url: String,
    /// Relative or absolute SCPD URL.
    pub scpd_url: String,
}

/// Bounded root device description for a DLNA MediaRenderer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceDescription {
    /// Human-readable receiver name.
    pub friendly_name: String,
    /// Manufacturer display name.
    pub manufacturer: String,
    /// Model display name.
    pub model_name: String,
    /// Persistent UDN, normally a `uuid:` value.
    pub udn: String,
    /// Advertised services.
    pub services: Vec<ServiceDescription>,
}

impl DeviceDescription {
    /// Validates and emits a deterministic UTF-8 UPnP device description.
    pub fn to_xml(&self) -> Result<String, DlnaError> {
        if self.services.len() > MAX_SERVICES {
            return Err(DlnaError::TooLarge("UPnP services"));
        }
        for (value, field) in [
            (&self.friendly_name, "friendly name"),
            (&self.manufacturer, "manufacturer"),
            (&self.model_name, "model name"),
            (&self.udn, "UDN"),
        ] {
            validate_field(value, field)?;
        }
        let mut xml = "<?xml version=\"1.0\" encoding=\"utf-8\"?>".to_owned()
            + "<root xmlns=\"urn:schemas-upnp-org:device-1-0\"><specVersion><major>1</major><minor>0</minor></specVersion><device>";
        push_tag(&mut xml, "friendlyName", &self.friendly_name);
        push_tag(&mut xml, "manufacturer", &self.manufacturer);
        push_tag(&mut xml, "modelName", &self.model_name);
        push_tag(&mut xml, "UDN", &self.udn);
        xml.push_str("<serviceList>");
        for service in &self.services {
            for (value, field) in [
                (&service.service_type, "service type"),
                (&service.service_id, "service id"),
                (&service.control_url, "control URL"),
                (&service.event_sub_url, "event URL"),
                (&service.scpd_url, "SCPD URL"),
            ] {
                validate_field(value, field)?;
            }
            xml.push_str("<service>");
            push_tag(&mut xml, "serviceType", &service.service_type);
            push_tag(&mut xml, "serviceId", &service.service_id);
            push_tag(&mut xml, "controlURL", &service.control_url);
            push_tag(&mut xml, "eventSubURL", &service.event_sub_url);
            push_tag(&mut xml, "SCPDURL", &service.scpd_url);
            xml.push_str("</service>");
        }
        xml.push_str("</serviceList></device></root>");
        Ok(xml)
    }
}

/// Transport states exposed by the bounded AVTransport implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportState {
    /// No URI has been selected.
    NoMediaPresent,
    /// A URI is loaded but not playing.
    Stopped,
    /// Media is actively playing.
    Playing,
    /// Media is paused at the current position.
    PausedPlayback,
}

impl TransportState {
    fn as_upnp(self) -> &'static str {
        match self {
            Self::NoMediaPresent => "NO_MEDIA_PRESENT",
            Self::Stopped => "STOPPED",
            Self::Playing => "PLAYING",
            Self::PausedPlayback => "PAUSED_PLAYBACK",
        }
    }
}

/// Mutable state owned by one MediaRenderer instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaRendererState {
    /// Current AVTransport state.
    pub transport_state: TransportState,
    /// Current media URL, if one was selected.
    pub current_uri: String,
    /// Current DIDL-Lite metadata, retained as bounded XML text.
    pub current_uri_metadata: String,
    /// Relative playback position in whole seconds.
    pub position_seconds: u64,
    /// Optional track duration in whole seconds.
    pub duration_seconds: Option<u64>,
    /// Master volume in the UPnP 0..100 range.
    pub volume: u8,
}

impl Default for MediaRendererState {
    fn default() -> Self {
        Self {
            transport_state: TransportState::NoMediaPresent,
            current_uri: String::new(),
            current_uri_metadata: String::new(),
            position_seconds: 0,
            duration_seconds: None,
            volume: 50,
        }
    }
}

/// One inclusive byte range for an HTTP 206 response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteRange {
    /// Inclusive first byte.
    pub start: usize,
    /// Inclusive last byte.
    pub end: usize,
}

impl ByteRange {
    /// Parses one RFC 7233-style `bytes=start-end` or `bytes=-suffix` range.
    /// Multiple ranges and unsatisfiable ranges are rejected before allocation.
    pub fn parse(value: &str, total_length: usize) -> Result<Self, DlnaError> {
        if total_length == 0 {
            return Err(DlnaError::Invalid("HTTP range"));
        }
        let value = value.trim();
        let Some(spec) = value.strip_prefix("bytes=") else {
            return Err(DlnaError::Invalid("HTTP range"));
        };
        if spec.is_empty() || spec.contains(',') {
            return Err(DlnaError::Invalid("HTTP range"));
        }
        let (first, last) = spec
            .split_once('-')
            .ok_or(DlnaError::Invalid("HTTP range"))?;
        if first.is_empty() {
            let suffix = last
                .parse::<usize>()
                .map_err(|_| DlnaError::Invalid("HTTP range"))?;
            if suffix == 0 {
                return Err(DlnaError::Invalid("HTTP range"));
            }
            let start = total_length.saturating_sub(suffix);
            return Ok(Self {
                start,
                end: total_length - 1,
            });
        }
        let start = first
            .parse::<usize>()
            .map_err(|_| DlnaError::Invalid("HTTP range"))?;
        if start >= total_length {
            return Err(DlnaError::Invalid("HTTP range"));
        }
        let end = if last.is_empty() {
            total_length - 1
        } else {
            last.parse::<usize>()
                .map_err(|_| DlnaError::Invalid("HTTP range"))?
                .min(total_length - 1)
        };
        if end < start {
            return Err(DlnaError::Invalid("HTTP range"));
        }
        Ok(Self { start, end })
    }
}

/// A bounded in-memory HTTP resource exposed by the caller-owned server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaResource {
    /// Origin-form path used to request the resource.
    pub path: String,
    /// MIME type returned in `Content-Type`.
    pub content_type: String,
    /// DLNA protocolInfo value returned in `contentFeatures.dlna.org`.
    pub protocol_info: String,
    /// Resource bytes. A production server may replace this helper with a
    /// streaming source while retaining the same range policy.
    pub body: Vec<u8>,
}

impl MediaResource {
    /// Creates a bounded resource descriptor and rejects header injection.
    pub fn new(
        path: impl Into<String>,
        content_type: impl Into<String>,
        protocol_info: impl Into<String>,
        body: Vec<u8>,
    ) -> Result<Self, DlnaError> {
        let resource = Self {
            path: path.into(),
            content_type: content_type.into(),
            protocol_info: protocol_info.into(),
            body,
        };
        if !resource.path.starts_with('/') {
            return Err(DlnaError::InvalidField("media path"));
        }
        for (value, field) in [
            (&resource.path, "media path"),
            (&resource.content_type, "media content type"),
            (&resource.protocol_info, "media protocol info"),
        ] {
            validate_field(value, field)?;
        }
        if resource.body.is_empty() {
            return Err(DlnaError::InvalidField("media body"));
        }
        if resource.body.len() > MAX_MEDIA_RESOURCE_BYTES {
            return Err(DlnaError::TooLarge("media resource"));
        }
        Ok(resource)
    }
}

/// One inclusive byte range for an explicitly registered file resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileByteRange {
    start: u64,
    end: u64,
}

impl FileByteRange {
    fn parse(value: &str, total_length: u64) -> Result<Self, DlnaError> {
        if total_length == 0 {
            return Err(DlnaError::Invalid("HTTP range"));
        }
        let value = value.trim();
        let Some(spec) = value.strip_prefix("bytes=") else {
            return Err(DlnaError::Invalid("HTTP range"));
        };
        if spec.is_empty() || spec.contains(',') {
            return Err(DlnaError::Invalid("HTTP range"));
        }
        let (first, last) = spec
            .split_once('-')
            .ok_or(DlnaError::Invalid("HTTP range"))?;
        if first.is_empty() {
            let suffix = last
                .parse::<u64>()
                .map_err(|_| DlnaError::Invalid("HTTP range"))?;
            if suffix == 0 {
                return Err(DlnaError::Invalid("HTTP range"));
            }
            return Ok(Self {
                start: total_length.saturating_sub(suffix),
                end: total_length - 1,
            });
        }
        let start = first
            .parse::<u64>()
            .map_err(|_| DlnaError::Invalid("HTTP range"))?;
        if start >= total_length {
            return Err(DlnaError::Invalid("HTTP range"));
        }
        let end = if last.is_empty() {
            total_length - 1
        } else {
            last.parse::<u64>()
                .map_err(|_| DlnaError::Invalid("HTTP range"))?
                .min(total_length - 1)
        };
        if end < start {
            return Err(DlnaError::Invalid("HTTP range"));
        }
        Ok(Self { start, end })
    }
}

/// Explicit file-backed media metadata for the streaming TCP adapter.
///
/// The caller chooses and registers the file; the HTTP handler never resolves
/// a URL, accepts a path from a request, or walks a directory. File bytes are
/// copied in bounded chunks and are not retained in the renderer state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileMediaResource {
    /// Origin-form path used to request the resource.
    pub path: String,
    /// MIME type returned in `Content-Type`.
    pub content_type: String,
    /// DLNA protocolInfo value returned in `contentFeatures.dlna.org`.
    pub protocol_info: String,
    /// Canonical file selected by the caller.
    pub file_path: PathBuf,
    /// File length captured when the resource is registered.
    pub length: u64,
}

impl FileMediaResource {
    /// Creates a bounded descriptor after canonicalizing one regular file.
    pub fn new(
        path: impl Into<String>,
        content_type: impl Into<String>,
        protocol_info: impl Into<String>,
        file_path: impl Into<PathBuf>,
    ) -> Result<Self, DlnaError> {
        let file_path = file_path.into();
        let metadata =
            std::fs::metadata(&file_path).map_err(|error| DlnaError::Io(error.kind()))?;
        if !metadata.is_file() {
            return Err(DlnaError::InvalidField("media file"));
        }
        let length = metadata.len();
        if length == 0 {
            return Err(DlnaError::InvalidField("media file"));
        }
        if length > MAX_MEDIA_FILE_BYTES {
            return Err(DlnaError::TooLarge("media file"));
        }
        let resource = Self {
            path: path.into(),
            content_type: content_type.into(),
            protocol_info: protocol_info.into(),
            file_path: file_path
                .canonicalize()
                .map_err(|error| DlnaError::Io(error.kind()))?,
            length,
        };
        if !resource.path.starts_with('/') {
            return Err(DlnaError::InvalidField("media path"));
        }
        for (value, field) in [
            (&resource.path, "media path"),
            (&resource.content_type, "media content type"),
            (&resource.protocol_info, "media protocol info"),
        ] {
            validate_field(value, field)?;
        }
        Ok(resource)
    }
}

/// One resource entry in a generated DIDL-Lite item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DidlResource {
    /// Absolute or origin-form media URI.
    pub uri: String,
    /// DLNA protocolInfo value.
    pub protocol_info: String,
    /// Optional `HH:MM:SS` duration.
    pub duration: Option<String>,
    /// Optional byte size.
    pub size: Option<usize>,
}

/// A bounded DIDL-Lite item for `CurrentURIMetaData`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DidlLiteItem {
    /// Stable DIDL object identifier.
    pub id: String,
    /// Parent container identifier.
    pub parent_id: String,
    /// Human-readable title.
    pub title: String,
    /// UPnP class, such as `object.item.videoItem`.
    pub class_name: String,
    /// One or more resources.
    pub resources: Vec<DidlResource>,
}

impl DidlLiteItem {
    /// Emits one deterministic DIDL-Lite document after validating bounds.
    pub fn to_xml(&self) -> Result<String, DlnaError> {
        if self.resources.is_empty() || self.resources.len() > MAX_DIDL_ITEMS {
            return Err(DlnaError::InvalidField("DIDL resources"));
        }
        for (value, field) in [
            (&self.id, "DIDL id"),
            (&self.parent_id, "DIDL parent id"),
            (&self.title, "DIDL title"),
            (&self.class_name, "DIDL class"),
        ] {
            validate_field(value, field)?;
        }
        let mut xml = "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\"><item restricted=\"1\"".to_string();
        xml.push_str(" id=\"");
        xml.push_str(&xml_escape(&self.id));
        xml.push_str("\" parentID=\"");
        xml.push_str(&xml_escape(&self.parent_id));
        xml.push_str("\">");
        xml.push_str("<dc:title>");
        xml.push_str(&xml_escape(&self.title));
        xml.push_str("</dc:title><upnp:class>");
        xml.push_str(&xml_escape(&self.class_name));
        xml.push_str("</upnp:class>");
        for resource in &self.resources {
            for (value, field) in [
                (&resource.uri, "DIDL resource URI"),
                (&resource.protocol_info, "DIDL protocol info"),
            ] {
                validate_field(value, field)?;
            }
            xml.push_str("<res protocolInfo=\"");
            xml.push_str(&xml_escape(&resource.protocol_info));
            if let Some(duration) = &resource.duration {
                validate_field(duration, "DIDL duration")?;
                xml.push_str(" duration=\"");
                xml.push_str(&xml_escape(duration));
                xml.push('"');
            }
            if let Some(size) = resource.size {
                xml.push_str(" size=\"");
                xml.push_str(&size.to_string());
                xml.push('"');
            }
            xml.push('>');
            xml.push_str(&xml_escape(&resource.uri));
            xml.push_str("</res>");
        }
        xml.push_str("</item></DIDL-Lite>");
        if xml.len() > MAX_SOAP_ARGUMENT_BYTES {
            return Err(DlnaError::TooLarge("DIDL metadata"));
        }
        Ok(xml)
    }
}

/// One validated GENA subscription returned to the caller-owned HTTP layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenaSubscription {
    /// Deterministic local subscription identifier.
    pub sid: String,
    /// Callback URL supplied by the subscriber.
    pub callback_url: String,
    /// UPnP service type being observed.
    pub service_type: String,
    /// Lease duration in seconds.
    pub timeout_seconds: u32,
    /// Next event sequence number.
    pub next_sequence: u32,
    expires_at: Instant,
}

/// One event notification that a caller-owned HTTP client can POST to a
/// subscriber callback URL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenaEvent {
    /// Subscription receiving the event.
    pub sid: String,
    /// Callback URL to which the event should be sent.
    pub callback_url: String,
    /// UPnP service type of the event body.
    pub service_type: String,
    /// Monotonic event sequence for this subscription.
    pub sequence: u32,
    /// XML property-set body.
    pub body: String,
}

impl GenaEvent {
    /// Encodes this event as one bounded HTTP `NOTIFY` request.
    ///
    /// The returned bytes are ready for a caller-owned TCP client to send to
    /// the validated callback authority. TLS connection setup, DNS, retries,
    /// and response parsing remain outside the protocol crate.
    pub fn encode_http_notify(&self) -> Result<Vec<u8>, DlnaError> {
        validate_callback_url(&self.callback_url)?;
        validate_service_type(&self.service_type)?;
        validate_field(&self.sid, "GENA SID")?;
        if self.sequence > i32::MAX as u32 {
            return Err(DlnaError::Invalid("GENA sequence"));
        }
        if self.body.is_empty() || self.body.len() > MAX_SOAP_ARGUMENT_BYTES {
            return Err(DlnaError::TooLarge("GENA event"));
        }
        let (host, target) = callback_authority_and_target(&self.callback_url)?;
        let mut headers = BTreeMap::new();
        headers.insert("content-length".to_string(), self.body.len().to_string());
        headers.insert(
            "content-type".to_string(),
            "text/xml; charset=\"utf-8\"".to_string(),
        );
        headers.insert("host".to_string(), host);
        headers.insert("nt".to_string(), "upnp:event".to_string());
        headers.insert("nts".to_string(), "upnp:propchange".to_string());
        headers.insert("seq".to_string(), self.sequence.to_string());
        headers.insert("sid".to_string(), self.sid.clone());

        let mut output = format!("NOTIFY {target} HTTP/1.1\r\n").into_bytes();
        for (name, value) in headers {
            validate_field(&name, "GENA header name")?;
            validate_field(&value, "GENA header value")?;
            output.extend_from_slice(name.as_bytes());
            output.extend_from_slice(b": ");
            output.extend_from_slice(value.as_bytes());
            output.extend_from_slice(b"\r\n");
        }
        output.extend_from_slice(b"\r\n");
        output.extend_from_slice(self.body.as_bytes());
        if output.len() > MAX_HTTP_BYTES {
            return Err(DlnaError::TooLarge("GENA request"));
        }
        Ok(output)
    }
}

/// Result returned by the bounded HTTP client used for GENA callbacks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenaCallbackResponse {
    /// HTTP status returned by the subscriber callback endpoint.
    pub status: u16,
    /// HTTP reason phrase returned by the subscriber.
    pub reason: String,
    /// Lowercase response headers.
    pub headers: BTreeMap<String, String>,
    /// Bounded response body.
    pub body: Vec<u8>,
}

/// A caller-owned, synchronous GENA callback client.
///
/// This client intentionally supports plain HTTP only. HTTPS callbacks are
/// rejected rather than silently sending cleartext or pretending to provide
/// TLS. A platform or daemon may wrap the same encoded request in a vetted TLS
/// implementation and retain the same response bounds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenaCallbackClient {
    timeout: Duration,
}

impl GenaCallbackClient {
    /// Creates a client with one connect/read/write timeout for each callback.
    pub fn new(timeout: Duration) -> Result<Self, DlnaError> {
        if timeout.is_zero() {
            return Err(DlnaError::Invalid("GENA callback timeout"));
        }
        Ok(Self { timeout })
    }

    /// Sends one encoded event to its validated callback and parses one HTTP response.
    pub fn send(&self, event: &GenaEvent) -> Result<GenaCallbackResponse, DlnaError> {
        let request = event.encode_http_notify()?;
        let (scheme, _rest) = event
            .callback_url
            .split_once("://")
            .ok_or(DlnaError::Invalid("GENA callback URL"))?;
        if !scheme.eq_ignore_ascii_case("http") {
            return Err(DlnaError::Invalid("GENA HTTPS requires TLS"));
        }
        let (authority, _) = callback_authority_and_target(&event.callback_url)?;
        let address = callback_socket_address(&authority)?;
        let mut stream = connect_callback(address, self.timeout)?;
        stream
            .set_read_timeout(Some(self.timeout))
            .and_then(|_| stream.set_write_timeout(Some(self.timeout)))
            .map_err(|error| DlnaError::Io(error.kind()))?;
        stream
            .write_all(&request)
            .and_then(|_| stream.flush())
            .map_err(|error| DlnaError::Io(error.kind()))?;
        let response = read_http_response(&mut stream)?;
        Ok(GenaCallbackResponse {
            status: response.status,
            reason: response.reason,
            headers: response.headers,
            body: response.body,
        })
    }
}

/// Bounded GENA subscription registry with deterministic IDs and event bodies.
#[derive(Clone, Debug, Default)]
pub struct GenaRegistry {
    subscriptions: BTreeMap<String, GenaSubscription>,
    next_id: u64,
}

impl GenaRegistry {
    /// Registers a new subscription and returns a 1..86400 second lease.
    pub fn subscribe(
        &mut self,
        service_type: &str,
        callback_url: &str,
        timeout: Option<&str>,
    ) -> Result<GenaSubscription, DlnaError> {
        self.subscribe_at(service_type, callback_url, timeout, Instant::now())
    }

    fn subscribe_at(
        &mut self,
        service_type: &str,
        callback_url: &str,
        timeout: Option<&str>,
        now: Instant,
    ) -> Result<GenaSubscription, DlnaError> {
        validate_service_type(service_type)?;
        validate_callback_url(callback_url)?;
        self.expire_at(now);
        if self.subscriptions.len() >= MAX_GENA_SUBSCRIPTIONS {
            return Err(DlnaError::TooLarge("GENA subscriptions"));
        }
        let timeout_seconds = parse_gena_timeout(timeout)?;
        self.next_id = self.next_id.saturating_add(1);
        let subscription = GenaSubscription {
            sid: format!("uuid:frameark-sub-{}", self.next_id),
            callback_url: callback_url.to_string(),
            service_type: service_type.to_string(),
            timeout_seconds,
            next_sequence: 0,
            expires_at: now + Duration::from_secs(timeout_seconds as u64),
        };
        self.subscriptions
            .insert(subscription.sid.clone(), subscription.clone());
        Ok(subscription)
    }

    /// Renews an existing subscription without changing its callback.
    pub fn renew(
        &mut self,
        sid: &str,
        timeout: Option<&str>,
    ) -> Result<GenaSubscription, DlnaError> {
        self.renew_at(sid, timeout, Instant::now())
    }

    fn renew_at(
        &mut self,
        sid: &str,
        timeout: Option<&str>,
        now: Instant,
    ) -> Result<GenaSubscription, DlnaError> {
        self.expire_at(now);
        let subscription = self
            .subscriptions
            .get_mut(sid)
            .ok_or(DlnaError::Invalid("GENA SID"))?;
        let timeout_seconds = parse_gena_timeout(timeout)?;
        subscription.timeout_seconds = timeout_seconds;
        subscription.expires_at = now + Duration::from_secs(timeout_seconds as u64);
        Ok(subscription.clone())
    }

    /// Removes a subscription. Unknown SIDs are rejected rather than ignored.
    pub fn unsubscribe(&mut self, sid: &str) -> Result<(), DlnaError> {
        self.expire();
        if self.subscriptions.remove(sid).is_none() {
            return Err(DlnaError::Invalid("GENA SID"));
        }
        Ok(())
    }

    /// Generates one event per matching subscription and advances each sequence.
    pub fn publish(
        &mut self,
        service_type: &str,
        properties: &BTreeMap<String, String>,
    ) -> Result<Vec<GenaEvent>, DlnaError> {
        self.publish_at(service_type, properties, Instant::now())
    }

    fn publish_at(
        &mut self,
        service_type: &str,
        properties: &BTreeMap<String, String>,
        now: Instant,
    ) -> Result<Vec<GenaEvent>, DlnaError> {
        validate_service_type(service_type)?;
        self.expire_at(now);
        let body = gena_property_set(properties)?;
        let mut events = Vec::new();
        for subscription in self.subscriptions.values_mut() {
            if subscription.service_type != service_type {
                continue;
            }
            subscription.next_sequence = subscription.next_sequence.wrapping_add(1);
            events.push(GenaEvent {
                sid: subscription.sid.clone(),
                callback_url: subscription.callback_url.clone(),
                service_type: service_type.to_string(),
                sequence: subscription.next_sequence,
                body: body.clone(),
            });
        }
        Ok(events)
    }

    /// Returns the current number of active subscriptions.
    pub fn len(&self) -> usize {
        let now = Instant::now();
        self.subscriptions
            .values()
            .filter(|subscription| subscription.expires_at > now)
            .count()
    }

    /// Returns whether no active subscriptions are registered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes subscriptions whose monotonic lease deadline has passed.
    ///
    /// The caller may invoke this from a daemon timer. `publish` also calls it
    /// immediately before creating events, so expired callbacks never receive
    /// a new notification even when the host does not run a dedicated timer.
    pub fn expire(&mut self) -> usize {
        self.expire_at(Instant::now())
    }

    /// Removes subscriptions expired at an explicit instant for deterministic
    /// tests and platform schedulers.
    pub fn expire_at(&mut self, now: Instant) -> usize {
        let before = self.subscriptions.len();
        self.subscriptions
            .retain(|_, subscription| subscription.expires_at > now);
        before.saturating_sub(self.subscriptions.len())
    }
}

/// Bounded SOAP/HTTP MediaRenderer handler.
///
/// This type parses complete requests and applies a single request to explicit
/// Rust state. The [`MediaRendererTcpServer`] adapter below adds a bounded
/// synchronous socket boundary without moving platform rendering into this
/// protocol crate. It implements the basic AVTransport, RenderingControl, and
/// ConnectionManager actions needed for a first M3 vertical slice.
pub struct MediaRendererHttpService {
    description: DeviceDescription,
    device_description_path: String,
    state: MediaRendererState,
    resources: BTreeMap<String, MediaResource>,
    file_resources: BTreeMap<String, FileMediaResource>,
    gena: GenaRegistry,
    pending_events: Vec<GenaEvent>,
}

/// A bounded synchronous TCP adapter for one MediaRenderer service.
///
/// The adapter handles one connection per [`serve_once`](Self::serve_once)
/// call, applies a read timeout, caps the complete HTTP request through
/// [`HttpRequest::parse`], and closes the connection after one response. A
/// production daemon can call it from an accept loop or replace it with an
/// asynchronous socket layer while reusing the same handler and state.
pub struct MediaRendererTcpServer {
    listener: TcpListener,
    service: MediaRendererHttpService,
    read_timeout: Duration,
}

impl MediaRendererTcpServer {
    /// Binds a listener and rejects a zero read timeout before accepting work.
    pub fn bind(
        address: SocketAddr,
        service: MediaRendererHttpService,
        read_timeout: Duration,
    ) -> Result<Self, DlnaError> {
        if read_timeout.is_zero() {
            return Err(DlnaError::Invalid("HTTP read timeout"));
        }
        let listener = TcpListener::bind(address).map_err(|error| DlnaError::Io(error.kind()))?;
        Ok(Self {
            listener,
            service,
            read_timeout,
        })
    }

    /// Returns the OS-assigned listener address, including an ephemeral port.
    pub fn local_addr(&self) -> Result<SocketAddr, DlnaError> {
        self.listener
            .local_addr()
            .map_err(|error| DlnaError::Io(error.kind()))
    }

    /// Provides read-only access to the handler state between connections.
    pub fn service(&self) -> &MediaRendererHttpService {
        &self.service
    }

    /// Provides mutable access for registering resources or draining events.
    pub fn service_mut(&mut self) -> &mut MediaRendererHttpService {
        &mut self.service
    }

    /// Accepts one bounded request, sends one response, and closes the stream.
    pub fn serve_once(&mut self) -> Result<SocketAddr, DlnaError> {
        let (mut stream, peer) = self
            .listener
            .accept()
            .map_err(|error| DlnaError::Io(error.kind()))?;
        stream
            .set_read_timeout(Some(self.read_timeout))
            .map_err(|error| DlnaError::Io(error.kind()))?;
        let request = read_http_request(&mut stream)?;
        if self.service.stream_file_response(&request, &mut stream)? {
            return Ok(peer);
        }
        let response = self.service.handle(&request);
        let encoded = response.encode()?;
        stream
            .write_all(&encoded)
            .and_then(|_| stream.flush())
            .map_err(|error| DlnaError::Io(error.kind()))?;
        Ok(peer)
    }

    /// Accepts a bounded batch of sequential requests and returns their peers.
    ///
    /// The loop intentionally has an explicit request budget and remains
    /// synchronous; callers that need cancellation or concurrent clients must
    /// provide their own bounded scheduler around this handler.
    pub fn serve_requests(&mut self, max_requests: usize) -> Result<Vec<SocketAddr>, DlnaError> {
        if max_requests == 0 || max_requests > MAX_TCP_REQUESTS_PER_RUN {
            return Err(DlnaError::Invalid("HTTP request batch size"));
        }
        let mut peers = Vec::with_capacity(max_requests);
        for _ in 0..max_requests {
            peers.push(self.serve_once()?);
        }
        Ok(peers)
    }
}

fn write_file_headers(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    protocol_info: &str,
    content_length: u64,
    content_range: Option<String>,
) -> Result<(), DlnaError> {
    let mut headers = vec![
        ("content-length", content_length.to_string()),
        ("connection", "close".to_string()),
        ("content-type", content_type.to_string()),
        ("accept-ranges", "bytes".to_string()),
        ("contentfeatures.dlna.org", protocol_info.to_string()),
        ("transfermode.dlna.org", "Streaming".to_string()),
    ];
    if let Some(content_range) = content_range {
        headers.push(("content-range", content_range));
    }
    let mut output = format!("HTTP/1.1 {status} {reason}\r\n").into_bytes();
    for (name, value) in headers {
        validate_field(name, "HTTP response header name")?;
        validate_field(&value, "HTTP response header value")?;
        output.extend_from_slice(name.as_bytes());
        output.extend_from_slice(b": ");
        output.extend_from_slice(value.as_bytes());
        output.extend_from_slice(b"\r\n");
    }
    output.extend_from_slice(b"\r\n");
    stream
        .write_all(&output)
        .map_err(|error| DlnaError::Io(error.kind()))
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, DlnaError> {
    let mut bytes = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    loop {
        if bytes.len() >= MAX_HTTP_BYTES {
            return Err(DlnaError::TooLarge("HTTP request"));
        }
        match HttpRequest::parse(&bytes) {
            Ok(request) => return Ok(request),
            Err(error) if request_may_be_incomplete(&bytes, &error) => {}
            Err(error) => return Err(error),
        }
        let read = stream
            .read(&mut chunk)
            .map_err(|error| DlnaError::Io(error.kind()))?;
        if read == 0 {
            return Err(DlnaError::Invalid("HTTP request"));
        }
        let remaining = MAX_HTTP_BYTES - bytes.len();
        bytes.extend_from_slice(&chunk[..read.min(remaining)]);
    }
}

fn callback_socket_address(authority: &str) -> Result<SocketAddr, DlnaError> {
    if authority.contains('@') {
        return Err(DlnaError::Invalid("GENA callback authority"));
    }
    let port = if authority.starts_with('[') {
        let end = authority
            .find(']')
            .ok_or(DlnaError::Invalid("GENA callback authority"))?;
        let suffix = authority
            .get(end + 1..)
            .ok_or(DlnaError::Invalid("GENA callback authority"))?;
        suffix
            .strip_prefix(':')
            .ok_or(DlnaError::Invalid("GENA callback port"))?
    } else {
        let (host, port) = authority
            .rsplit_once(':')
            .ok_or(DlnaError::Invalid("GENA callback port"))?;
        if host.is_empty() || host.contains(':') {
            return Err(DlnaError::Invalid("GENA callback authority"));
        }
        port
    };
    let port = port
        .parse::<u16>()
        .map_err(|_| DlnaError::Invalid("GENA callback port"))?;
    if port == 0 {
        return Err(DlnaError::Invalid("GENA callback port"));
    }
    authority
        .to_socket_addrs()
        .map_err(|error| DlnaError::Io(error.kind()))?
        .next()
        .ok_or(DlnaError::Invalid("GENA callback address"))
}

fn connect_callback(address: SocketAddr, timeout: Duration) -> Result<TcpStream, DlnaError> {
    TcpStream::connect_timeout(&address, timeout).map_err(|error| DlnaError::Io(error.kind()))
}

fn read_http_response(stream: &mut TcpStream) -> Result<HttpResponse, DlnaError> {
    let mut bytes = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 4096];
    loop {
        if bytes.len() >= MAX_HTTP_BYTES {
            return Err(DlnaError::TooLarge("HTTP response"));
        }
        match HttpResponse::parse(&bytes) {
            Ok(response) => return Ok(response),
            Err(error) if response_may_be_incomplete(&bytes, &error) => {}
            Err(error) => return Err(error),
        }
        let read = stream
            .read(&mut chunk)
            .map_err(|error| DlnaError::Io(error.kind()))?;
        if read == 0 {
            return Err(DlnaError::Invalid("HTTP response"));
        }
        let remaining = MAX_HTTP_BYTES - bytes.len();
        bytes.extend_from_slice(&chunk[..read.min(remaining)]);
    }
}

fn response_may_be_incomplete(bytes: &[u8], error: &DlnaError) -> bool {
    if *error == DlnaError::Invalid("HTTP terminator") {
        return true;
    }
    if *error != DlnaError::Invalid("HTTP content length") {
        return false;
    }
    let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
        return true;
    };
    let Ok(header_text) = std::str::from_utf8(&bytes[..header_end]) else {
        return false;
    };
    let Some(content_length) = header_text
        .split("\r\n")
        .skip(1)
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
        })
        .flatten()
    else {
        return false;
    };
    bytes.len() - header_end - 4 < content_length
}

fn request_may_be_incomplete(bytes: &[u8], error: &DlnaError) -> bool {
    if *error == DlnaError::Invalid("HTTP terminator") {
        return true;
    }
    if *error != DlnaError::Invalid("HTTP content length") {
        return false;
    }
    let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
        return true;
    };
    let Ok(header_text) = std::str::from_utf8(&bytes[..header_end]) else {
        return false;
    };
    let Some(content_length) = header_text
        .split("\r\n")
        .skip(1)
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            (name.eq_ignore_ascii_case("content-length"))
                .then(|| value.trim().parse::<usize>().ok())
        })
        .flatten()
    else {
        return false;
    };
    bytes.len() - header_end - 4 < content_length
}

impl MediaRendererHttpService {
    /// Creates a handler after validating the supplied device description.
    pub fn new(description: DeviceDescription) -> Result<Self, DlnaError> {
        description.to_xml()?;
        Ok(Self {
            description,
            device_description_path: "/device.xml".to_string(),
            state: MediaRendererState::default(),
            resources: BTreeMap::new(),
            file_resources: BTreeMap::new(),
            gena: GenaRegistry::default(),
            pending_events: Vec::new(),
        })
    }

    /// Returns a read-only snapshot of renderer state for platform adapters.
    pub fn state(&self) -> &MediaRendererState {
        &self.state
    }

    /// Returns mutable renderer state for a platform-owned playback backend.
    pub fn state_mut(&mut self) -> &mut MediaRendererState {
        &mut self.state
    }

    /// Registers or replaces one bounded media resource for HTTP GET/Range.
    pub fn register_media_resource(&mut self, resource: MediaResource) -> Result<(), DlnaError> {
        if !self.resources.contains_key(&resource.path)
            && self.resources.len() + self.file_resources.len() >= MAX_MEDIA_RESOURCES
        {
            return Err(DlnaError::TooLarge("media resources"));
        }
        self.resources.insert(resource.path.clone(), resource);
        Ok(())
    }

    /// Registers or replaces one explicit file-backed resource.
    pub fn register_file_media_resource(
        &mut self,
        resource: FileMediaResource,
    ) -> Result<(), DlnaError> {
        if !self.file_resources.contains_key(&resource.path)
            && self.resources.len() + self.file_resources.len() >= MAX_MEDIA_RESOURCES
        {
            return Err(DlnaError::TooLarge("media resources"));
        }
        self.file_resources.insert(resource.path.clone(), resource);
        Ok(())
    }

    /// Returns the number of active GENA subscriptions.
    pub fn subscription_count(&self) -> usize {
        self.gena.len()
    }

    /// Drains events generated by state changes for caller-owned callback I/O.
    /// Expired or unsubscribed events are removed before ownership is transferred.
    /// Callers must re-check authorization/lease policy if they delay delivery.
    pub fn drain_events(&mut self) -> Vec<GenaEvent> {
        self.expire_subscriptions();
        std::mem::take(&mut self.pending_events)
    }

    /// Reaps expired leases and pending events. May be called by a host timer.
    /// Returns the number of subscription records removed.
    pub fn expire_subscriptions(&mut self) -> usize {
        let removed = self.gena.expire();
        self.discard_inactive_events();
        removed
    }

    fn stream_file_response(
        &self,
        request: &HttpRequest,
        stream: &mut TcpStream,
    ) -> Result<bool, DlnaError> {
        if request.method != "GET" {
            return Ok(false);
        }
        let path = request.target.split('?').next().unwrap_or(&request.target);
        let Some(resource) = self.file_resources.get(path) else {
            return Ok(false);
        };
        let range = match request.header("range") {
            Some(value) => match FileByteRange::parse(value, resource.length) {
                Ok(range) => Some(range),
                Err(_) => {
                    write_file_headers(
                        stream,
                        416,
                        "Range Not Satisfiable",
                        &resource.content_type,
                        &resource.protocol_info,
                        0,
                        Some(format!("bytes */{}", resource.length)),
                    )?;
                    return Ok(true);
                }
            },
            None => None,
        };
        let (start, end, status, reason, content_range) = if let Some(range) = range {
            (
                range.start,
                range.end,
                206,
                "Partial Content",
                Some(format!(
                    "bytes {}-{}/{}",
                    range.start, range.end, resource.length
                )),
            )
        } else {
            (0, resource.length - 1, 200, "OK", None)
        };
        let content_length = end
            .checked_sub(start)
            .and_then(|length| length.checked_add(1))
            .ok_or(DlnaError::Invalid("media file range"))?;
        write_file_headers(
            stream,
            status,
            reason,
            &resource.content_type,
            &resource.protocol_info,
            content_length,
            content_range,
        )?;
        let mut file =
            File::open(&resource.file_path).map_err(|error| DlnaError::Io(error.kind()))?;
        let current_length = file
            .metadata()
            .map_err(|error| DlnaError::Io(error.kind()))?
            .len();
        if current_length != resource.length {
            return Err(DlnaError::Invalid("media file changed"));
        }
        file.seek(SeekFrom::Start(start))
            .map_err(|error| DlnaError::Io(error.kind()))?;
        let mut remaining = content_length;
        let mut buffer = vec![0_u8; MAX_MEDIA_STREAM_CHUNK_BYTES];
        while remaining > 0 {
            let requested = remaining.min(buffer.len() as u64) as usize;
            let read = file
                .read(&mut buffer[..requested])
                .map_err(|error| DlnaError::Io(error.kind()))?;
            if read == 0 {
                return Err(DlnaError::Invalid("media file changed"));
            }
            stream
                .write_all(&buffer[..read])
                .map_err(|error| DlnaError::Io(error.kind()))?;
            remaining -= read as u64;
        }
        stream
            .flush()
            .map_err(|error| DlnaError::Io(error.kind()))?;
        Ok(true)
    }

    fn discard_inactive_events(&mut self) {
        self.pending_events
            .retain(|event| self.gena.subscriptions.contains_key(&event.sid));
    }

    /// Applies one parsed request and returns an HTTP response.
    pub fn handle(&mut self, request: &HttpRequest) -> HttpResponse {
        self.expire_subscriptions();
        let path = request.target.split('?').next().unwrap_or(&request.target);
        match request.method.as_str() {
            "GET" => self.handle_get(path, request),
            "POST" => self.handle_post(path, request),
            "SUBSCRIBE" => self.handle_subscribe(path, request),
            "UNSUBSCRIBE" => self.handle_unsubscribe(path, request),
            _ => {
                let mut response = HttpResponse::empty(405, "Method Not Allowed");
                response
                    .headers
                    .insert("allow".to_string(), "GET, POST".to_string());
                response
            }
        }
    }

    fn handle_subscribe(&mut self, path: &str, request: &HttpRequest) -> HttpResponse {
        let Some(service_type) = self.control_service_for_path(path) else {
            return HttpResponse::empty(404, "Not Found");
        };
        let timeout = request.header("timeout");
        let subscription = if let Some(sid) = request.header("sid") {
            if request.header("callback").is_some() || request.header("nt").is_some() {
                return HttpResponse::empty(412, "Precondition Failed");
            }
            match self.gena.renew(sid, timeout) {
                Ok(subscription) => subscription,
                Err(_) => return HttpResponse::empty(412, "Precondition Failed"),
            }
        } else {
            let Some(callback_header) = request.header("callback") else {
                return HttpResponse::empty(412, "Precondition Failed");
            };
            if request.header("nt") != Some("upnp:event") {
                return HttpResponse::empty(412, "Precondition Failed");
            }
            let Some(callback_url) = parse_callback_header(callback_header) else {
                return HttpResponse::empty(412, "Precondition Failed");
            };
            match self.gena.subscribe(&service_type, callback_url, timeout) {
                Ok(subscription) => subscription,
                Err(_) => return HttpResponse::empty(412, "Precondition Failed"),
            }
        };
        if request.header("sid").is_none() {
            self.queue_state_event(&service_type);
        }
        let mut response = HttpResponse::empty(200, "OK");
        response.headers.insert("sid".to_string(), subscription.sid);
        response.headers.insert(
            "timeout".to_string(),
            format!("Second-{}", subscription.timeout_seconds),
        );
        response
    }

    fn handle_unsubscribe(&mut self, path: &str, request: &HttpRequest) -> HttpResponse {
        if self.control_service_for_path(path).is_none() {
            return HttpResponse::empty(404, "Not Found");
        }
        let Some(sid) = request.header("sid") else {
            return HttpResponse::empty(412, "Precondition Failed");
        };
        match self.gena.unsubscribe(sid) {
            Ok(()) => {
                self.discard_inactive_events();
                HttpResponse::empty(200, "OK")
            }
            Err(_) => HttpResponse::empty(412, "Precondition Failed"),
        }
    }

    fn handle_get(&self, path: &str, request: &HttpRequest) -> HttpResponse {
        if let Some(resource) = self.resources.get(path) {
            let range = match request.header("range") {
                Some(value) => match ByteRange::parse(value, resource.body.len()) {
                    Ok(range) => Some(range),
                    Err(_) => {
                        let mut response = HttpResponse::empty(416, "Range Not Satisfiable");
                        response.headers.insert(
                            "content-range".to_string(),
                            format!("bytes */{}", resource.body.len()),
                        );
                        return response;
                    }
                },
                None => None,
            };
            return self.media_response(resource, range);
        }
        if path == self.device_description_path {
            return match self.description.to_xml() {
                Ok(xml) => HttpResponse::xml(200, "OK", xml),
                Err(_) => HttpResponse::empty(500, "Internal Server Error"),
            };
        }
        if let Some(service) = self.scpd_service_for_path(path) {
            return HttpResponse::xml(200, "OK", scpd_xml(service));
        }
        HttpResponse::empty(404, "Not Found")
    }

    fn media_response(&self, resource: &MediaResource, range: Option<ByteRange>) -> HttpResponse {
        let mut response = if let Some(range) = range {
            let body = resource.body[range.start..=range.end].to_vec();
            let mut response = HttpResponse {
                status: 206,
                reason: "Partial Content".to_string(),
                headers: BTreeMap::new(),
                body,
            };
            response.headers.insert(
                "content-range".to_string(),
                format!(
                    "bytes {}-{}/{}",
                    range.start,
                    range.end,
                    resource.body.len()
                ),
            );
            response
        } else {
            HttpResponse {
                status: 200,
                reason: "OK".to_string(),
                headers: BTreeMap::new(),
                body: resource.body.clone(),
            }
        };
        response
            .headers
            .insert("content-type".to_string(), resource.content_type.clone());
        response
            .headers
            .insert("accept-ranges".to_string(), "bytes".to_string());
        response.headers.insert(
            "contentfeatures.dlna.org".to_string(),
            resource.protocol_info.clone(),
        );
        response
            .headers
            .insert("transfermode.dlna.org".to_string(), "Streaming".to_string());
        response
    }

    fn handle_post(&mut self, path: &str, request: &HttpRequest) -> HttpResponse {
        let Some(service) = self.control_service_for_path(path) else {
            return HttpResponse::empty(404, "Not Found");
        };
        let Some(action_header) = request.header("soapaction") else {
            return soap_fault_response(402, "Invalid Args");
        };
        let action = match parse_soap_action(action_header, &request.body) {
            Ok(action) => action,
            Err(_) => return soap_fault_response(402, "Invalid Args"),
        };
        if action.service != service {
            return soap_fault_response(401, "Invalid Action");
        }
        match self.invoke(action.clone()) {
            Ok(body) => {
                if matches!(
                    action.name.as_str(),
                    "SetAVTransportURI" | "Play" | "Pause" | "Stop" | "Seek" | "SetVolume"
                ) {
                    self.queue_state_event(&action.service);
                }
                HttpResponse::xml(200, "OK", body)
            }
            Err(fault) => soap_fault_response(fault.code, fault.description),
        }
    }

    fn queue_state_event(&mut self, service_type: &str) {
        let mut properties = BTreeMap::new();
        match service_type {
            AVTRANSPORT_SERVICE => {
                properties.insert(
                    "TransportState".to_string(),
                    self.state.transport_state.as_upnp().to_string(),
                );
                properties.insert(
                    "CurrentTrackURI".to_string(),
                    self.state.current_uri.clone(),
                );
            }
            RENDERING_CONTROL_SERVICE => {
                properties.insert("Volume".to_string(), self.state.volume.to_string());
            }
            _ => return,
        }
        if let Ok(events) = self.gena.publish(service_type, &properties) {
            let available = MAX_GENA_PENDING_EVENTS.saturating_sub(self.pending_events.len());
            self.pending_events
                .extend(events.into_iter().take(available));
        }
    }

    fn scpd_service_for_path(&self, path: &str) -> Option<&str> {
        self.description
            .services
            .iter()
            .find(|service| service.scpd_url == path)
            .map(|service| service.service_type.as_str())
    }

    fn control_service_for_path(&self, path: &str) -> Option<String> {
        self.description
            .services
            .iter()
            .find(|service| service.control_url == path)
            .map(|service| service.service_type.clone())
    }

    fn invoke(&mut self, action: SoapAction) -> Result<String, SoapFault> {
        match action.service.as_str() {
            AVTRANSPORT_SERVICE => self.invoke_avtransport(&action),
            RENDERING_CONTROL_SERVICE => self.invoke_rendering_control(&action),
            CONNECTION_MANAGER_SERVICE => self.invoke_connection_manager(&action),
            _ => Err(SoapFault::new(401, "Invalid Action")),
        }
    }

    fn invoke_avtransport(&mut self, action: &SoapAction) -> Result<String, SoapFault> {
        match action.name.as_str() {
            "SetAVTransportURI" => {
                require_instance_zero(action)?;
                let uri = action.required_arg("CurrentURI")?;
                validate_media_uri(uri)?;
                self.state.current_uri = uri.to_string();
                self.state.current_uri_metadata = action
                    .arg("CurrentURIMetaData")
                    .unwrap_or_default()
                    .to_string();
                self.state.position_seconds = 0;
                self.state.duration_seconds = metadata_duration(&self.state.current_uri_metadata);
                self.state.transport_state = TransportState::Stopped;
                Ok(soap_response(AVTRANSPORT_SERVICE, &action.name, ""))
            }
            "Play" => {
                require_instance_zero(action)?;
                if action.arg("Speed").unwrap_or("1") != "1" {
                    return Err(SoapFault::new(712, "Play speed not supported"));
                }
                ensure_media_loaded(&self.state)?;
                self.state.transport_state = TransportState::Playing;
                Ok(soap_response(AVTRANSPORT_SERVICE, &action.name, ""))
            }
            "Pause" => {
                require_instance_zero(action)?;
                ensure_media_loaded(&self.state)?;
                self.state.transport_state = TransportState::PausedPlayback;
                Ok(soap_response(AVTRANSPORT_SERVICE, &action.name, ""))
            }
            "Stop" => {
                require_instance_zero(action)?;
                ensure_media_loaded(&self.state)?;
                self.state.transport_state = TransportState::Stopped;
                Ok(soap_response(AVTRANSPORT_SERVICE, &action.name, ""))
            }
            "Seek" => {
                require_instance_zero(action)?;
                ensure_media_loaded(&self.state)?;
                if action.arg("Unit") != Some("REL_TIME") {
                    return Err(SoapFault::new(710, "Seek mode not supported"));
                }
                let target = action.required_arg("Target")?;
                self.state.position_seconds = parse_upnp_time(target)?;
                Ok(soap_response(AVTRANSPORT_SERVICE, &action.name, ""))
            }
            "GetTransportInfo" => {
                require_instance_zero(action)?;
                let arguments = format!(
                    "<CurrentTransportState>{}</CurrentTransportState><CurrentTransportStatus>OK</CurrentTransportStatus><CurrentSpeed>1</CurrentSpeed>",
                    self.state.transport_state.as_upnp()
                );
                Ok(soap_response(AVTRANSPORT_SERVICE, &action.name, &arguments))
            }
            "GetPositionInfo" => {
                require_instance_zero(action)?;
                let duration = self
                    .state
                    .duration_seconds
                    .map(format_upnp_time)
                    .unwrap_or_else(|| "00:00:00".to_string());
                let arguments = format!(
                    "<Track>1</Track><TrackDuration>{duration}</TrackDuration><RelTime>{}</RelTime><AbsTime>NOT_IMPLEMENTED</AbsTime><TrackURI>{}</TrackURI>",
                    format_upnp_time(self.state.position_seconds),
                    xml_escape(&self.state.current_uri)
                );
                Ok(soap_response(AVTRANSPORT_SERVICE, &action.name, &arguments))
            }
            "GetMediaInfo" => {
                require_instance_zero(action)?;
                let arguments = format!(
                    "<NrTracks>{}</NrTracks><MediaDuration>{}</MediaDuration><CurrentURI>{}</CurrentURI><CurrentURIMetaData>{}</CurrentURIMetaData>",
                    if self.state.current_uri.is_empty() {
                        0
                    } else {
                        1
                    },
                    self.state
                        .duration_seconds
                        .map(format_upnp_time)
                        .unwrap_or_else(|| "00:00:00".to_string()),
                    xml_escape(&self.state.current_uri),
                    xml_escape(&self.state.current_uri_metadata)
                );
                Ok(soap_response(AVTRANSPORT_SERVICE, &action.name, &arguments))
            }
            _ => Err(SoapFault::new(401, "Invalid Action")),
        }
    }

    fn invoke_rendering_control(&mut self, action: &SoapAction) -> Result<String, SoapFault> {
        match action.name.as_str() {
            "SetVolume" => {
                require_instance_zero(action)?;
                if action.arg("Channel") != Some("Master") {
                    return Err(SoapFault::new(501, "Action Failed"));
                }
                let volume = action
                    .required_arg("DesiredVolume")?
                    .parse::<u8>()
                    .map_err(|_| SoapFault::new(402, "Invalid Args"))?;
                self.state.volume = volume;
                Ok(soap_response(RENDERING_CONTROL_SERVICE, &action.name, ""))
            }
            "GetVolume" => {
                require_instance_zero(action)?;
                if action.arg("Channel") != Some("Master") {
                    return Err(SoapFault::new(501, "Action Failed"));
                }
                let arguments = format!("<CurrentVolume>{}</CurrentVolume>", self.state.volume);
                Ok(soap_response(
                    RENDERING_CONTROL_SERVICE,
                    &action.name,
                    &arguments,
                ))
            }
            _ => Err(SoapFault::new(401, "Invalid Action")),
        }
    }

    fn invoke_connection_manager(&self, action: &SoapAction) -> Result<String, SoapFault> {
        match action.name.as_str() {
            "GetProtocolInfo" => {
                let arguments =
                    "<Source></Source><Sink>http-get:*:video/mp4:*,http-get:*:audio/L16:*</Sink>";
                Ok(soap_response(
                    CONNECTION_MANAGER_SERVICE,
                    &action.name,
                    arguments,
                ))
            }
            "GetCurrentConnectionIDs" => Ok(soap_response(
                CONNECTION_MANAGER_SERVICE,
                &action.name,
                "<ConnectionIDs>0</ConnectionIDs>",
            )),
            "GetCurrentConnectionInfo" => {
                let arguments = "<RcsID>0</RcsID><AVTransportID>0</AVTransportID><ProtocolInfo></ProtocolInfo><PeerConnectionManager></PeerConnectionManager><PeerConnectionID>-1</PeerConnectionID><Direction>Output</Direction><Status>OK</Status>";
                Ok(soap_response(
                    CONNECTION_MANAGER_SERVICE,
                    &action.name,
                    arguments,
                ))
            }
            _ => Err(SoapFault::new(401, "Invalid Action")),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SoapAction {
    service: String,
    name: String,
    args: BTreeMap<String, String>,
}

impl SoapAction {
    fn arg(&self, name: &str) -> Option<&str> {
        self.args.get(name).map(String::as_str)
    }

    fn required_arg(&self, name: &'static str) -> Result<&str, SoapFault> {
        self.arg(name)
            .ok_or_else(|| SoapFault::new(402, "Invalid Args"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SoapFault {
    code: u16,
    description: &'static str,
}

impl SoapFault {
    const fn new(code: u16, description: &'static str) -> Self {
        Self { code, description }
    }
}

fn parse_soap_action(header: &str, body: &[u8]) -> Result<SoapAction, DlnaError> {
    let value = header.trim().trim_matches('"');
    let (service, name) = value
        .rsplit_once('#')
        .ok_or(DlnaError::Invalid("SOAPAction"))?;
    if !matches!(
        service,
        AVTRANSPORT_SERVICE | RENDERING_CONTROL_SERVICE | CONNECTION_MANAGER_SERVICE
    ) || name.is_empty()
    {
        return Err(DlnaError::Invalid("SOAPAction"));
    }
    validate_field(service, "SOAP service")?;
    validate_field(name, "SOAP action")?;
    let body = std::str::from_utf8(body).map_err(|_| DlnaError::Invalid("SOAP UTF-8"))?;
    if !body.contains("Envelope") || !body.contains("Body") || !body.contains(name) {
        return Err(DlnaError::Invalid("SOAP envelope"));
    }
    let mut args = BTreeMap::new();
    for arg in [
        "InstanceID",
        "CurrentURI",
        "CurrentURIMetaData",
        "Speed",
        "Unit",
        "Target",
        "Channel",
        "DesiredVolume",
    ] {
        if let Some(value) = extract_xml_value(body, arg)?
            && args.insert(arg.to_string(), value).is_some()
        {
            return Err(DlnaError::Invalid("duplicate SOAP argument"));
        }
    }
    Ok(SoapAction {
        service: service.to_string(),
        name: name.to_string(),
        args,
    })
}

fn extract_xml_value(body: &str, tag: &str) -> Result<Option<String>, DlnaError> {
    let mut search_from = 0;
    while let Some(relative) = body[search_from..].find('<') {
        let start = search_from + relative;
        let rest = &body[start + 1..];
        let Some(end) = rest.find('>') else {
            return Err(DlnaError::Invalid("SOAP XML tag"));
        };
        let opening = &rest[..end];
        let local_name = opening
            .split_once(':')
            .map(|(_, local)| local)
            .unwrap_or(opening)
            .split_whitespace()
            .next()
            .unwrap_or_default();
        if local_name == tag && !opening.starts_with('/') && !opening.ends_with('/') {
            let content_start = start + end + 2;
            let closing_plain = format!("</{tag}>");
            let closing_prefixed = format!("</u:{tag}>");
            let (closing_start, closing_len) =
                if let Some(index) = body[content_start..].find(&closing_plain) {
                    (content_start + index, closing_plain.len())
                } else if let Some(index) = body[content_start..].find(&closing_prefixed) {
                    (content_start + index, closing_prefixed.len())
                } else {
                    return Err(DlnaError::Invalid("SOAP XML closing tag"));
                };
            let raw = body[content_start..closing_start].trim();
            if raw.len() > MAX_SOAP_ARGUMENT_BYTES {
                return Err(DlnaError::TooLarge("SOAP argument"));
            }
            if contains_xml_opening(&body[closing_start + closing_len..], tag) {
                return Err(DlnaError::Invalid("duplicate SOAP argument"));
            }
            return Ok(Some(xml_unescape(raw)?));
        }
        search_from = start + end + 2;
    }
    Ok(None)
}

fn contains_xml_opening(body: &str, tag: &str) -> bool {
    let mut search_from = 0;
    while let Some(relative) = body[search_from..].find('<') {
        let start = search_from + relative;
        let rest = &body[start + 1..];
        let Some(end) = rest.find('>') else {
            return false;
        };
        let opening = &rest[..end];
        let local_name = opening
            .split_once(':')
            .map(|(_, local)| local)
            .unwrap_or(opening)
            .split_whitespace()
            .next()
            .unwrap_or_default();
        if local_name == tag && !opening.starts_with('/') && !opening.ends_with('/') {
            return true;
        }
        search_from = start + end + 2;
    }
    false
}

fn xml_unescape(value: &str) -> Result<String, DlnaError> {
    let mut output = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(index) = rest.find('&') {
        output.push_str(&rest[..index]);
        let Some(end) = rest[index..].find(';') else {
            return Err(DlnaError::Invalid("XML entity"));
        };
        let entity = &rest[index..=index + end];
        output.push_str(match entity {
            "&amp;" => "&",
            "&lt;" => "<",
            "&gt;" => ">",
            "&quot;" => "\"",
            "&apos;" => "'",
            _ => return Err(DlnaError::Invalid("XML entity")),
        });
        rest = &rest[index + end + 1..];
    }
    output.push_str(rest);
    if output.len() > MAX_SOAP_ARGUMENT_BYTES {
        return Err(DlnaError::TooLarge("SOAP argument"));
    }
    Ok(output)
}

fn require_instance_zero(action: &SoapAction) -> Result<(), SoapFault> {
    if action.arg("InstanceID").unwrap_or("0") == "0" {
        Ok(())
    } else {
        Err(SoapFault::new(718, "Invalid InstanceID"))
    }
}

fn ensure_media_loaded(state: &MediaRendererState) -> Result<(), SoapFault> {
    if state.current_uri.is_empty() {
        Err(SoapFault::new(701, "Transition not available"))
    } else {
        Ok(())
    }
}

fn validate_media_uri(uri: &str) -> Result<(), SoapFault> {
    if uri.len() > MAX_FIELD_BYTES || uri.contains(['\r', '\n']) {
        return Err(SoapFault::new(714, "Illegal MIME-type"));
    }
    let Some((scheme, _)) = uri.split_once("://") else {
        return Err(SoapFault::new(714, "Illegal MIME-type"));
    };
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return Err(SoapFault::new(714, "Illegal MIME-type"));
    }
    Ok(())
}

fn parse_upnp_time(value: &str) -> Result<u64, SoapFault> {
    let parts = value.split(':').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(SoapFault::new(711, "Seek target invalid"));
    }
    let hours = parts[0]
        .parse::<u64>()
        .map_err(|_| SoapFault::new(711, "Seek target invalid"))?;
    let minutes = parts[1]
        .parse::<u64>()
        .map_err(|_| SoapFault::new(711, "Seek target invalid"))?;
    let seconds = parts[2]
        .parse::<u64>()
        .map_err(|_| SoapFault::new(711, "Seek target invalid"))?;
    if minutes >= 60 || seconds >= 60 {
        return Err(SoapFault::new(711, "Seek target invalid"));
    }
    Ok(hours * 3600 + minutes * 60 + seconds)
}

fn format_upnp_time(seconds: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds / 60) % 60,
        seconds % 60
    )
}

fn metadata_duration(metadata: &str) -> Option<u64> {
    for quote in ['"', '\''] {
        let marker = format!("duration={quote}");
        if let Some(start) = metadata.find(&marker) {
            let value_start = start + marker.len();
            let value_end = metadata[value_start..].find(quote)? + value_start;
            return parse_upnp_time(&metadata[value_start..value_end]).ok();
        }
    }
    None
}

fn soap_response(service: &str, action: &str, arguments: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:{action}Response xmlns:u=\"{service}\">{arguments}</u:{action}Response></s:Body></s:Envelope>",
        action = action,
        service = service,
        arguments = arguments
    )
}

fn soap_fault_response(code: u16, description: &str) -> HttpResponse {
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><s:Fault><faultcode>s:Client</faultcode><faultstring>UPnPError</faultstring><detail><UPnPError xmlns=\"urn:schemas-upnp-org:control-1-0\"><errorCode>{code}</errorCode><errorDescription>{}</errorDescription></UPnPError></detail></s:Fault></s:Body></s:Envelope>",
        xml_escape(description)
    );
    HttpResponse::xml(500, "Internal Server Error", body)
}

fn scpd_xml(service: &str) -> String {
    match service {
        AVTRANSPORT_SERVICE => "<?xml version=\"1.0\" encoding=\"utf-8\"?><scpd xmlns=\"urn:schemas-upnp-org:service-1-0\"><specVersion><major>1</major><minor>0</minor></specVersion><actionList><action><name>SetAVTransportURI</name></action><action><name>Play</name></action><action><name>Pause</name></action><action><name>Stop</name></action><action><name>Seek</name></action><action><name>GetTransportInfo</name></action><action><name>GetPositionInfo</name></action><action><name>GetMediaInfo</name></action></actionList><serviceStateTable><stateVariable sendEvents=\"yes\"><name>TransportState</name><dataType>string</dataType></stateVariable></serviceStateTable></scpd>".to_string(),
        RENDERING_CONTROL_SERVICE => "<?xml version=\"1.0\" encoding=\"utf-8\"?><scpd xmlns=\"urn:schemas-upnp-org:service-1-0\"><specVersion><major>1</major><minor>0</minor></specVersion><actionList><action><name>SetVolume</name></action><action><name>GetVolume</name></action></actionList><serviceStateTable><stateVariable sendEvents=\"yes\"><name>Volume</name><dataType>ui2</dataType></stateVariable></serviceStateTable></scpd>".to_string(),
        CONNECTION_MANAGER_SERVICE => "<?xml version=\"1.0\" encoding=\"utf-8\"?><scpd xmlns=\"urn:schemas-upnp-org:service-1-0\"><specVersion><major>1</major><minor>0</minor></specVersion><actionList><action><name>GetProtocolInfo</name></action><action><name>GetCurrentConnectionIDs</name></action><action><name>GetCurrentConnectionInfo</name></action></actionList></scpd>".to_string(),
        _ => "<?xml version=\"1.0\" encoding=\"utf-8\"?><scpd xmlns=\"urn:schemas-upnp-org:service-1-0\"><specVersion><major>1</major><minor>0</minor></specVersion></scpd>".to_string(),
    }
}

fn push_tag(output: &mut String, tag: &str, value: &str) {
    output.push('<');
    output.push_str(tag);
    output.push('>');
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            _ => output.push(character),
        }
    }
    output.push_str("</");
    output.push_str(tag);
    output.push('>');
}

fn xml_escape(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            _ => output.push(character),
        }
    }
    output
}

fn validate_service_type(service_type: &str) -> Result<(), DlnaError> {
    if !matches!(
        service_type,
        AVTRANSPORT_SERVICE | RENDERING_CONTROL_SERVICE | CONNECTION_MANAGER_SERVICE
    ) {
        return Err(DlnaError::Invalid("GENA service type"));
    }
    validate_field(service_type, "GENA service type")
}

fn validate_callback_url(callback_url: &str) -> Result<(), DlnaError> {
    validate_field(callback_url, "GENA callback URL")?;
    let Some((scheme, _)) = callback_url.split_once("://") else {
        return Err(DlnaError::Invalid("GENA callback URL"));
    };
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return Err(DlnaError::Invalid("GENA callback URL"));
    }
    Ok(())
}

fn callback_authority_and_target(callback_url: &str) -> Result<(String, String), DlnaError> {
    let (_, rest) = callback_url
        .split_once("://")
        .ok_or(DlnaError::Invalid("GENA callback URL"))?;
    let slash = rest.find('/');
    let (authority, target) = match slash {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    if authority.is_empty()
        || authority.len() > MAX_FIELD_BYTES
        || !authority
            .bytes()
            .all(|byte| !byte.is_ascii_whitespace() && !byte.is_ascii_control())
        || target.is_empty()
        || target.len() > MAX_FIELD_BYTES
        || !target.starts_with('/')
        || target.contains(['\r', '\n', ' '])
    {
        return Err(DlnaError::Invalid("GENA callback URL"));
    }
    Ok((authority.to_string(), target.to_string()))
}

fn parse_callback_header(value: &str) -> Option<&str> {
    let value = value.trim();
    let value = value.strip_prefix('<')?.strip_suffix('>')?;
    if value.contains('<') || value.contains('>') {
        return None;
    }
    Some(value)
}

fn parse_gena_timeout(timeout: Option<&str>) -> Result<u32, DlnaError> {
    let Some(timeout) = timeout else {
        return Ok(DEFAULT_GENA_TIMEOUT_SECONDS);
    };
    let timeout = timeout.trim();
    if timeout.eq_ignore_ascii_case("second-infinite") {
        return Ok(MAX_GENA_TIMEOUT_SECONDS);
    }
    let Some(value) = timeout.strip_prefix("Second-") else {
        return Err(DlnaError::Invalid("GENA timeout"));
    };
    let seconds = value
        .parse::<u32>()
        .map_err(|_| DlnaError::Invalid("GENA timeout"))?;
    if seconds == 0 || seconds > MAX_GENA_TIMEOUT_SECONDS {
        return Err(DlnaError::Invalid("GENA timeout"));
    }
    Ok(seconds)
}

fn gena_property_set(properties: &BTreeMap<String, String>) -> Result<String, DlnaError> {
    if properties.len() > MAX_SERVICES {
        return Err(DlnaError::TooLarge("GENA properties"));
    }
    let mut body = "<e:propertyset xmlns:e=\"urn:schemas-upnp-org:event-1-0\">".to_string();
    for (name, value) in properties {
        validate_field(name, "GENA property name")?;
        if !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(DlnaError::Invalid("GENA property name"));
        }
        if value.len() > MAX_FIELD_BYTES || value.contains(['\r', '\n']) {
            return Err(DlnaError::InvalidField("GENA property value"));
        }
        body.push_str("<e:property><");
        body.push_str(name);
        body.push('>');
        body.push_str(&xml_escape(value));
        body.push_str("</");
        body.push_str(name);
        body.push_str("></e:property>");
    }
    body.push_str("</e:propertyset>");
    if body.len() > MAX_SOAP_ARGUMENT_BYTES {
        return Err(DlnaError::TooLarge("GENA event"));
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_reencodes_bounded_msearch() {
        let bytes = b"M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: ssdp:all\r\n\r\n";
        let message = SsdpMessage::parse(bytes).unwrap();
        assert_eq!(
            message.headers.get("host"),
            Some(&"239.255.255.250:1900".to_string())
        );
        assert_eq!(
            SsdpMessage::parse(&message.encode().unwrap()).unwrap(),
            message
        );
    }

    #[test]
    fn rejects_duplicate_headers_and_bad_content_length() {
        let duplicate = b"NOTIFY * HTTP/1.1\r\nNT: upnp:rootdevice\r\nnt: uuid:x\r\n\r\n";
        assert_eq!(
            SsdpMessage::parse(duplicate),
            Err(DlnaError::Invalid("SSDP duplicate or oversized header"))
        );
        let bad_length = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nabc";
        assert_eq!(
            SsdpMessage::parse(bad_length),
            Err(DlnaError::Invalid("SSDP content length"))
        );
    }

    #[test]
    fn device_description_escapes_fields_and_lists_services() {
        let description = DeviceDescription {
            friendly_name: "Frame & Ark".to_string(),
            manufacturer: "FrameArk".to_string(),
            model_name: "Receiver".to_string(),
            udn: "uuid:test".to_string(),
            services: vec![ServiceDescription {
                service_type: "urn:schemas-upnp-org:service:AVTransport:1".to_string(),
                service_id: "urn:upnp-org:serviceId:AVTransport".to_string(),
                control_url: "/upnp/control".to_string(),
                event_sub_url: "/upnp/event".to_string(),
                scpd_url: "/upnp/scpd.xml".to_string(),
            }],
        };
        let xml = description.to_xml().unwrap();
        assert!(xml.contains("Frame &amp; Ark"));
        assert!(
            xml.contains("<serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>")
        );
    }

    fn advertisement() -> SsdpAdvertisement {
        SsdpAdvertisement::new(
            "urn:schemas-upnp-org:device:MediaRenderer:1",
            "uuid:frameark-renderer::urn:schemas-upnp-org:device:MediaRenderer:1",
            "http://127.0.0.1:8080/device.xml",
            1800,
            "FrameArk/0.1 UPnP/1.1",
        )
        .unwrap()
    }

    #[test]
    fn udp_notify_round_trip_is_bounded_and_parseable() {
        let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .unwrap();
        let publisher =
            SsdpPublisher::bind("127.0.0.1:0".parse().unwrap(), advertisement()).unwrap();
        let sent = publisher
            .notify(receiver.local_addr().unwrap(), true)
            .unwrap();
        let mut buffer = [0_u8; MAX_SSDP_BYTES];
        let (received, _) = receiver.recv_from(&mut buffer).unwrap();
        assert_eq!(sent, received);
        let message = SsdpMessage::parse(&buffer[..received]).unwrap();
        assert_eq!(message.headers.get("nts"), Some(&"ssdp:alive".to_string()));
    }

    #[test]
    fn multicast_notify_emits_alive_and_byebye_packets() {
        let publisher =
            SsdpPublisher::bind("127.0.0.1:0".parse().unwrap(), advertisement()).unwrap();
        assert!(publisher.notify_multicast(true).unwrap() > 0);
        assert!(publisher.notify_multicast(false).unwrap() > 0);
    }

    #[test]
    fn search_response_only_matches_requested_service() {
        let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .unwrap();
        let publisher =
            SsdpPublisher::bind("127.0.0.1:0".parse().unwrap(), advertisement()).unwrap();
        let mut headers = BTreeMap::new();
        headers.insert("man".to_string(), "\"ssdp:discover\"".to_string());
        headers.insert("st".to_string(), "ssdp:all".to_string());
        let request = SsdpMessage {
            start_line: SsdpStartLine::Request {
                method: "M-SEARCH".to_string(),
                target: "*".to_string(),
            },
            headers,
            body: Vec::new(),
        };
        assert!(
            publisher
                .respond_to_search(&request, receiver.local_addr().unwrap())
                .unwrap()
                .is_some()
        );
        let mut buffer = [0_u8; MAX_SSDP_BYTES];
        let (received, _) = receiver.recv_from(&mut buffer).unwrap();
        let response = SsdpMessage::parse(&buffer[..received]).unwrap();
        assert_eq!(
            response.headers.get("st"),
            Some(&advertisement().service_type)
        );

        let mut unrelated = request;
        unrelated
            .headers
            .insert("st".to_string(), "upnp:rootdevice".to_string());
        assert_eq!(
            publisher
                .respond_to_search(&unrelated, receiver.local_addr().unwrap())
                .unwrap(),
            None
        );
    }

    fn renderer_description() -> DeviceDescription {
        DeviceDescription {
            friendly_name: "FrameArk Renderer".to_string(),
            manufacturer: "FrameArk".to_string(),
            model_name: "FrameArk Test Receiver".to_string(),
            udn: "uuid:frameark-test".to_string(),
            services: vec![
                ServiceDescription {
                    service_type: AVTRANSPORT_SERVICE.to_string(),
                    service_id: "urn:upnp-org:serviceId:AVTransport".to_string(),
                    control_url: "/upnp/control/avtransport".to_string(),
                    event_sub_url: "/upnp/event/avtransport".to_string(),
                    scpd_url: "/scpd/avtransport.xml".to_string(),
                },
                ServiceDescription {
                    service_type: RENDERING_CONTROL_SERVICE.to_string(),
                    service_id: "urn:upnp-org:serviceId:RenderingControl".to_string(),
                    control_url: "/upnp/control/renderingcontrol".to_string(),
                    event_sub_url: "/upnp/event/renderingcontrol".to_string(),
                    scpd_url: "/scpd/renderingcontrol.xml".to_string(),
                },
                ServiceDescription {
                    service_type: CONNECTION_MANAGER_SERVICE.to_string(),
                    service_id: "urn:upnp-org:serviceId:ConnectionManager".to_string(),
                    control_url: "/upnp/control/connectionmanager".to_string(),
                    event_sub_url: "/upnp/event/connectionmanager".to_string(),
                    scpd_url: "/scpd/connectionmanager.xml".to_string(),
                },
            ],
        }
    }

    #[test]
    fn tcp_renderer_serves_one_bounded_loopback_request() {
        let service = MediaRendererHttpService::new(renderer_description()).unwrap();
        let mut server = MediaRendererTcpServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            service,
            Duration::from_secs(1),
        )
        .unwrap();
        let address = server.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .write_all(b"GET /device.xml HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .unwrap();
            let mut response = Vec::new();
            stream.read_to_end(&mut response).unwrap();
            response
        });

        let peer = server.serve_once().unwrap();
        let response = client.join().unwrap();
        assert!(peer.ip().is_loopback());
        assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
        assert!(
            response
                .windows(17)
                .any(|window| window == b"FrameArk Renderer")
        );
    }

    #[test]
    fn tcp_renderer_serves_a_bounded_request_batch_and_rejects_unbounded_budget() {
        let service = MediaRendererHttpService::new(renderer_description()).unwrap();
        let mut server = MediaRendererTcpServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            service,
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(
            server.serve_requests(0),
            Err(DlnaError::Invalid("HTTP request batch size"))
        );
        assert_eq!(
            server.serve_requests(MAX_TCP_REQUESTS_PER_RUN + 1),
            Err(DlnaError::Invalid("HTTP request batch size"))
        );
        let address = server.local_addr().unwrap();
        let clients = std::thread::spawn(move || {
            for _ in 0..2 {
                let mut stream = TcpStream::connect(address).unwrap();
                stream
                    .write_all(b"GET /device.xml HTTP/1.1\r\nHost: localhost\r\n\r\n")
                    .unwrap();
                let mut response = Vec::new();
                stream.read_to_end(&mut response).unwrap();
                assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
            }
        });
        let peers = server.serve_requests(2).unwrap();
        clients.join().unwrap();
        assert_eq!(peers.len(), 2);
        assert!(peers.iter().all(|peer| peer.ip().is_loopback()));
    }

    #[test]
    fn tcp_renderer_streams_explicit_file_ranges_without_loading_the_file() {
        let path = std::env::temp_dir().join(format!(
            "frameark-dlna-{}-{}.bin",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let payload = (0..(256 * 1024 + 17))
            .map(|value| (value % 251) as u8)
            .collect::<Vec<_>>();
        std::fs::write(&path, &payload).unwrap();
        let resource = FileMediaResource::new(
            "/media/movie.bin",
            "video/mp4",
            "http-get:*:video/mp4:*",
            &path,
        )
        .unwrap();
        let mut service = MediaRendererHttpService::new(renderer_description()).unwrap();
        service.register_file_media_resource(resource).unwrap();
        let mut server = MediaRendererTcpServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            service,
            Duration::from_secs(1),
        )
        .unwrap();
        let address = server.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .write_all(
                    b"GET /media/movie.bin HTTP/1.1\r\nHost: localhost\r\nRange: bytes=4-200000\r\n\r\n",
                )
                .unwrap();
            let mut response = Vec::new();
            stream.read_to_end(&mut response).unwrap();
            response
        });
        server.serve_once().unwrap();
        let response = client.join().unwrap();
        let header_end = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap();
        let headers = std::str::from_utf8(&response[..header_end]).unwrap();
        assert!(headers.starts_with("HTTP/1.1 206 Partial Content\r\n"));
        assert!(headers.contains("content-length: 199997"));
        assert!(headers.contains("content-range: bytes 4-200000/262161"));
        assert_eq!(&response[header_end + 4..], &payload[4..=200_000]);
        std::fs::remove_file(path).unwrap();
    }

    fn soap_request(path: &str, service: &str, action: &str, arguments: &str) -> HttpRequest {
        let body = format!(
            "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><u:{action} xmlns:u=\"{service}\">{arguments}</u:{action}></s:Body></s:Envelope>"
        )
        .into_bytes();
        let wire = format!(
            "POST {path} HTTP/1.1\r\nSOAPAction: \"{service}#{action}\"\r\nContent-Length: {}\r\nContent-Type: text/xml\r\n\r\n",
            body.len()
        );
        let mut wire = wire.into_bytes();
        wire.extend_from_slice(&body);
        HttpRequest::parse(&wire).unwrap()
    }

    #[test]
    fn http_parser_rejects_chunked_and_round_trips_body_length() {
        let bytes = b"GET /device.xml HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        let request = HttpRequest::parse(bytes).unwrap();
        assert_eq!(request.method, "GET");
        assert_eq!(request.header("HOST"), Some("127.0.0.1"));
        let chunked = b"POST /upnp HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n";
        assert_eq!(
            HttpRequest::parse(chunked),
            Err(DlnaError::Invalid("HTTP transfer encoding"))
        );
        let wrong_length = b"POST /upnp HTTP/1.1\r\nContent-Length: 4\r\n\r\nabc";
        assert_eq!(
            HttpRequest::parse(wrong_length),
            Err(DlnaError::Invalid("HTTP content length"))
        );
    }

    #[test]
    fn renderer_serves_description_and_scopd_with_bounded_http_responses() {
        let mut service = MediaRendererHttpService::new(renderer_description()).unwrap();
        let request =
            HttpRequest::parse(b"GET /device.xml HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let response = service.handle(&request);
        assert_eq!(response.status, 200);
        assert!(
            std::str::from_utf8(&response.body)
                .unwrap()
                .contains("FrameArk Renderer")
        );
        assert!(
            response
                .encode()
                .unwrap()
                .windows(4)
                .any(|window| window == b"\r\n\r\n")
        );

        let scpd =
            HttpRequest::parse(b"GET /scpd/avtransport.xml HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .unwrap();
        let response = service.handle(&scpd);
        assert_eq!(response.status, 200);
        assert!(
            std::str::from_utf8(&response.body)
                .unwrap()
                .contains("GetPositionInfo")
        );
    }

    #[test]
    fn range_and_didl_contracts_are_bounded_and_escaped() {
        assert_eq!(
            ByteRange::parse("bytes=2-5", 10).unwrap(),
            ByteRange { start: 2, end: 5 }
        );
        assert_eq!(
            ByteRange::parse("bytes=-3", 10).unwrap(),
            ByteRange { start: 7, end: 9 }
        );
        assert!(ByteRange::parse("bytes=2-5,7-8", 10).is_err());
        assert!(ByteRange::parse("bytes=99-", 10).is_err());

        let item = DidlLiteItem {
            id: "item-1".to_string(),
            parent_id: "0".to_string(),
            title: "A & B".to_string(),
            class_name: "object.item.videoItem".to_string(),
            resources: vec![DidlResource {
                uri: "https://media.example/video.mp4?a=1&b=2".to_string(),
                protocol_info: "http-get:*:video/mp4:*".to_string(),
                duration: Some("00:00:12".to_string()),
                size: Some(12_345),
            }],
        };
        let xml = item.to_xml().unwrap();
        assert!(xml.contains("A &amp; B"));
        assert!(xml.contains("a=1&amp;b=2"));
        assert!(xml.contains("duration=\"00:00:12\""));
    }

    #[test]
    fn renderer_serves_full_and_partial_media_without_fetching_urls() {
        let mut service = MediaRendererHttpService::new(renderer_description()).unwrap();
        service
            .register_media_resource(
                MediaResource::new(
                    "/media/test.bin",
                    "video/mp4",
                    "http-get:*:video/mp4:*",
                    b"0123456789".to_vec(),
                )
                .unwrap(),
            )
            .unwrap();
        let full =
            HttpRequest::parse(b"GET /media/test.bin HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let response = service.handle(&full);
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"0123456789");
        assert_eq!(
            response.headers.get("accept-ranges"),
            Some(&"bytes".to_string())
        );

        let partial = HttpRequest::parse(
            b"GET /media/test.bin HTTP/1.1\r\nHost: localhost\r\nRange: bytes=3-6\r\n\r\n",
        )
        .unwrap();
        let response = service.handle(&partial);
        assert_eq!(response.status, 206);
        assert_eq!(response.body, b"3456");
        assert_eq!(
            response.headers.get("content-range"),
            Some(&"bytes 3-6/10".to_string())
        );

        let invalid = HttpRequest::parse(
            b"GET /media/test.bin HTTP/1.1\r\nHost: localhost\r\nRange: bytes=99-\r\n\r\n",
        )
        .unwrap();
        assert_eq!(service.handle(&invalid).status, 416);
    }

    #[test]
    fn gena_registry_bounds_leases_sequences_and_service_scope() {
        let mut registry = GenaRegistry::default();
        let subscription = registry
            .subscribe(
                AVTRANSPORT_SERVICE,
                "http://127.0.0.1:9000/events",
                Some("Second-60"),
            )
            .unwrap();
        assert_eq!(subscription.next_sequence, 0);
        assert_eq!(subscription.timeout_seconds, 60);
        let mut properties = BTreeMap::new();
        properties.insert("TransportState".to_string(), "PLAYING".to_string());
        let events = registry.publish(AVTRANSPORT_SERVICE, &properties).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sequence, 1);
        assert!(events[0].body.contains("PLAYING"));
        assert!(
            registry
                .publish(RENDERING_CONTROL_SERVICE, &properties)
                .unwrap()
                .is_empty()
        );
        let renewed = registry
            .renew(&subscription.sid, Some("Second-infinite"))
            .unwrap();
        assert_eq!(renewed.timeout_seconds, MAX_GENA_TIMEOUT_SECONDS);
        registry.unsubscribe(&subscription.sid).unwrap();
        assert!(registry.is_empty());
        assert!(
            registry
                .subscribe(AVTRANSPORT_SERVICE, "file:///tmp/events", None)
                .is_err()
        );
        assert!(
            registry
                .subscribe(
                    AVTRANSPORT_SERVICE,
                    "http://127.0.0.1:9000/events",
                    Some("Second-0")
                )
                .is_err()
        );
    }

    #[test]
    fn gena_registry_expires_leases_before_publishing() {
        let mut registry = GenaRegistry::default();
        let now = Instant::now();
        registry
            .subscribe_at(
                AVTRANSPORT_SERVICE,
                "http://127.0.0.1:9000/events",
                Some("Second-60"),
                now,
            )
            .unwrap();
        assert_eq!(registry.expire_at(now + Duration::from_secs(59)), 0);

        let mut properties = BTreeMap::new();
        properties.insert("TransportState".to_string(), "PLAYING".to_string());
        assert_eq!(
            registry
                .publish_at(
                    AVTRANSPORT_SERVICE,
                    &properties,
                    now + Duration::from_secs(59)
                )
                .unwrap()
                .len(),
            1
        );

        assert!(
            registry
                .publish_at(
                    AVTRANSPORT_SERVICE,
                    &properties,
                    now + Duration::from_secs(60)
                )
                .unwrap()
                .is_empty()
        );
        assert_eq!(registry.subscriptions.len(), 0);
    }

    #[test]
    fn gena_renewal_refreshes_deadline_but_never_revives_expired_lease() {
        let mut registry = GenaRegistry::default();
        let now = Instant::now();
        let subscription = registry
            .subscribe_at(
                AVTRANSPORT_SERVICE,
                "http://127.0.0.1:9000/events",
                Some("Second-60"),
                now,
            )
            .unwrap();
        let renewed = registry
            .renew_at(
                &subscription.sid,
                Some("Second-120"),
                now + Duration::from_secs(59),
            )
            .unwrap();
        assert_eq!(renewed.expires_at, now + Duration::from_secs(179));
        assert_eq!(renewed.callback_url, subscription.callback_url);
        assert_eq!(registry.expire_at(now + Duration::from_secs(60)), 0);
        assert!(
            registry
                .renew_at(
                    &subscription.sid,
                    Some("Second-0"),
                    now + Duration::from_secs(178)
                )
                .is_err()
        );
        assert_eq!(
            registry.renew_at(
                &subscription.sid,
                Some("Second-60"),
                now + Duration::from_secs(179)
            ),
            Err(DlnaError::Invalid("GENA SID"))
        );
        assert!(registry.is_empty());
    }

    #[test]
    fn gena_subscribe_reclaims_expired_capacity_without_a_timer() {
        let mut registry = GenaRegistry::default();
        let now = Instant::now();
        for _ in 0..MAX_GENA_SUBSCRIPTIONS {
            registry
                .subscribe_at(
                    AVTRANSPORT_SERVICE,
                    "http://127.0.0.1:9000/events",
                    Some("Second-60"),
                    now,
                )
                .unwrap();
        }
        assert!(
            registry
                .subscribe_at(
                    AVTRANSPORT_SERVICE,
                    "http://127.0.0.1:9000/events",
                    None,
                    now
                )
                .is_err()
        );
        registry
            .subscribe_at(
                AVTRANSPORT_SERVICE,
                "http://127.0.0.1:9000/events",
                None,
                now + Duration::from_secs(60),
            )
            .unwrap();
        assert_eq!(registry.subscriptions.len(), 1);
    }

    #[test]
    fn renderer_discards_expired_or_unsubscribed_pending_events() {
        let mut service = MediaRendererHttpService::new(renderer_description()).unwrap();
        let now = Instant::now();
        let subscription = service
            .gena
            .subscribe_at(
                AVTRANSPORT_SERVICE,
                "http://127.0.0.1:9000/events",
                Some("Second-60"),
                now - Duration::from_secs(61),
            )
            .unwrap();
        service.pending_events.push(GenaEvent {
            sid: subscription.sid.clone(),
            callback_url: subscription.callback_url,
            service_type: subscription.service_type,
            sequence: 1,
            body: "<event/>".to_string(),
        });
        assert_eq!(service.subscription_count(), 0);
        assert!(service.drain_events().is_empty());
        assert!(service.gena.subscriptions.is_empty());

        let subscribe = HttpRequest::parse(b"SUBSCRIBE /upnp/control/avtransport HTTP/1.1\r\nHost: localhost\r\nNT: upnp:event\r\nCALLBACK: <http://127.0.0.1:9000/events>\r\nTIMEOUT: Second-120\r\n\r\n").unwrap();
        let response = service.handle(&subscribe);
        let sid = response.headers.get("sid").unwrap();
        assert_eq!(service.pending_events.len(), 1);
        let unsubscribe = HttpRequest::parse(format!("UNSUBSCRIBE /upnp/control/avtransport HTTP/1.1\r\nHost: localhost\r\nSID: {sid}\r\n\r\n").as_bytes()).unwrap();
        assert_eq!(service.handle(&unsubscribe).status, 200);
        assert!(service.drain_events().is_empty());
        let renew_expired = HttpRequest::parse(format!("SUBSCRIBE /upnp/control/avtransport HTTP/1.1\r\nHost: localhost\r\nSID: {}\r\nTIMEOUT: Second-60\r\n\r\n", subscription.sid).as_bytes()).unwrap();
        assert_eq!(service.handle(&renew_expired).status, 412);
    }

    #[test]
    fn renderer_subscribe_emits_initial_and_mutation_events() {
        let mut service = MediaRendererHttpService::new(renderer_description()).unwrap();
        let subscribe = HttpRequest::parse(
            b"SUBSCRIBE /upnp/control/avtransport HTTP/1.1\r\nHost: localhost\r\nNT: upnp:event\r\nCALLBACK: <http://127.0.0.1:9000/events>\r\nTIMEOUT: Second-120\r\n\r\n",
        )
        .unwrap();
        let response = service.handle(&subscribe);
        assert_eq!(response.status, 200);
        let sid = response.headers.get("sid").cloned().unwrap();
        assert_eq!(
            response.headers.get("timeout"),
            Some(&"Second-120".to_string())
        );
        assert_eq!(service.subscription_count(), 1);
        let initial = service.drain_events();
        assert_eq!(initial.len(), 1);
        assert_eq!(initial[0].sid, sid);
        assert_eq!(initial[0].sequence, 1);

        let set_uri = soap_request(
            "/upnp/control/avtransport",
            AVTRANSPORT_SERVICE,
            "SetAVTransportURI",
            "<InstanceID>0</InstanceID><CurrentURI>https://media.example/video.mp4</CurrentURI>",
        );
        assert_eq!(service.handle(&set_uri).status, 200);
        let mutation = service.drain_events();
        assert_eq!(mutation.len(), 1);
        assert_eq!(mutation[0].sequence, 2);
        assert!(mutation[0].body.contains("CurrentTrackURI"));
        let notify = mutation[0].encode_http_notify().unwrap();
        let parsed_notify = HttpRequest::parse(&notify).unwrap();
        assert_eq!(parsed_notify.method, "NOTIFY");
        assert_eq!(parsed_notify.target, "/events");
        assert_eq!(parsed_notify.header("SID"), Some(sid.as_str()));
        assert_eq!(parsed_notify.header("SEQ"), Some("2"));
        assert_eq!(parsed_notify.header("NTS"), Some("upnp:propchange"));
        assert_eq!(parsed_notify.body, mutation[0].body.as_bytes());

        let renew = HttpRequest::parse(
            format!(
                "SUBSCRIBE /upnp/control/avtransport HTTP/1.1\r\nHost: localhost\r\nSID: {sid}\r\nTIMEOUT: Second-300\r\n\r\n"
            )
            .as_bytes(),
        )
        .unwrap();
        let response = service.handle(&renew);
        assert_eq!(response.status, 200);
        assert_eq!(
            response.headers.get("timeout"),
            Some(&"Second-300".to_string())
        );
        assert!(service.drain_events().is_empty());

        let unsubscribe = HttpRequest::parse(
            format!(
                "UNSUBSCRIBE /upnp/control/avtransport HTTP/1.1\r\nHost: localhost\r\nSID: {sid}\r\n\r\n"
            )
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(service.handle(&unsubscribe).status, 200);
        assert_eq!(service.subscription_count(), 0);
    }

    #[test]
    fn gena_callback_client_sends_notify_and_parses_bounded_response() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, peer) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let request = read_http_request(&mut stream).unwrap();
            assert_eq!(request.method, "NOTIFY");
            assert_eq!(request.target, "/events");
            assert_eq!(request.header("NT"), Some("upnp:event"));
            assert_eq!(request.body, b"<event/>".to_vec());
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
            peer
        });
        let event = GenaEvent {
            sid: "uuid:frameark-sub-1".to_string(),
            callback_url: format!("http://{address}/events"),
            service_type: AVTRANSPORT_SERVICE.to_string(),
            sequence: 4,
            body: "<event/>".to_string(),
        };
        let response = GenaCallbackClient::new(Duration::from_secs(1))
            .unwrap()
            .send(&event)
            .unwrap();
        assert_eq!(response.status, 200);
        assert!(response.body.is_empty());
        assert!(server.join().unwrap().ip().is_loopback());
    }

    #[test]
    fn gena_callback_client_rejects_tls_and_unbounded_authorities() {
        let event = GenaEvent {
            sid: "uuid:frameark-sub-1".to_string(),
            callback_url: "https://127.0.0.1:443/events".to_string(),
            service_type: AVTRANSPORT_SERVICE.to_string(),
            sequence: 1,
            body: "<event/>".to_string(),
        };
        assert_eq!(
            GenaCallbackClient::new(Duration::from_secs(1))
                .unwrap()
                .send(&event),
            Err(DlnaError::Invalid("GENA HTTPS requires TLS"))
        );
        let invalid = GenaEvent {
            callback_url: "http://127.0.0.1/events".to_string(),
            ..event
        };
        assert_eq!(
            GenaCallbackClient::new(Duration::from_secs(1))
                .unwrap()
                .send(&invalid),
            Err(DlnaError::Invalid("GENA callback port"))
        );
    }

    #[test]
    fn http_response_parser_rejects_chunked_and_preserves_headers() {
        let response = HttpResponse::parse(
            b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nX-FrameArk: yes\r\n\r\nack",
        )
        .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.headers.get("x-frameark"), Some(&"yes".to_string()));
        assert_eq!(response.body, b"ack");
        assert_eq!(
            HttpResponse::parse(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n"),
            Err(DlnaError::Invalid("HTTP transfer encoding"))
        );
    }

    #[test]
    fn renderer_executes_avtransport_and_volume_lifecycle() {
        let mut service = MediaRendererHttpService::new(renderer_description()).unwrap();
        let set_uri = soap_request(
            "/upnp/control/avtransport",
            AVTRANSPORT_SERVICE,
            "SetAVTransportURI",
            "<InstanceID>0</InstanceID><CurrentURI>https://media.example/video.mp4</CurrentURI><CurrentURIMetaData>&lt;DIDL-Lite&gt;&lt;item&gt;&lt;res duration=\"00:01:02\"/&gt;&lt;/item&gt;&lt;/DIDL-Lite&gt;</CurrentURIMetaData>",
        );
        assert_eq!(service.handle(&set_uri).status, 200);
        assert_eq!(service.state().transport_state, TransportState::Stopped);
        assert_eq!(service.state().duration_seconds, Some(62));

        let play = soap_request(
            "/upnp/control/avtransport",
            AVTRANSPORT_SERVICE,
            "Play",
            "<InstanceID>0</InstanceID><Speed>1</Speed>",
        );
        assert_eq!(service.handle(&play).status, 200);
        assert_eq!(service.state().transport_state, TransportState::Playing);

        let seek = soap_request(
            "/upnp/control/avtransport",
            AVTRANSPORT_SERVICE,
            "Seek",
            "<InstanceID>0</InstanceID><Unit>REL_TIME</Unit><Target>00:00:12</Target>",
        );
        assert_eq!(service.handle(&seek).status, 200);
        assert_eq!(service.state().position_seconds, 12);

        let volume = soap_request(
            "/upnp/control/renderingcontrol",
            RENDERING_CONTROL_SERVICE,
            "SetVolume",
            "<InstanceID>0</InstanceID><Channel>Master</Channel><DesiredVolume>73</DesiredVolume>",
        );
        assert_eq!(service.handle(&volume).status, 200);
        assert_eq!(service.state().volume, 73);

        let stop = soap_request(
            "/upnp/control/avtransport",
            AVTRANSPORT_SERVICE,
            "Stop",
            "<InstanceID>0</InstanceID>",
        );
        assert_eq!(service.handle(&stop).status, 200);
        assert_eq!(service.state().transport_state, TransportState::Stopped);
    }

    #[test]
    fn renderer_rejects_unsafe_uri_unknown_action_and_invalid_seek() {
        let mut service = MediaRendererHttpService::new(renderer_description()).unwrap();
        let unsafe_uri = soap_request(
            "/upnp/control/avtransport",
            AVTRANSPORT_SERVICE,
            "SetAVTransportURI",
            "<InstanceID>0</InstanceID><CurrentURI>file:///private/video.mp4</CurrentURI>",
        );
        assert_eq!(service.handle(&unsafe_uri).status, 500);
        assert!(service.state().current_uri.is_empty());

        let duplicate_arg = soap_request(
            "/upnp/control/avtransport",
            AVTRANSPORT_SERVICE,
            "SetAVTransportURI",
            "<InstanceID>0</InstanceID><InstanceID>0</InstanceID><CurrentURI>https://media.example/video.mp4</CurrentURI>",
        );
        assert_eq!(service.handle(&duplicate_arg).status, 500);

        let unknown = soap_request(
            "/upnp/control/avtransport",
            AVTRANSPORT_SERVICE,
            "Next",
            "<InstanceID>0</InstanceID>",
        );
        assert_eq!(service.handle(&unknown).status, 500);

        let set_uri = soap_request(
            "/upnp/control/avtransport",
            AVTRANSPORT_SERVICE,
            "SetAVTransportURI",
            "<InstanceID>0</InstanceID><CurrentURI>https://media.example/video.mp4</CurrentURI>",
        );
        assert_eq!(service.handle(&set_uri).status, 200);
        let seek = soap_request(
            "/upnp/control/avtransport",
            AVTRANSPORT_SERVICE,
            "Seek",
            "<InstanceID>0</InstanceID><Unit>REL_TIME</Unit><Target>bad</Target>",
        );
        let response = service.handle(&seek);
        assert_eq!(response.status, 500);
        assert_eq!(service.state().position_seconds, 0);
    }
}
