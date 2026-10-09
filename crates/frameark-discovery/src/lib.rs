//! Local-network discovery contracts and the mDNS/DNS-SD adapter for FrameArk.
//!
//! The adapter deliberately exposes protocol-neutral discovery events. Session
//! authorization and transport negotiation remain in `frameark-core`; this
//! crate only publishes and observes bounded service metadata.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::net::IpAddr;
use std::time::Duration;

use mdns_sd::{IfKind, ServiceDaemon, ServiceEvent, ServiceInfo, UnregisterStatus};

/// The DNS-SD service type reserved for FrameArk Native Protocol endpoints.
pub const FRAMEARK_SERVICE_TYPE: &str = "_frameark._udp.local.";

const MAX_INTERFACES: usize = 16;
const MAX_INSTANCE_NAME_BYTES: usize = 63;
const MAX_HOST_NAME_BYTES: usize = 253;
const MAX_TXT_KEY_BYTES: usize = 63;
const MAX_TXT_VALUE_BYTES: usize = 255;

/// Errors raised while validating or driving local-network discovery.
#[derive(Debug, Eq, PartialEq)]
pub enum DiscoveryError {
    /// A caller supplied an empty or oversized service field.
    InvalidField {
        /// Field name used for diagnostics.
        field: &'static str,
        /// Stable reason code.
        reason: &'static str,
    },
    /// The mDNS backend rejected an operation.
    Backend(String),
    /// The browser channel was closed by the backend.
    BrowserClosed,
    /// The registration handle has already been consumed.
    RegistrationClosed,
    /// A backend operation did not complete within the bounded wait.
    OperationTimeout,
}

impl fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField { field, reason } => {
                write!(f, "invalid discovery {field}: {reason}")
            }
            Self::Backend(message) => write!(f, "mDNS backend error: {message}"),
            Self::BrowserClosed => f.write_str("mDNS browser channel closed"),
            Self::RegistrationClosed => f.write_str("mDNS registration handle is closed"),
            Self::OperationTimeout => f.write_str("mDNS operation did not complete in time"),
        }
    }
}

impl std::error::Error for DiscoveryError {}

/// Configuration for a local-network mDNS daemon.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiscoveryConfig {
    interfaces: Vec<String>,
}

impl DiscoveryConfig {
    /// Creates a configuration that uses all active network interfaces.
    pub const fn all_interfaces() -> Self {
        Self {
            interfaces: Vec::new(),
        }
    }

    /// Restricts publication to named interfaces. An empty list means all interfaces.
    pub fn with_interfaces<I, S>(interfaces: I) -> Result<Self, DiscoveryError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let interfaces = interfaces.into_iter().map(Into::into).collect::<Vec<_>>();
        if interfaces.len() > MAX_INTERFACES {
            return Err(DiscoveryError::InvalidField {
                field: "interfaces",
                reason: "too_many",
            });
        }
        if interfaces.iter().any(|name| name.trim().is_empty()) {
            return Err(DiscoveryError::InvalidField {
                field: "interfaces",
                reason: "empty",
            });
        }
        Ok(Self { interfaces })
    }

    /// Returns the configured interface names, or an empty slice for automatic discovery.
    pub fn interfaces(&self) -> &[String] {
        &self.interfaces
    }
}

/// A bounded service advertisement for a FrameArk endpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedService {
    instance_name: String,
    host_name: String,
    port: u16,
    properties: BTreeMap<String, String>,
}

impl PublishedService {
    /// Creates an advertisement. Addresses are filled by the daemon per active interface.
    pub fn new(
        instance_name: impl Into<String>,
        host_name: impl Into<String>,
        port: u16,
    ) -> Result<Self, DiscoveryError> {
        let instance_name = instance_name.into();
        let host_name = host_name.into();
        validate_text("instance_name", &instance_name, MAX_INSTANCE_NAME_BYTES)?;
        validate_text("host_name", &host_name, MAX_HOST_NAME_BYTES)?;
        if port == 0 {
            return Err(DiscoveryError::InvalidField {
                field: "port",
                reason: "zero",
            });
        }
        Ok(Self {
            instance_name,
            host_name,
            port,
            properties: BTreeMap::new(),
        })
    }

