//! Bounded device identity and trust policy for FrameArk pairing adapters.
//!
//! This crate stores only caller-supplied device identifiers, labels, and
//! SHA-256 public-key fingerprints. It does not generate keys, perform
//! signatures, persist records, or replace the platform secure keystore.

use frameark_core::DeviceId;
use std::collections::BTreeMap;
use std::fmt::{Debug, Display, Formatter};

/// Maximum trusted devices retained by one registry.
pub const MAX_TRUSTED_DEVICES: usize = 128;
/// Maximum UTF-8 bytes retained for a human-facing trusted-device label.
pub const MAX_TRUST_LABEL_BYTES: usize = 128;
/// Length of a lowercase colon-separated SHA-256 fingerprint.
pub const SHA256_FINGERPRINT_BYTES: usize = 95;
/// Maximum serialized trust snapshot accepted by the bounded codec.
pub const MAX_SERIALIZED_TRUST_BYTES: usize = 64 * 1024;
const SERIALIZED_MAGIC: &[u8; 4] = b"FTR1";

/// Errors returned when an identity or trust mutation violates policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrustError {
    /// The fingerprint is not a lowercase colon-separated SHA-256 value.
    InvalidFingerprint,
    /// The display label is empty, too large, or contains a control character.
    InvalidLabel,
    /// The registry has reached its explicit capacity.
    Capacity,
    /// A known device presented a different public-key fingerprint.
    IdentityChanged,
    /// A requested trust record does not exist.
    UnknownDevice,
    /// A serialized trust snapshot is malformed or has trailing bytes.
    InvalidEncoding,
    /// A serialized trust snapshot exceeds its explicit bound.
    TooLarge,
}

impl Display for TrustError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidFingerprint => "invalid SHA-256 fingerprint",
            Self::InvalidLabel => "invalid trusted-device label",
            Self::Capacity => "trusted-device capacity exhausted",
            Self::IdentityChanged => "trusted-device identity changed",
            Self::UnknownDevice => "trusted device is unknown",
            Self::InvalidEncoding => "invalid trust snapshot encoding",
            Self::TooLarge => "trust snapshot exceeds its bound",
        })
    }
}

impl std::error::Error for TrustError {}

/// A normalized SHA-256 public-key fingerprint.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeviceFingerprint(String);

impl DeviceFingerprint {
    /// Validates a lowercase `aa:bb:...` SHA-256 fingerprint.
    pub fn new(value: impl Into<String>) -> Result<Self, TrustError> {
        let value = value.into();
        if value.len() != SHA256_FINGERPRINT_BYTES
            || value.bytes().enumerate().any(|(index, byte)| {
                if index % 3 == 2 {
                    byte != b':'
                } else {
                    !matches!(byte, b'0'..=b'9' | b'a'..=b'f')
                }
            })
        {
            return Err(TrustError::InvalidFingerprint);
        }
        Ok(Self(value))
    }

    /// Returns the normalized fingerprint text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Debug for DeviceFingerprint {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("DeviceFingerprint(<redacted>)")
    }
}

impl Display for DeviceFingerprint {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One explicitly trusted device record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustedDevice {
    /// Stable device identifier supplied by discovery or the platform.
    pub device_id: DeviceId,
    /// Public-key fingerprint supplied by the authenticated platform boundary.
    pub fingerprint: DeviceFingerprint,
    /// User-facing label with no control characters.
    pub label: String,
}

/// Outcome of adding or refreshing a trust record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrustMutation {
    /// A new device was added.
    Added,
    /// The same identity was already trusted and its label was refreshed.
    Refreshed,
}

/// In-memory bounded trust registry used by protocol and platform adapters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustRegistry {
    capacity: usize,
    records: BTreeMap<DeviceId, TrustedDevice>,
}

impl Default for TrustRegistry {
    fn default() -> Self {
        Self::with_default_capacity()
    }
}

