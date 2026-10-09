use std::collections::BTreeSet;
use std::fmt;

use frameark_core::Capability;

/// Version of the capability payload carried by FANP ClientHello/ServerHello.
pub const FANP_CAPABILITY_VERSION: u8 = 1;
/// Maximum number of capability entries accepted from an untrusted peer.
pub const MAX_CAPABILITIES: usize = 16;

const CAPABILITY_PAYLOAD_HEADER_LEN: usize = 2;

/// Errors raised while decoding or intersecting FANP capability offers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NegotiationError {
    /// The payload does not satisfy the bounded capability schema.
    InvalidPayload,
    /// The peer selected a capability schema version this crate does not know.
    UnsupportedVersion(u8),
    /// The offer contains more entries than the bounded schema permits.
    TooManyCapabilities,
    /// The two peers do not share a usable capability.
    NoCommonCapability,
}

impl fmt::Display for NegotiationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPayload => formatter.write_str("invalid FANP capability payload"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported FANP capability version {version}")
            }
            Self::TooManyCapabilities => {
                formatter.write_str("FANP capability offer exceeds the entry limit")
            }
            Self::NoCommonCapability => formatter.write_str("FANP peers have no common capability"),
        }
    }
}

impl std::error::Error for NegotiationError {}

/// A bounded capability offer exchanged after temporary pairing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityOffer {
    protocol_version: u8,
    capabilities: BTreeSet<Capability>,
}

impl CapabilityOffer {
    /// Creates a capability offer with the current FANP schema version.
    pub fn new<I>(capabilities: I) -> Result<Self, NegotiationError>
    where
        I: IntoIterator<Item = Capability>,
    {
        Self::with_version(FANP_CAPABILITY_VERSION, capabilities)
    }

    /// Creates an offer for an explicit capability schema version.
    pub fn with_version<I>(protocol_version: u8, capabilities: I) -> Result<Self, NegotiationError>
    where
        I: IntoIterator<Item = Capability>,
    {
        if protocol_version != FANP_CAPABILITY_VERSION {
            return Err(NegotiationError::UnsupportedVersion(protocol_version));
        }
        let capabilities = capabilities.into_iter().collect::<BTreeSet<_>>();
        if capabilities.len() > MAX_CAPABILITIES {
            return Err(NegotiationError::TooManyCapabilities);
        }
        Ok(Self {
            protocol_version,
            capabilities,
        })
    }

    /// Returns a receiver/sender baseline containing all core capabilities.
    pub fn default_capabilities() -> Self {
        Self::new(Capability::all()).expect("core capability inventory is bounded")
    }

    /// Returns the capability schema version.
    pub const fn protocol_version(&self) -> u8 {
        self.protocol_version
    }

    /// Returns the sorted capability set.
    pub fn capabilities(&self) -> &BTreeSet<Capability> {
        &self.capabilities
    }

    /// Returns whether this offer contains a capability.
    pub fn supports(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Encodes the bounded payload used by a FANP ClientHello or ServerHello.
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut payload =
            Vec::with_capacity(CAPABILITY_PAYLOAD_HEADER_LEN + self.capabilities.len());
        payload.push(self.protocol_version);
        payload.push(self.capabilities.len() as u8);
        payload.extend(
            self.capabilities
                .iter()
                .map(|capability| capability_to_wire(*capability)),
        );
        payload
    }

    /// Decodes a bounded capability payload from an untrusted peer.
    pub(crate) fn decode(payload: &[u8]) -> Result<Self, NegotiationError> {
        if payload.len() < CAPABILITY_PAYLOAD_HEADER_LEN {
            return Err(NegotiationError::InvalidPayload);
        }
        let version = payload[0];
        if version != FANP_CAPABILITY_VERSION {
            return Err(NegotiationError::UnsupportedVersion(version));
        }
        let count = payload[1] as usize;
        if count > MAX_CAPABILITIES || payload.len() != CAPABILITY_PAYLOAD_HEADER_LEN + count {
            return Err(if count > MAX_CAPABILITIES {
                NegotiationError::TooManyCapabilities
            } else {
                NegotiationError::InvalidPayload
            });
        }
        let mut capabilities = BTreeSet::new();
        for wire_value in &payload[CAPABILITY_PAYLOAD_HEADER_LEN..] {
            let capability =
                capability_from_wire(*wire_value).ok_or(NegotiationError::InvalidPayload)?;
            if !capabilities.insert(capability) {
                return Err(NegotiationError::InvalidPayload);
            }
        }
        Self::with_version(version, capabilities)
    }

