//! Bounded device identity and trust policy for FrameArk pairing adapters.
//!
//! This crate stores only caller-supplied device identifiers, labels, and
//! SHA-256 public-key fingerprints. It does not generate keys, perform
//! signatures, or replace the platform secure keystore. The optional snapshot
//! store persists only the bounded public trust registry; callers still need a
//! platform-protected directory and authenticated user approval.

use frameark_core::DeviceId;
use ring::digest;
use std::collections::BTreeMap;
use std::fmt::{Debug, Display, Formatter};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Maximum trusted devices retained by one registry.
pub const MAX_TRUSTED_DEVICES: usize = 128;
/// Maximum unactioned pairing approvals retained in memory.
pub const MAX_PENDING_TRUST_REQUESTS: usize = 16;
/// Default lifetime for one unactioned pairing approval.
pub const DEFAULT_TRUST_REQUEST_TTL: Duration = Duration::from_secs(300);
/// Maximum lifetime accepted for one unactioned pairing approval.
pub const MAX_TRUST_REQUEST_TTL: Duration = Duration::from_secs(3_600);
/// Maximum UTF-8 bytes retained for a human-facing trusted-device label.
pub const MAX_TRUST_LABEL_BYTES: usize = 128;
/// Length of a lowercase colon-separated SHA-256 fingerprint.
pub const SHA256_FINGERPRINT_BYTES: usize = 95;
/// Raw public-key size accepted by [`DeviceFingerprint::from_public_key`].
pub const SHA256_PUBLIC_KEY_BYTES: usize = 32;
/// Maximum serialized trust snapshot accepted by the bounded codec.
pub const MAX_SERIALIZED_TRUST_BYTES: usize = 64 * 1024;
const SERIALIZED_MAGIC: &[u8; 4] = b"FTR1";

/// Errors returned when an identity or trust mutation violates policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrustError {
    /// The fingerprint is not a lowercase colon-separated SHA-256 value.
    InvalidFingerprint,
    /// The supplied public key does not have the bounded raw-key size.
    InvalidPublicKey,
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
    /// The in-memory approval queue has reached its explicit capacity.
    PendingCapacity,
    /// A caller referenced an approval request that no longer exists.
    UnknownRequest,
    /// An approval request expired before it was acted on.
    RequestExpired,
    /// A caller supplied a zero or unbounded approval lifetime.
    InvalidRequestTtl,
}

/// Redacted errors from the caller-owned snapshot persistence boundary.
#[derive(Debug)]
pub enum TrustStoreError {
    /// The snapshot file could not be read or replaced.
    Io,
    /// The file was read but did not satisfy the bounded FTR1 contract.
    Corrupt(TrustError),
}

impl Display for TrustStoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io => formatter.write_str("trust snapshot storage failed"),
            Self::Corrupt(error) => write!(formatter, "trust snapshot is invalid: {error}"),
        }
    }
}

impl std::error::Error for TrustStoreError {}

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Bounded persistence adapter for an encoded [`TrustRegistry`].
///
/// The adapter writes a private temporary file, flushes it, and replaces the
/// destination with a rename. On platforms whose rename cannot replace an
/// existing file, the destination is removed only after the temporary file is
/// durable; callers should place the path in a platform-protected directory
/// and treat the operation as a best-effort crash-safe replacement. The file
/// contains fingerprints and labels only, never private keys or pairing PINs.
pub struct TrustSnapshotStore {
    path: PathBuf,
}