impl TrustRegistry {
    /// Creates an empty registry with an explicit bounded capacity.
    pub fn new(capacity: usize) -> Result<Self, TrustError> {
        if capacity == 0 || capacity > MAX_TRUSTED_DEVICES {
            return Err(TrustError::Capacity);
        }
        Ok(Self {
            capacity,
            records: BTreeMap::new(),
        })
    }

    /// Creates an empty registry at the maximum supported capacity.
    pub fn with_default_capacity() -> Self {
        Self {
            capacity: MAX_TRUSTED_DEVICES,
            records: BTreeMap::new(),
        }
    }

    /// Returns the number of trusted devices.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns whether no trusted devices are present.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Looks up a trusted record without exposing mutable storage.
    pub fn get(&self, device_id: &DeviceId) -> Option<&TrustedDevice> {
        self.records.get(device_id)
    }

    /// Checks both the stable device identifier and the current key fingerprint.
    pub fn is_trusted(&self, device_id: &DeviceId, fingerprint: &DeviceFingerprint) -> bool {
        self.records
            .get(device_id)
            .is_some_and(|record| &record.fingerprint == fingerprint)
    }

    /// Adds a new record or refreshes the label for the same key identity.
    ///
    /// A changed fingerprint is rejected until the caller explicitly revokes
    /// the old record, preventing silent key replacement during reconnect.
    pub fn trust(
        &mut self,
        device_id: DeviceId,
        fingerprint: DeviceFingerprint,
        label: impl Into<String>,
    ) -> Result<TrustMutation, TrustError> {
        let label = validate_label(label.into())?;
        if let Some(existing) = self.records.get_mut(&device_id) {
            if existing.fingerprint != fingerprint {
                return Err(TrustError::IdentityChanged);
            }
            existing.label = label;
            return Ok(TrustMutation::Refreshed);
        }
        if self.records.len() >= self.capacity {
            return Err(TrustError::Capacity);
        }
        self.records.insert(
            device_id.clone(),
            TrustedDevice {
                device_id,
                fingerprint,
                label,
            },
        );
        Ok(TrustMutation::Added)
    }

    /// Explicitly revokes one device record.
    pub fn revoke(&mut self, device_id: &DeviceId) -> Result<TrustedDevice, TrustError> {
        self.records
            .remove(device_id)
            .ok_or(TrustError::UnknownDevice)
    }

    /// Encodes trusted records for a platform-owned secure storage adapter.
    ///
    /// The versioned `FTR1` snapshot contains no private key material. Records
    /// are sorted by `DeviceId` for deterministic output and the result is
    /// rejected if it exceeds the bounded snapshot size.
    pub fn encode(&self) -> Result<Vec<u8>, TrustError> {
        let count = u16::try_from(self.records.len()).map_err(|_| TrustError::TooLarge)?;
        let capacity = u16::try_from(self.capacity).map_err(|_| TrustError::TooLarge)?;
        let mut output = Vec::with_capacity(8 + self.records.len() * 128);
        output.extend_from_slice(SERIALIZED_MAGIC);
        output.extend_from_slice(&count.to_be_bytes());
        output.extend_from_slice(&capacity.to_be_bytes());
        for record in self.records.values() {
            write_field(&mut output, record.device_id.as_str().as_bytes())?;
            output.extend_from_slice(record.fingerprint.as_str().as_bytes());
            write_field(&mut output, record.label.as_bytes())?;
        }
        if output.len() > MAX_SERIALIZED_TRUST_BYTES {
            return Err(TrustError::TooLarge);
        }
        Ok(output)
    }

