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
}