impl TrustSnapshotStore {
    /// Creates a store at an explicit caller-owned path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Returns the configured path for diagnostics that already apply redaction.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Loads a snapshot, or creates an empty bounded registry when absent.
    pub fn load(&self, capacity: usize) -> Result<TrustRegistry, TrustStoreError> {
        match fs::read(&self.path) {
            Ok(bytes) => TrustRegistry::decode(&bytes).map_err(TrustStoreError::Corrupt),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                TrustRegistry::new(capacity).map_err(TrustStoreError::Corrupt)
            }
            Err(_) => Err(TrustStoreError::Io),
        }
    }

    /// Replaces the snapshot with a bounded, flushed FTR1 representation.
    pub fn save(&self, registry: &TrustRegistry) -> Result<(), TrustStoreError> {
        let encoded = registry.encode().map_err(TrustStoreError::Corrupt)?;
        let temporary = self.temporary_path();
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|_| TrustStoreError::Io)?;
            file.write_all(&encoded).map_err(|_| TrustStoreError::Io)?;
            file.sync_all().map_err(|_| TrustStoreError::Io)?;
            drop(file);
            replace_file(&temporary, &self.path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn temporary_path(&self) -> PathBuf {
        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("trust");
        self.path.with_file_name(format!(".{name}.{sequence}.tmp"))
    }
}

fn replace_file(temporary: &Path, destination: &Path) -> Result<(), TrustStoreError> {
    match fs::rename(temporary, destination) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            fs::remove_file(destination).map_err(|_| TrustStoreError::Io)?;
            fs::rename(temporary, destination).map_err(|_| TrustStoreError::Io)
        }
        Err(_) => Err(TrustStoreError::Io),
    }
}

impl Display for TrustError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidFingerprint => "invalid SHA-256 fingerprint",
            Self::InvalidPublicKey => "invalid raw public key",
            Self::InvalidLabel => "invalid trusted-device label",
            Self::Capacity => "trusted-device capacity exhausted",
            Self::IdentityChanged => "trusted-device identity changed",
            Self::UnknownDevice => "trusted device is unknown",
            Self::InvalidEncoding => "invalid trust snapshot encoding",
            Self::TooLarge => "trust snapshot exceeds its bound",
            Self::PendingCapacity => "pending trust request capacity exhausted",
            Self::UnknownRequest => "trust request is unknown",
            Self::RequestExpired => "trust request expired",
            Self::InvalidRequestTtl => "trust request lifetime is invalid",
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

    /// Derives the canonical fingerprint for one raw 32-byte public key.
    ///
    /// This performs hashing only; it does not authenticate a key or verify a
    /// signature. Callers must invoke it only after a protocol-specific
    /// key-possession proof and explicit user approval have succeeded.
    pub fn from_public_key(public_key: &[u8]) -> Result<Self, TrustError> {
        if public_key.len() != SHA256_PUBLIC_KEY_BYTES {
            return Err(TrustError::InvalidPublicKey);
        }
        let digest = digest::digest(&digest::SHA256, public_key);
        let mut fingerprint = String::with_capacity(SHA256_FINGERPRINT_BYTES);
        for (index, byte) in digest.as_ref().iter().enumerate() {
            if index != 0 {
                fingerprint.push(':');
            }
            use std::fmt::Write as _;
            write!(&mut fingerprint, "{byte:02x}").expect("writing to String cannot fail");
        }
        Self::new(fingerprint)
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

/// One authenticated transport's request for explicit user trust approval.
///
/// The request is deliberately ephemeral and is never included in an FTR1
/// snapshot. The caller must obtain the device ID, fingerprint, and label from
/// an authenticated key-possession boundary before enqueueing it; this type
/// does not verify signatures or certificates itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustRequest {
    id: u64,
    device_id: DeviceId,
    fingerprint: DeviceFingerprint,
    label: String,
    expires_at: Instant,
}

impl TrustRequest {
    /// Returns the opaque request identifier shown to the approval adapter.
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Returns the sender's stable device identifier.
    pub fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    /// Returns the authenticated sender public-key fingerprint.
    pub fn fingerprint(&self) -> &DeviceFingerprint {
        &self.fingerprint
    }

    /// Returns the bounded user-facing sender label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns whether the request is expired at the supplied monotonic time.
    pub fn is_expired(&self, now: Instant) -> bool {
        now >= self.expires_at
    }

    /// Returns the remaining approval lifetime, or zero after expiry.
    pub fn remaining(&self, now: Instant) -> Duration {
        self.expires_at.saturating_duration_since(now)
    }
}