    /// Decodes a bounded `FTR1` snapshot produced by [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<Self, TrustError> {
        if bytes.len() > MAX_SERIALIZED_TRUST_BYTES {
            return Err(TrustError::TooLarge);
        }
        if bytes.len() < SERIALIZED_MAGIC.len() + 4
            || &bytes[..SERIALIZED_MAGIC.len()] != SERIALIZED_MAGIC
        {
            return Err(TrustError::InvalidEncoding);
        }
        let mut cursor = SERIALIZED_MAGIC.len();
        let count = read_u16(bytes, &mut cursor)? as usize;
        let capacity = usize::from(read_u16(bytes, &mut cursor)?);
        if capacity == 0 || capacity > MAX_TRUSTED_DEVICES || count > capacity {
            return Err(TrustError::InvalidEncoding);
        }
        if count > MAX_TRUSTED_DEVICES {
            return Err(TrustError::TooLarge);
        }
        let mut registry = Self::new(capacity)?;
        for _ in 0..count {
            let device_id = DeviceId::try_from(read_text(bytes, &mut cursor)?)
                .map_err(|_| TrustError::InvalidEncoding)?;
            let fingerprint = DeviceFingerprint::new(read_exact_text(
                bytes,
                &mut cursor,
                SHA256_FINGERPRINT_BYTES,
            )?)?;
            let label = read_text(bytes, &mut cursor)?;
            if registry.records.contains_key(&device_id) {
                return Err(TrustError::InvalidEncoding);
            }
            registry.trust(device_id, fingerprint, label)?;
        }
        if cursor != bytes.len() {
            return Err(TrustError::InvalidEncoding);
        }
        Ok(registry)
    }
}

fn write_field(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), TrustError> {
    let length = u16::try_from(bytes.len()).map_err(|_| TrustError::TooLarge)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(bytes);
    if output.len() > MAX_SERIALIZED_TRUST_BYTES {
        return Err(TrustError::TooLarge);
    }
    Ok(())
}

fn read_u16(bytes: &[u8], cursor: &mut usize) -> Result<u16, TrustError> {
    let end = cursor.checked_add(2).ok_or(TrustError::InvalidEncoding)?;
    let value = bytes.get(*cursor..end).ok_or(TrustError::InvalidEncoding)?;
    *cursor = end;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}

fn read_text(bytes: &[u8], cursor: &mut usize) -> Result<String, TrustError> {
    let length = usize::from(read_u16(bytes, cursor)?);
    let end = cursor
        .checked_add(length)
        .ok_or(TrustError::InvalidEncoding)?;
    let value = bytes.get(*cursor..end).ok_or(TrustError::InvalidEncoding)?;
    *cursor = end;
    String::from_utf8(value.to_vec()).map_err(|_| TrustError::InvalidEncoding)
}

fn read_exact_text(bytes: &[u8], cursor: &mut usize, length: usize) -> Result<String, TrustError> {
    let end = cursor
        .checked_add(length)
        .ok_or(TrustError::InvalidEncoding)?;
    let value = bytes.get(*cursor..end).ok_or(TrustError::InvalidEncoding)?;
    *cursor = end;
    String::from_utf8(value.to_vec()).map_err(|_| TrustError::InvalidEncoding)
}