    /// Adds or replaces a TXT property after applying RFC 6763 bounds.
    pub fn with_property(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, DiscoveryError> {
        let key = key.into();
        let value = value.into();
        if key.is_empty() || key.len() > MAX_TXT_KEY_BYTES || !key.is_ascii() || key.contains('=') {
            return Err(DiscoveryError::InvalidField {
                field: "txt_key",
                reason: "invalid",
            });
        }
        if value.len() > MAX_TXT_VALUE_BYTES
            || key.len().saturating_add(value.len()).saturating_add(1) > u8::MAX as usize
        {
            return Err(DiscoveryError::InvalidField {
                field: "txt_value",
                reason: "too_long",
            });
        }
        self.properties.insert(key, value);
        Ok(self)
    }

    /// Returns the advertised instance name.
    pub fn instance_name(&self) -> &str {
        &self.instance_name
    }

    /// Returns the advertised host name.
    pub fn host_name(&self) -> &str {
        &self.host_name
    }

    /// Returns the advertised UDP port.
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// Returns TXT properties in deterministic key order.
    pub fn properties(&self) -> &BTreeMap<String, String> {
        &self.properties
    }

    fn to_service_info(&self, config: &DiscoveryConfig) -> Result<ServiceInfo, DiscoveryError> {
        let properties = self
            .properties
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>();
        let mut info = ServiceInfo::new(
            FRAMEARK_SERVICE_TYPE,
            &self.instance_name,
            &self.host_name,
            "",
            self.port,
            properties.as_slice(),
        )
        .map_err(|error| DiscoveryError::Backend(error.to_string()))?
        .enable_addr_auto();
        if !config.interfaces.is_empty() {
            info.set_interfaces(
                config
                    .interfaces
                    .iter()
                    .map(|name| IfKind::Name(name.clone()))
                    .collect(),
            );
        }
        Ok(info)
    }
}

/// A resolved service observed through DNS-SD.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredService {
    /// Service type and domain.
    pub service_type: String,
    /// Fully qualified instance name.
    pub fullname: String,
    /// Hostname from the SRV record.
    pub host: String,
    /// Resolved service port.
    pub port: u16,
    /// Addresses reported by the active interfaces.
    pub addresses: BTreeSet<IpAddr>,
    /// UTF-8 TXT properties; invalid values are omitted by the backend.
    pub properties: BTreeMap<String, String>,
}

/// Normalized events consumed by the Rust discovery/session layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscoveryEvent {
    /// Browse query started.
    SearchStarted,
    /// PTR record found before SRV/TXT/address resolution.
    ServiceFound { fullname: String },
    /// Service records are resolved and ready for connection policy.
    ServiceResolved(DiscoveredService),
    /// A service lease expired or a goodbye record was received.
    ServiceRemoved { fullname: String },
    /// Browse query stopped.
    SearchStopped,
    /// A backend event unknown to this crate version was safely ignored.
    BackendIgnored,
}

/// A live mDNS publisher/browser for the FrameArk service type.
pub struct MdnsDiscovery {
    daemon: ServiceDaemon,
    config: DiscoveryConfig,
}

impl MdnsDiscovery {
    /// Starts a daemon. Socket setup is lazy and backend errors are reported by the monitor.
    pub fn new(config: DiscoveryConfig) -> Result<Self, DiscoveryError> {
        let daemon =
            ServiceDaemon::new().map_err(|error| DiscoveryError::Backend(error.to_string()))?;
        Ok(Self { daemon, config })
    }

    /// Publishes one service and returns a handle that unregisters on drop.
    pub fn publish(
        &self,
        service: &PublishedService,
    ) -> Result<PublishedRegistration, DiscoveryError> {
        let info = service.to_service_info(&self.config)?;
        let fullname = info.get_fullname().to_string();
        self.daemon
            .register(info)
            .map_err(|error| DiscoveryError::Backend(error.to_string()))?;
        Ok(PublishedRegistration {
            daemon: Some(self.daemon.clone()),
            fullname,
        })
    }

    /// Starts browsing for FrameArk services.
    pub fn browse(&self) -> Result<DiscoveryBrowser, DiscoveryError> {
        let receiver = self
            .daemon
            .browse(FRAMEARK_SERVICE_TYPE)
            .map_err(|error| DiscoveryError::Backend(error.to_string()))?;
        Ok(DiscoveryBrowser { receiver })
    }

    /// Requests daemon shutdown and waits for the backend acknowledgement.
    pub fn shutdown(&self) -> Result<(), DiscoveryError> {
        let receiver = self
            .daemon
            .shutdown()
            .map_err(|error| DiscoveryError::Backend(error.to_string()))?;
        receiver
            .recv_timeout(Duration::from_secs(2))
            .map(|_| ())
            .map_err(|_| DiscoveryError::OperationTimeout)
    }
}

/// A registration handle returned by [`MdnsDiscovery::publish`].
pub struct PublishedRegistration {
    daemon: Option<ServiceDaemon>,
    fullname: String,
}

impl PublishedRegistration {
    /// Returns the fully qualified DNS-SD instance name.
    pub fn fullname(&self) -> &str {
        &self.fullname
    }

    /// Unregisters the service and waits for the daemon acknowledgement.
    pub fn unregister(mut self) -> Result<(), DiscoveryError> {
        let daemon = self
            .daemon
            .take()
            .ok_or(DiscoveryError::RegistrationClosed)?;
        unregister(&daemon, &self.fullname)
    }
}