/// Bounded, ephemeral approval queue shared by pairing adapters and UIs.
///
/// Requests are removed only after explicit approve/reject or expiry. Approval
/// delegates identity rotation, capacity, and persistence-independent policy
/// to TrustRegistry. The queue itself contains no private key material and
/// must not be treated as proof that a request was authenticated.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingTrustRequests {
    requests: BTreeMap<u64, TrustRequest>,
    next_id: u64,
}

impl Default for PendingTrustRequests {
    fn default() -> Self {
        Self {
            requests: BTreeMap::new(),
            next_id: 1,
        }
    }
}

impl PendingTrustRequests {
    /// Returns the number of unexpired requests after a monotonic cleanup.
    pub fn len(&mut self, now: Instant) -> usize {
        self.expire(now);
        self.requests.len()
    }

    /// Returns whether no unexpired requests remain.
    pub fn is_empty(&mut self, now: Instant) -> bool {
        self.len(now) == 0
    }

    /// Returns a request by ID after removing expired entries.
    pub fn get(&mut self, id: u64, now: Instant) -> Option<&TrustRequest> {
        self.expire(now);
        self.requests.get(&id)
    }

    /// Returns a deterministic snapshot of unexpired requests for a UI.
    pub fn list(&mut self, now: Instant) -> Vec<TrustRequest> {
        self.expire(now);
        self.requests.values().cloned().collect()
    }

    /// Queues a request using the default bounded lifetime.
    pub fn begin(
        &mut self,
        device_id: DeviceId,
        fingerprint: DeviceFingerprint,
        label: impl Into<String>,
    ) -> Result<TrustRequest, TrustError> {
        self.begin_at(
            device_id,
            fingerprint,
            label,
            DEFAULT_TRUST_REQUEST_TTL,
            Instant::now(),
        )
    }

    /// Queues a request with an explicit bounded lifetime.
    pub fn begin_with_ttl(
        &mut self,
        device_id: DeviceId,
        fingerprint: DeviceFingerprint,
        label: impl Into<String>,
        ttl: Duration,
    ) -> Result<TrustRequest, TrustError> {
        self.begin_at(device_id, fingerprint, label, ttl, Instant::now())
    }

