//! Bounded DLNA/UPnP protocol contracts.
//!
//! This crate starts M3 with deterministic SSDP parsing and XML description
//! generation. It provides a synchronous unicast-testable UDP publisher, but
//! deliberately does not join multicast groups, expose an HTTP server, or
//! claim AVTransport/GENA interoperability until those layers have their own
//! bounded parsers and compatibility fixtures.

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::net::{SocketAddr, UdpSocket};

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

/// Bounded SOAP/HTTP MediaRenderer handler.
///
/// The caller owns the TCP listener and connection lifecycle. This type only
/// parses complete requests and applies a single request to explicit Rust
/// state, which keeps socket policy and platform rendering outside the protocol
/// crate. It implements the basic AVTransport, RenderingControl, and
/// ConnectionManager actions needed for a first M3 vertical slice.
pub struct MediaRendererHttpService {
    description: DeviceDescription,
    device_description_path: String,
    state: MediaRendererState,
}

impl MediaRendererHttpService {
    /// Creates a handler after validating the supplied device description.
    pub fn new(description: DeviceDescription) -> Result<Self, DlnaError> {
        description.to_xml()?;
        Ok(Self {
            description,
            device_description_path: "/device.xml".to_string(),
            state: MediaRendererState::default(),
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

    /// Applies one parsed request and returns an HTTP response.
    pub fn handle(&mut self, request: &HttpRequest) -> HttpResponse {
        let path = request.target.split('?').next().unwrap_or(&request.target);
        match request.method.as_str() {
            "GET" => self.handle_get(path),
            "POST" => self.handle_post(path, request),
            _ => {
                let mut response = HttpResponse::empty(405, "Method Not Allowed");
                response
                    .headers
                    .insert("allow".to_string(), "GET, POST".to_string());
                response
            }
        }
    }

    fn handle_get(&self, path: &str) -> HttpResponse {
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
        match self.invoke(action) {
            Ok(body) => HttpResponse::xml(200, "OK", body),
            Err(fault) => soap_fault_response(fault.code, fault.description),
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