    /// Computes the capabilities both peers can use for this session.
    pub(crate) fn intersect(
        &self,
        peer: &Self,
    ) -> Result<NegotiatedCapabilities, NegotiationError> {
        if self.protocol_version != peer.protocol_version {
            return Err(NegotiationError::UnsupportedVersion(peer.protocol_version));
        }
        let capabilities = self
            .capabilities
            .intersection(&peer.capabilities)
            .copied()
            .collect::<BTreeSet<_>>();
        if capabilities.is_empty() {
            return Err(NegotiationError::NoCommonCapability);
        }
        Ok(NegotiatedCapabilities {
            protocol_version: self.protocol_version,
            capabilities,
        })
    }
}

/// The capability intersection selected for one FANP session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NegotiatedCapabilities {
    protocol_version: u8,
    capabilities: BTreeSet<Capability>,
}

impl NegotiatedCapabilities {
    /// Returns the selected capability schema version.
    pub const fn protocol_version(&self) -> u8 {
        self.protocol_version
    }

    /// Returns the sorted intersection of both peer offers.
    pub fn capabilities(&self) -> &BTreeSet<Capability> {
        &self.capabilities
    }

    /// Returns whether the negotiated session supports a capability.
    pub fn supports(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        let offer = CapabilityOffer {
            protocol_version: self.protocol_version,
            capabilities: self.capabilities.clone(),
        };
        offer.encode()
    }
}

fn capability_to_wire(capability: Capability) -> u8 {
    match capability {
        Capability::Video => 1,
        Capability::Audio => 2,
        Capability::Subtitles => 3,
        Capability::RemoteControl => 4,
    }
}

fn capability_from_wire(value: u8) -> Option<Capability> {
    match value {
        1 => Some(Capability::Video),
        2 => Some(Capability::Audio),
        3 => Some(Capability::Subtitles),
        4 => Some(Capability::RemoteControl),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_round_trip_in_deterministic_order() {
        let offer = CapabilityOffer::new([
            Capability::RemoteControl,
            Capability::Video,
            Capability::Audio,
        ])
        .unwrap();
        let encoded = offer.encode();
        assert_eq!(encoded, [1, 3, 1, 2, 4]);
        assert_eq!(CapabilityOffer::decode(&encoded), Ok(offer));
    }

    #[test]
    fn malformed_and_duplicate_entries_are_rejected() {
        assert_eq!(
            CapabilityOffer::decode(&[FANP_CAPABILITY_VERSION]),
            Err(NegotiationError::InvalidPayload)
        );
        assert_eq!(
            CapabilityOffer::decode(&[FANP_CAPABILITY_VERSION, 2, 1]),
            Err(NegotiationError::InvalidPayload)
        );
        assert_eq!(
            CapabilityOffer::decode(&[FANP_CAPABILITY_VERSION, 2, 1, 1]),
            Err(NegotiationError::InvalidPayload)
        );
        assert_eq!(
            CapabilityOffer::decode(&[FANP_CAPABILITY_VERSION, 1, 99]),
            Err(NegotiationError::InvalidPayload)
        );
        assert_eq!(
            CapabilityOffer::decode(&[FANP_CAPABILITY_VERSION + 1, 0]),
            Err(NegotiationError::UnsupportedVersion(
                FANP_CAPABILITY_VERSION + 1
            ))
        );
        assert_eq!(
            CapabilityOffer::decode(&[FANP_CAPABILITY_VERSION, (MAX_CAPABILITIES + 1) as u8]),
            Err(NegotiationError::TooManyCapabilities)
        );
    }

    #[test]
    fn intersection_rejects_incompatible_offers() {
        let video = CapabilityOffer::new([Capability::Video]).unwrap();
        let audio = CapabilityOffer::new([Capability::Audio]).unwrap();
        assert_eq!(
            video.intersect(&audio),
            Err(NegotiationError::NoCommonCapability)
        );
        let both = CapabilityOffer::new([Capability::Video, Capability::Audio]).unwrap();
        let negotiated = both.intersect(&video).unwrap();
        assert!(negotiated.supports(Capability::Video));
        assert!(!negotiated.supports(Capability::Audio));
    }
}