fn validate_label(label: String) -> Result<String, TrustError> {
    if label.trim().is_empty()
        || label.len() > MAX_TRUST_LABEL_BYTES
        || label.chars().any(char::is_control)
    {
        return Err(TrustError::InvalidLabel);
    }
    Ok(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FINGERPRINT: &str = "9f:64:a7:47:e1:b9:7f:13:1f:ab:b6:b4:47:29:6c:9b:6f:02:01:e7:9f:b3:c5:35:6e:6c:77:e8:9b:6a:80:6a";

    fn device(value: &str) -> DeviceId {
        DeviceId::try_from(value).unwrap()
    }

    #[test]
    fn fingerprint_requires_lowercase_sha256_text() {
        assert_eq!(
            DeviceFingerprint::new(FINGERPRINT).unwrap().as_str(),
            FINGERPRINT
        );
        assert_eq!(
            DeviceFingerprint::new(FINGERPRINT.to_ascii_uppercase()),
            Err(TrustError::InvalidFingerprint)
        );
        assert_eq!(
            DeviceFingerprint::new("aa:bb"),
            Err(TrustError::InvalidFingerprint)
        );
    }

    #[test]
    fn trust_refreshes_labels_but_rejects_silent_key_rotation() {
        let mut registry = TrustRegistry::new(2).unwrap();
        let fingerprint = DeviceFingerprint::new(FINGERPRINT).unwrap();
        let id = device("phone-1");
        assert_eq!(
            registry
                .trust(id.clone(), fingerprint.clone(), "Living room")
                .unwrap(),
            TrustMutation::Added
        );
        assert!(registry.is_trusted(&id, &fingerprint));
        assert_eq!(
            registry
                .trust(id.clone(), fingerprint.clone(), "Phone")
                .unwrap(),
            TrustMutation::Refreshed
        );
        assert_eq!(registry.get(&id).unwrap().label, "Phone");
        let changed = DeviceFingerprint::new(
            "aa:64:a7:47:e1:b9:7f:13:1f:ab:b6:b4:47:29:6c:9b:6f:02:01:e7:9f:b3:c5:35:6e:6c:77:e8:9b:6a:80:6a",
        )
        .unwrap();
        assert_eq!(
            registry.trust(id, changed, "Rotated"),
            Err(TrustError::IdentityChanged)
        );
    }

    #[test]
    fn revoke_is_explicit_and_capacity_is_bounded() {
        let mut registry = TrustRegistry::new(1).unwrap();
        let id = device("phone-1");
        let fingerprint = DeviceFingerprint::new(FINGERPRINT).unwrap();
        registry
            .trust(id.clone(), fingerprint.clone(), "Phone")
            .unwrap();
        assert_eq!(
            registry.trust(device("laptop-1"), fingerprint.clone(), "Laptop"),
            Err(TrustError::Capacity)
        );
        assert_eq!(registry.revoke(&id).unwrap().device_id, id);
        assert_eq!(registry.revoke(&id), Err(TrustError::UnknownDevice));
        assert!(registry.is_empty());
    }

    #[test]
    fn labels_are_bounded_and_redacted_debug_does_not_expose_fingerprint() {
        let fingerprint = DeviceFingerprint::new(FINGERPRINT).unwrap();
        assert!(!format!("{fingerprint:?}").contains(FINGERPRINT));
        let mut registry = TrustRegistry::with_default_capacity();
        assert_eq!(
            registry.trust(device("phone-1"), fingerprint, "\n"),
            Err(TrustError::InvalidLabel)
        );
        assert_eq!(TrustRegistry::new(0), Err(TrustError::Capacity));
    }

    #[test]
    fn trust_snapshots_round_trip_deterministically() {
        let mut registry = TrustRegistry::new(2).unwrap();
        let fingerprint = DeviceFingerprint::new(FINGERPRINT).unwrap();
        registry
            .trust(device("phone-2"), fingerprint.clone(), "Phone")
            .unwrap();
        registry
            .trust(device("phone-1"), fingerprint, "Tablet")
            .unwrap();
        let encoded = registry.encode().unwrap();
        assert_eq!(&encoded[..4], b"FTR1");
        assert_eq!(TrustRegistry::decode(&encoded).unwrap(), registry);
        assert_eq!(
            TrustRegistry::decode(&encoded).unwrap().encode().unwrap(),
            encoded
        );
    }

    #[test]
    fn trust_snapshot_rejects_truncation_duplicates_and_trailing_bytes() {
        let fingerprint = DeviceFingerprint::new(FINGERPRINT).unwrap();
        let mut registry = TrustRegistry::default();
        registry
            .trust(device("phone-1"), fingerprint, "Phone")
            .unwrap();
        let encoded = registry.encode().unwrap();
        assert_eq!(
            TrustRegistry::decode(&encoded[..encoded.len() - 1]),
            Err(TrustError::InvalidEncoding)
        );
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert_eq!(
            TrustRegistry::decode(&trailing),
            Err(TrustError::InvalidEncoding)
        );
        let mut bad_magic = encoded;
        bad_magic[0] = b'X';
        assert_eq!(
            TrustRegistry::decode(&bad_magic),
            Err(TrustError::InvalidEncoding)
        );
    }
}