impl Drop for PublishedRegistration {
    fn drop(&mut self) {
        if let Some(daemon) = self.daemon.take() {
            let _ = daemon.unregister(&self.fullname);
        }
    }
}

/// A browser receiver with normalized events and bounded waits.
pub struct DiscoveryBrowser {
    receiver: mdns_sd::Receiver<ServiceEvent>,
}

impl DiscoveryBrowser {
    /// Receives one event, returning `Ok(None)` when the timeout expires.
    pub fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> Result<Option<DiscoveryEvent>, DiscoveryError> {
        match self.receiver.recv_timeout(timeout) {
            Ok(event) => Ok(Some(normalize_event(event))),
            Err(mdns_sd::RecvTimeoutError::Timeout) => Ok(None),
            Err(mdns_sd::RecvTimeoutError::Disconnected) => Err(DiscoveryError::BrowserClosed),
        }
    }
}

fn unregister(daemon: &ServiceDaemon, fullname: &str) -> Result<(), DiscoveryError> {
    let receiver = daemon
        .unregister(fullname)
        .map_err(|error| DiscoveryError::Backend(error.to_string()))?;
    match receiver
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| DiscoveryError::OperationTimeout)?
    {
        UnregisterStatus::OK => Ok(()),
        UnregisterStatus::NotFound => Err(DiscoveryError::Backend("service not found".into())),
    }
}

fn normalize_event(event: ServiceEvent) -> DiscoveryEvent {
    match event {
        ServiceEvent::SearchStarted(_) => DiscoveryEvent::SearchStarted,
        ServiceEvent::ServiceFound(_, fullname) => DiscoveryEvent::ServiceFound { fullname },
        ServiceEvent::ServiceResolved(info) => DiscoveryEvent::ServiceResolved(DiscoveredService {
            service_type: info.ty_domain,
            fullname: info.fullname,
            host: info.host,
            port: info.port,
            addresses: info
                .addresses
                .iter()
                .map(mdns_sd::ScopedIp::to_ip_addr)
                .collect(),
            properties: info
                .txt_properties
                .into_property_map_str()
                .into_iter()
                .collect(),
        }),
        ServiceEvent::ServiceRemoved(_, fullname) => DiscoveryEvent::ServiceRemoved { fullname },
        ServiceEvent::SearchStopped(_) => DiscoveryEvent::SearchStopped,
        _ => DiscoveryEvent::BackendIgnored,
    }
}

fn validate_text(field: &'static str, value: &str, max_bytes: usize) -> Result<(), DiscoveryError> {
    if value.trim().is_empty() {
        return Err(DiscoveryError::InvalidField {
            field,
            reason: "empty",
        });
    }
    if value.len() > max_bytes {
        return Err(DiscoveryError::InvalidField {
            field,
            reason: "too_long",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_to_all_interfaces() {
        assert_eq!(
            DiscoveryConfig::default(),
            DiscoveryConfig::all_interfaces()
        );
        assert!(DiscoveryConfig::default().interfaces().is_empty());
    }

    #[test]
    fn config_rejects_empty_interface_names() {
        assert_eq!(
            DiscoveryConfig::with_interfaces([" "]),
            Err(DiscoveryError::InvalidField {
                field: "interfaces",
                reason: "empty",
            })
        );
    }

    #[test]
    fn published_service_enforces_bounds_and_builds_auto_addresses() {
        let service = PublishedService::new("Living Room", "frameark-host.local.", 4_321)
            .unwrap()
            .with_property("ver", "1")
            .unwrap();
        let info = service
            .to_service_info(&DiscoveryConfig::default())
            .unwrap();
        assert!(info.is_addr_auto());
        assert_eq!(info.get_fullname(), "Living Room._frameark._udp.local.");
        assert_eq!(service.properties().get("ver"), Some(&"1".to_string()));
    }

    #[test]
    fn published_service_rejects_zero_port_and_invalid_txt_key() {
        assert!(matches!(
            PublishedService::new("receiver", "receiver.local.", 0),
            Err(DiscoveryError::InvalidField { field: "port", .. })
        ));
        let service = PublishedService::new("receiver", "receiver.local.", 4_321).unwrap();
        assert!(matches!(
            service.with_property("bad=key", "value"),
            Err(DiscoveryError::InvalidField {
                field: "txt_key",
                ..
            })
        ));
    }

    #[test]
    fn events_are_normalized_without_exposing_backend_types() {
        assert_eq!(
            normalize_event(ServiceEvent::ServiceFound(
                FRAMEARK_SERVICE_TYPE.to_string(),
                "receiver._frameark._udp.local.".to_string(),
            )),
            DiscoveryEvent::ServiceFound {
                fullname: "receiver._frameark._udp.local.".to_string(),
            }
        );
    }
}