    fn begin_at(
        &mut self,
        device_id: DeviceId,
        fingerprint: DeviceFingerprint,
        label: impl Into<String>,
        ttl: Duration,
        now: Instant,
    ) -> Result<TrustRequest, TrustError> {
        if ttl.is_zero() || ttl > MAX_TRUST_REQUEST_TTL {
            return Err(TrustError::InvalidRequestTtl);
        }
        let label = validate_label(label.into())?;
        self.expire(now);
        if let Some(existing) = self
            .requests
            .values_mut()
            .find(|request| request.device_id == device_id && request.fingerprint == fingerprint)
        {
            existing.label = label;
            existing.expires_at = now + ttl;
            return Ok(existing.clone());
        }
        if self.requests.len() >= MAX_PENDING_TRUST_REQUESTS {
            return Err(TrustError::PendingCapacity);
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let request = TrustRequest {
            id,
            device_id,
            fingerprint,
            label,
            expires_at: now + ttl,
        };
        self.requests.insert(id, request.clone());
        Ok(request)
    }

    /// Approves a live request and removes it only after the registry accepts it.
    pub fn approve(
        &mut self,
        id: u64,
        registry: &mut TrustRegistry,
        now: Instant,
    ) -> Result<TrustMutation, TrustError> {
        let request = self
            .requests
            .get(&id)
            .cloned()
            .ok_or(TrustError::UnknownRequest)?;
        if request.is_expired(now) {
            self.requests.remove(&id);
            return Err(TrustError::RequestExpired);
        }
        let mutation = registry.trust(
            request.device_id.clone(),
            request.fingerprint.clone(),
            request.label.clone(),
        )?;
        self.requests.remove(&id);
        Ok(mutation)
    }

    /// Rejects one live or expired request without mutating the trust registry.
    pub fn reject(&mut self, id: u64) -> Result<TrustRequest, TrustError> {
        self.requests.remove(&id).ok_or(TrustError::UnknownRequest)
    }

    /// Removes all requests whose monotonic deadline has elapsed.
    pub fn expire(&mut self, now: Instant) -> usize {
        let before = self.requests.len();
        self.requests.retain(|_, request| !request.is_expired(now));
        before - self.requests.len()
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

    /// Adds or refreshes a peer after its protocol proof was verified.
    ///
    /// The registry derives the canonical fingerprint from the raw public key
    /// so callers cannot accidentally persist a differently formatted value.
    /// This method still does not verify signatures; callers must establish
    /// key possession and obtain explicit approval before invoking it.
    pub fn trust_public_key(
        &mut self,
        device_id: DeviceId,
        public_key: &[u8],
        label: impl Into<String>,
    ) -> Result<TrustMutation, TrustError> {
        let fingerprint = DeviceFingerprint::from_public_key(public_key)?;
        self.trust(device_id, fingerprint, label)
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
    fn public_key_fingerprint_is_canonical_and_bounded() {
        let public_key: Vec<u8> = (0..32).collect();
        assert_eq!(
            DeviceFingerprint::from_public_key(&public_key)
                .unwrap()
                .as_str(),
            "63:0d:cd:29:66:c4:33:66:91:12:54:48:bb:b2:5b:4f:f4:12:a4:9c:73:2d:b2:c8:ab:c1:b8:58:1b:d7:10:dd"
        );
        assert_eq!(
            DeviceFingerprint::from_public_key(&public_key[..31]),
            Err(TrustError::InvalidPublicKey)
        );
    }

    #[test]
    fn trust_public_key_binds_identity_to_derived_fingerprint() {
        let public_key: Vec<u8> = (0..32).collect();
        let id = device("phone-1");
        let mut registry = TrustRegistry::new(1).unwrap();
        assert_eq!(
            registry
                .trust_public_key(id.clone(), &public_key, "Phone")
                .unwrap(),
            TrustMutation::Added
        );
        let fingerprint = DeviceFingerprint::from_public_key(&public_key).unwrap();
        assert!(registry.is_trusted(&id, &fingerprint));
        assert_eq!(
            registry.trust_public_key(id, &[7; 32], "Rotated"),
            Err(TrustError::IdentityChanged)
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
    fn pending_requests_are_bounded_deduplicated_and_expiring() {
        let fingerprint = DeviceFingerprint::new(FINGERPRINT).unwrap();
        let now = Instant::now();
        let mut pending = PendingTrustRequests::default();
        let first = pending
            .begin_at(
                device("phone-1"),
                fingerprint.clone(),
                "Phone",
                Duration::from_secs(30),
                now,
            )
            .unwrap();
        assert_eq!(first.id(), 1);
        assert_eq!(pending.len(now), 1);
        let refreshed = pending
            .begin_at(
                device("phone-1"),
                fingerprint,
                "Living room",
                Duration::from_secs(60),
                now,
            )
            .unwrap();
        assert_eq!(refreshed.id(), first.id());
        assert_eq!(refreshed.label(), "Living room");
        assert_eq!(pending.len(now), 1);
        for index in 0..MAX_PENDING_TRUST_REQUESTS - 1 {
            pending
                .begin_at(
                    device(&format!("peer-{index}")),
                    DeviceFingerprint::new(FINGERPRINT).unwrap(),
                    "Peer",
                    Duration::from_secs(30),
                    now,
                )
                .unwrap();
        }
        assert_eq!(pending.len(now), MAX_PENDING_TRUST_REQUESTS);
        assert_eq!(
            pending.begin_at(
                device("peer-overflow"),
                DeviceFingerprint::new(FINGERPRINT).unwrap(),
                "Peer",
                Duration::from_secs(30),
                now,
            ),
            Err(TrustError::PendingCapacity)
        );
        assert_eq!(
            pending.begin_at(
                device("phone-2"),
                DeviceFingerprint::new(FINGERPRINT).unwrap(),
                "Phone",
                Duration::ZERO,
                now,
            ),
            Err(TrustError::InvalidRequestTtl)
        );
        assert_eq!(
            pending.expire(now + Duration::from_secs(61)),
            MAX_PENDING_TRUST_REQUESTS
        );
        assert!(pending.is_empty(now + Duration::from_secs(61)));
    }

    #[test]
    fn pending_approval_commits_only_after_registry_accepts() {
        let fingerprint = DeviceFingerprint::new(FINGERPRINT).unwrap();
        let now = Instant::now();
        let mut pending = PendingTrustRequests::default();
        let request = pending
            .begin_at(
                device("phone-1"),
                fingerprint.clone(),
                "Phone",
                Duration::from_secs(30),
                now,
            )
            .unwrap();
        let mut registry = TrustRegistry::new(1).unwrap();
        assert_eq!(
            pending.approve(request.id(), &mut registry, now),
            Ok(TrustMutation::Added)
        );
        assert!(registry.is_trusted(&device("phone-1"), &fingerprint));
        assert_eq!(
            pending.approve(request.id(), &mut registry, now),
            Err(TrustError::UnknownRequest)
        );

        let second = pending
            .begin_at(
                device("phone-2"),
                DeviceFingerprint::new(FINGERPRINT).unwrap(),
                "Tablet",
                Duration::from_secs(30),
                now,
            )
            .unwrap();
        assert_eq!(
            pending.approve(second.id(), &mut registry, now),
            Err(TrustError::Capacity)
        );
        assert!(pending.get(second.id(), now).is_some());
        assert_eq!(pending.reject(second.id()).unwrap().label(), "Tablet");
    }

    #[test]
    fn expired_approval_is_removed_and_fingerprint_debug_is_redacted() {
        let fingerprint = DeviceFingerprint::new(FINGERPRINT).unwrap();
        let now = Instant::now();
        let mut pending = PendingTrustRequests::default();
        let request = pending
            .begin_at(
                device("phone-1"),
                fingerprint,
                "Phone",
                Duration::from_secs(1),
                now,
            )
            .unwrap();
        assert!(!format!("{request:?}").contains(FINGERPRINT));
        let mut registry = TrustRegistry::default();
        assert_eq!(
            pending.approve(request.id(), &mut registry, now + Duration::from_secs(1),),
            Err(TrustError::RequestExpired)
        );
        assert!(pending.is_empty(now + Duration::from_secs(1)));
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

    #[test]
    fn snapshot_store_loads_missing_file_and_replaces_existing_snapshot() {
        let path = std::env::temp_dir().join(format!(
            "frameark-trust-{}-{}.ftr",
            std::process::id(),
            TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let store = TrustSnapshotStore::new(&path);
        let mut registry = store.load(2).unwrap();
        let fingerprint = DeviceFingerprint::new(FINGERPRINT).unwrap();
        registry
            .trust(device("phone-1"), fingerprint.clone(), "Phone")
            .unwrap();
        store.save(&registry).unwrap();
        assert_eq!(store.load(2).unwrap(), registry);

        registry
            .trust(device("phone-2"), fingerprint, "Tablet")
            .unwrap();
        store.save(&registry).unwrap();
        assert_eq!(store.load(2).unwrap(), registry);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn snapshot_store_reports_corrupt_file_without_exposing_path() {
        let path = std::env::temp_dir().join(format!(
            "frameark-trust-corrupt-{}-{}.ftr",
            std::process::id(),
            TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, b"not-ftr1").unwrap();
        let error = TrustSnapshotStore::new(&path).load(1).unwrap_err();
        assert!(matches!(
            error,
            TrustStoreError::Corrupt(TrustError::InvalidEncoding)
        ));
        assert!(!error.to_string().contains(path.to_string_lossy().as_ref()));
        let _ = fs::remove_file(path);
    }
}
