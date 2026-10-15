//! JNI entry points kept deliberately small while the Android media bridge is
//! introduced in bounded lifecycle increments.
//!
//! This crate is the only workspace exception to the `unsafe_code` lint because
//! Rust 2024 requires an unsafe attribute for the exported JNI symbol. The
//! function body itself does not dereference raw pointers or access JNI state.

#![allow(unsafe_code)]

mod dlna_output;
mod dlna_runtime;
mod dlna_source;
#[cfg(feature = "android-playback-probe")]
mod playback_probe;
mod sender;
mod sender_discovery;

use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use frameark_api::{AudioFrame, AudioRenderer, EncodedMediaSink, VideoFrame, VideoRenderer};
use frameark_core::{
    AudioConfig, CORE_ABI_VERSION, DeviceId, MediaCodec, Session, SessionId, SessionState,
    VideoConfig,
};
use frameark_discovery::{DiscoveryConfig, MdnsDiscovery, PublishedRegistration, PublishedService};
use frameark_native::media_session::MediaSession;
use frameark_native::{
    MediaRendererFactory, NativeMediaControlReceiver, ReceiverPolicy, SessionBackend, SessionOffer,
};
use frameark_transport::{PairingCode, PairingServer, PeerIdentityProof};
use frameark_trust::{DeviceFingerprint, TrustError, TrustRegistry};
use jni::{
    JNIEnv,
    objects::{JByteArray, JObject, JString},
    sys::{jboolean, jbyteArray, jint, jlong},
};

/// Maximum encoded frame accepted at the JNI boundary.
pub const MAX_JNI_MEDIA_FRAME_BYTES: usize = 4 * 1024 * 1024;
/// Maximum encoded frames retained before the producer receives backpressure.
pub const MAX_JNI_MEDIA_QUEUE_FRAMES: usize = 16;
/// Maximum verified sender-proof requests retained across the JNI boundary.
pub const MAX_JNI_PROOF_EVENTS: usize = 16;
const MEDIA_FRAME_MAGIC: &[u8; 4] = b"FAMF";
const MEDIA_FRAME_VERSION: u8 = 1;
const MEDIA_KIND_VIDEO: u8 = 1;
const MEDIA_KIND_AUDIO: u8 = 2;
const MEDIA_CONFIG_MAGIC: &[u8; 4] = b"FAMC";
const MEDIA_CONFIG_VERSION: u8 = 1;
const PROOF_EVENT_MAGIC: &[u8; 4] = b"FAPR";
const PROOF_EVENT_HEADER_BYTES: usize = 14;
const PROOF_DECISION_POLL_INTERVAL: Duration = Duration::from_millis(50);
const NETWORK_SERVER_NAME: &str = "frameark.local";
const DISCOVERY_INSTANCE_NAME: &str = "FrameArk Receiver";
const DISCOVERY_HOST_NAME: &str = "frameark.local.";
const NETWORK_OPERATION_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_PLAYBACK_EVENTS: usize = 128;
const MAX_PLAYBACK_BYTES: usize = 16 * 1024 * 1024;

/// Returns the core ABI version expected by the Android receiver.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeVersion(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jint {
    CORE_ABI_VERSION as jint
}

/// Installs the Android Keystore-backed public identity before startup.
///
/// Only the stable alias and lowercase SHA-256 public-key fingerprint cross
/// JNI. Private key material remains inside Android Keystore. A return value of
/// 0 means accepted (including the same identity on a running receiver), -2
/// means an identity change was rejected while running, and -3 means
/// the identity fields are malformed or the state lock is unavailable.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeSetDeviceIdentity(
    mut env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    alias: JString<'_>,
    fingerprint: JString<'_>,
) -> jint {
    let Ok(alias) = env.get_string(&alias) else {
        return -3;
    };
    let Ok(fingerprint) = env.get_string(&fingerprint) else {
        return -3;
    };
    let alias = alias.to_string_lossy().into_owned();
    let fingerprint = fingerprint.to_string_lossy().into_owned();
    let Ok(mut state) = receiver_state().lock() else {
        return -3;
    };
    install_device_identity(&mut state, &alias, &fingerprint)
}

/// Adds or refreshes one explicitly approved peer in Rust's bounded registry.
/// Return values: 0 accepted, -2 key changed, -3 malformed/lock failure,
/// -4 registry capacity exhausted, -5 receiver running/stopping.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeTrustPeer(
    mut env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    device_id: JString<'_>,
    fingerprint: JString<'_>,
    label: JString<'_>,
) -> jint {
    let Ok(device_id) = env.get_string(&device_id) else {
        return -3;
    };
    let Ok(fingerprint) = env.get_string(&fingerprint) else {
        return -3;
    };
    let Ok(label) = env.get_string(&label) else {
        return -3;
    };
    let Ok(device_id) = DeviceId::try_from(device_id.to_string_lossy().into_owned()) else {
        return -3;
    };
    let Ok(fingerprint) = DeviceFingerprint::new(fingerprint.to_string_lossy().into_owned()) else {
        return -3;
    };
    let label = label.to_string_lossy().into_owned();
    let Ok(mut state) = receiver_state().lock() else {
        return -3;
    };
    mutate_trust_registry(&mut state, |registry| {
        registry.trust(device_id, fingerprint, label).map(|_| ())
    })
}

/// Adds or refreshes an explicitly approved peer from its verified raw public
/// key. The proof/signature check and user approval happen before this JNI
/// entry point; Rust only derives the canonical trust fingerprint here.
/// Return values: 0 accepted, -2 key changed, -3 malformed/lock failure,
/// -4 registry capacity exhausted, -5 receiver running/stopping.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeTrustPeerPublicKey(
    mut env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    device_id: JString<'_>,
    public_key: JByteArray<'_>,
    label: JString<'_>,
) -> jint {
    if !matches!(env.get_array_length(&public_key), Ok(32 | 65)) {
        return -3;
    }
    let Ok(device_id) = env.get_string(&device_id) else {
        return -3;
    };
    let Ok(label) = env.get_string(&label) else {
        return -3;
    };
    let Ok(public_key) = env.convert_byte_array(&public_key) else {
        return -3;
    };
    let Ok(device_id) = DeviceId::try_from(device_id.to_string_lossy().into_owned()) else {
        return -3;
    };
    let label = label.to_string_lossy().into_owned();
    let Ok(mut state) = receiver_state().lock() else {
        return -3;
    };
    mutate_trust_registry(&mut state, |registry| {
        registry
            .trust_public_key(device_id, &public_key, label)
            .map(|_| ())
    })
}

/// Explicitly revokes one trusted peer. Return values: 0 revoked, -3 unknown
/// or malformed/lock failure, -5 receiver running/stopping. A rejected call
/// never reports that a live listener's startup trust snapshot was revoked.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeRevokePeer(
    mut env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    device_id: JString<'_>,
) -> jint {
    let Ok(device_id) = env.get_string(&device_id) else {
        return -3;
    };
    let Ok(device_id) = DeviceId::try_from(device_id.to_string_lossy().into_owned()) else {
        return -3;
    };
    let Ok(mut state) = receiver_state().lock() else {
        return -3;
    };
    mutate_trust_registry(&mut state, |registry| {
        registry.revoke(&device_id).map(|_| ())
    })
}

/// Checks one peer/fingerprint pair without exposing the registry contents.
/// Return values: 1 trusted, 0 not trusted, -3 malformed/lock failure.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeIsPeerTrusted(
    mut env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    device_id: JString<'_>,
    fingerprint: JString<'_>,
) -> jint {
    let Ok(device_id) = env.get_string(&device_id) else {
        return -3;
    };
    let Ok(fingerprint) = env.get_string(&fingerprint) else {
        return -3;
    };
    let Ok(device_id) = DeviceId::try_from(device_id.to_string_lossy().into_owned()) else {
        return -3;
    };
    let Ok(fingerprint) = DeviceFingerprint::new(fingerprint.to_string_lossy().into_owned()) else {
        return -3;
    };
    let Ok(state) = receiver_state().lock() else {
        return -3;
    };
    if state.trust_registry.is_trusted(&device_id, &fingerprint) {
        1
    } else {
        0
    }
}

/// Exports the bounded public trust registry snapshot for caller-owned secure
/// storage. The returned FTR1 bytes contain no private keys or pairing codes;
/// a null result means the state lock or snapshot encoder was unavailable.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeExportTrustSnapshot(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jbyteArray {
    let Ok(state) = receiver_state().lock() else {
        return std::ptr::null_mut();
    };
    let Some(snapshot) = export_trust_snapshot(&state) else {
        return std::ptr::null_mut();
    };
    env.byte_array_from_slice(&snapshot)
        .map(|array| array.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Imports a bounded public trust registry snapshot before a session starts.
/// Return values: 0 accepted, -2 active session, -3 malformed/lock failure.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeImportTrustSnapshot(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    snapshot: JByteArray<'_>,
) -> jint {
    let Ok(bytes) = env.convert_byte_array(&snapshot) else {
        return -3;
    };
    let Ok(mut state) = receiver_state().lock() else {
        return -3;
    };
    import_trust_snapshot(&mut state, &bytes)
}

/// Enables or disables the optional one-shot post-PIN pairing approval gate.
/// Return values: 0 accepted while stopped, -2 active receiver, -3 lock
/// failure. This gate is an authorization seam only; it does not authenticate
/// a sender identity or persist trust.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeSetPairingApprovalRequired(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    required: jboolean,
) -> jint {
    let Ok(mut state) = receiver_state().lock() else {
        return -3;
    };
    if state.is_active() {
        return -2;
    }
    state.pairing_approval_required = required != 0;
    state.pairing_approval.store(false, Ordering::Release);
    0
}

/// Arms exactly one upcoming FANP pairing when the optional approval gate is
/// enabled. Return values: 0 armed, -2 gate disabled/not running, -3 lock
/// failure. The arm is consumed before `PairAccepted` is sent.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeApproveNextPairing(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jint {
    let Ok(state) = receiver_state().lock() else {
        return -3;
    };
    if !state.pairing_approval_required || state.network.is_none() {
        return -2;
    }
    state.pairing_approval.store(true, Ordering::Release);
    0
}

/// Enables or disables the optional Rust-verified sender-proof approval flow.
/// Return values: 0 accepted while stopped, -2 active receiver, -3 lock
/// failure. When enabled, the next network session requires a FANP Ed25519/P-256
/// proof and exposes only a bounded public-key event to Android.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeSetPeerProofRequired(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    required: jboolean,
) -> jint {
    let Ok(mut state) = receiver_state().lock() else {
        return -3;
    };
    if state.is_active() {
        return -2;
    }
    state.peer_proof_required = required != 0;
    state.peer_proof_bridge.clear();
    0
}

/// Polls one Rust-verified sender-proof event. The bounded `FAPR` v1/v2 envelope
/// contains only a request id, device id, and canonical public key; proof
/// signatures, addresses, pairing codes, and private material never cross JNI.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativePollPeerProof(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jbyteArray {
    let Some(event) = receiver_state()
        .lock()
        .ok()
        .and_then(|state| state.peer_proof_bridge.pop())
    else {
        return std::ptr::null_mut();
    };
    env.byte_array_from_slice(&encode_peer_proof_event(&event))
        .map(|array| array.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Resolves one proof request after Android has shown the copied public key to
/// the user. Return values: 0 accepted, -2 unknown/expired request, -3 lock
/// failure or malformed request id.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeDecidePeerProof(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    request_id: jlong,
    approved: jboolean,
) -> jint {
    if request_id <= 0 {
        return -3;
    }
    let Ok(state) = receiver_state().lock() else {
        return -3;
    };
    if state
        .peer_proof_bridge
        .decide(request_id as u64, approved != 0)
    {
        0
    } else {
        -2
    }
}

/// Returns 1 only while this request can still receive its first decision.
/// Expiry, cancellation, stop and already-decided requests return 0.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativePeerProofPending(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    request_id: jlong,
) -> jint {
    if request_id <= 0 {
        return -3;
    }
    let Ok(state) = receiver_state().lock() else {
        return -3;
    };
    i32::from(state.peer_proof_bridge.is_pending(request_id as u64))
}

fn mutate_trust_registry(
    state: &mut NativeReceiverState,
    mutation: impl FnOnce(&mut TrustRegistry) -> Result<(), TrustError>,
) -> jint {
    if state.is_active() {
        return -5;
    }
    match mutation(&mut state.trust_registry) {
        Ok(()) => 0,
        Err(TrustError::IdentityChanged) => -2,
        Err(TrustError::Capacity) => -4,
        Err(_) => -3,
    }
}

fn export_trust_snapshot(state: &NativeReceiverState) -> Option<Vec<u8>> {
    state.trust_registry.encode().ok()
}

fn import_trust_snapshot(state: &mut NativeReceiverState, bytes: &[u8]) -> jint {
    let Ok(registry) = TrustRegistry::decode(bytes) else {
        return -3;
    };
    if state.is_active() {
        return -2;
    }
    state.trust_registry = registry;
    0
}

fn install_device_identity(
    state: &mut NativeReceiverState,
    alias: &str,
    fingerprint: &str,
) -> jint {
    if alias.is_empty()
        || alias.len() > 64
        || !alias.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
        || !valid_identity_fingerprint(fingerprint)
    {
        return -3;
    }
    // Keystore aliases are local labels shared by every installation, not IDs.
    let Ok(device_id) = DeviceId::try_from(format!("sha256-{}", fingerprint.replace(':', "")))
    else {
        return -3;
    };
    if state.identity_alias.as_deref() == Some(alias)
        && state.device_id.as_ref() == Some(&device_id)
        && state.identity_fingerprint.as_deref() == Some(fingerprint)
    {
        return 0;
    }
    if state.is_active() {
        return -2;
    }
    state.identity_alias = Some(alias.to_owned());
    state.device_id = Some(device_id);
    state.identity_fingerprint = Some(fingerprint.to_owned());
    0
}

fn prepare_receiver_session(state: &NativeReceiverState) -> Result<Session, jint> {
    if state.stopping {
        return Err(-2);
    }
    let device_id = state.device_id.clone().ok_or(-5)?;
    if state.identity_fingerprint.is_none() {
        return Err(-5);
    }
    let session_id = SessionId::try_from("android-receiver").map_err(|_| -1)?;
    let mut session = Session::new(session_id, device_id);
    session
        .transition(SessionState::Connecting)
        .map_err(|_| -1)?;
    Ok(session)
}

fn valid_identity_fingerprint(value: &str) -> bool {
    value.len() == 95
        && value.bytes().enumerate().all(|(index, byte)| {
            if index % 3 == 2 {
                byte == b':'
            } else {
                byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
            }
        })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingPeerProof {
    request_id: u64,
    device_id: String,
    public_key: Vec<u8>,
    expires_at: Instant,
}

#[derive(Default)]
struct PeerProofBridgeState {
    next_request_id: u64,
    queued: VecDeque<PendingPeerProof>,
    pending: BTreeMap<u64, PendingPeerProof>,
    decisions: BTreeMap<u64, bool>,
}

#[derive(Clone, Default)]
struct PeerProofBridge {
    state: Arc<Mutex<PeerProofBridgeState>>,
}

// Removing a request must also happen when its async waiter is dropped.
struct PeerProofWaitGuard<'a> {
    bridge: &'a PeerProofBridge,
    request_id: u64,
}

impl Drop for PeerProofWaitGuard<'_> {
    fn drop(&mut self) {
        self.bridge.remove(self.request_id);
    }
}

impl PeerProofBridgeState {
    fn expire(&mut self) {
        let now = Instant::now();
        self.pending.retain(|_, request| request.expires_at > now);
        self.queued
            .retain(|request| self.pending.contains_key(&request.request_id));
        self.decisions.retain(|id, _| self.pending.contains_key(id));
    }
}

impl PeerProofBridge {
    fn enqueue(&self, proof: &PeerIdentityProof) -> Option<u64> {
        self.enqueue_with_timeout(proof, NETWORK_OPERATION_TIMEOUT)
    }

    fn enqueue_with_timeout(&self, proof: &PeerIdentityProof, ttl: Duration) -> Option<u64> {
        if ttl.is_zero() || ttl > NETWORK_OPERATION_TIMEOUT {
            return None;
        }
        let Ok(mut state) = self.state.lock() else {
            return None;
        };
        state.expire();
        if proof.device_id.is_empty()
            || proof.device_id.len() > 128
            || DeviceFingerprint::from_public_key(&proof.public_key).is_err()
            || state.pending.len() >= MAX_JNI_PROOF_EVENTS
            || state
                .pending
                .values()
                .any(|pending| pending.device_id == proof.device_id)
            || state
                .queued
                .iter()
                .any(|pending| pending.device_id == proof.device_id)
        {
            return None;
        }
        // JNI carries signed ids. Never wrap/reuse an id after stop or expiry.
        state.next_request_id = state.next_request_id.checked_add(1)?;
        if state.next_request_id > i64::MAX as u64 {
            return None;
        }
        let event = PendingPeerProof {
            request_id: state.next_request_id,
            device_id: proof.device_id.clone(),
            public_key: proof.public_key.clone(),
            expires_at: Instant::now() + ttl,
        };
        state.pending.insert(event.request_id, event.clone());
        state.queued.push_back(event.clone());
        Some(event.request_id)
    }

    fn pop(&self) -> Option<PendingPeerProof> {
        let mut state = self.state.lock().ok()?;
        state.expire();
        state.queued.pop_front()
    }

    fn is_pending(&self, request_id: u64) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        state.expire();
        state.pending.contains_key(&request_id) && !state.decisions.contains_key(&request_id)
    }

    fn decide(&self, request_id: u64, approved: bool) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        state.expire();
        if !state.pending.contains_key(&request_id) || state.decisions.contains_key(&request_id) {
            return false;
        }
        state.decisions.insert(request_id, approved);
        state.queued.retain(|event| event.request_id != request_id);
        true
    }

    async fn wait_for_decision(&self, request_id: u64, timeout: Duration) -> bool {
        let _cleanup = PeerProofWaitGuard {
            bridge: self,
            request_id,
        };
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Ok(mut state) = self.state.lock() {
                state.expire();
                if !state.pending.contains_key(&request_id) {
                    return false;
                }
                if let Some(decision) = state.decisions.remove(&request_id) {
                    state.pending.remove(&request_id);
                    return decision;
                }
            } else {
                return false;
            }
            if tokio::time::Instant::now() >= deadline {
                self.remove(request_id);
                return false;
            }
            tokio::time::sleep(PROOF_DECISION_POLL_INTERVAL).await;
        }
    }

    fn remove(&self, request_id: u64) {
        if let Ok(mut state) = self.state.lock() {
            state.pending.remove(&request_id);
            state.decisions.remove(&request_id);
            state.queued.retain(|event| event.request_id != request_id);
        }
    }

    fn clear(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.queued.clear();
            state.pending.clear();
            state.decisions.clear();
        }
    }
}

fn encode_peer_proof_event(event: &PendingPeerProof) -> Vec<u8> {
    let device_id = event.device_id.as_bytes();
    let mut encoded =
        Vec::with_capacity(PROOF_EVENT_HEADER_BYTES + device_id.len() + event.public_key.len());
    encoded.extend_from_slice(PROOF_EVENT_MAGIC);
    encoded.push(if event.public_key.len() == 32 { 1 } else { 2 });
    encoded.extend_from_slice(&event.request_id.to_be_bytes());
    encoded.push(device_id.len() as u8);
    encoded.extend_from_slice(device_id);
    encoded.extend_from_slice(&event.public_key);
    encoded
}

#[derive(Default)]
struct NativeReceiverState {
    // Remains true while shutdown joins outside the state lock. This prevents
    // mutations/restart from racing the old listener's trust/media snapshot.
    stopping: bool,
    session: Option<Session>,
    media_queue: VecDeque<NativeMediaFrame>,
    playback_queue: PlaybackQueue,
    video_config: Option<VideoConfig>,
    audio_config: Option<AudioConfig>,
    identity_alias: Option<String>,
    device_id: Option<DeviceId>,
    identity_fingerprint: Option<String>,
    trust_registry: TrustRegistry,
    pairing_approval_required: bool,
    pairing_approval: Arc<AtomicBool>,
    peer_proof_required: bool,
    peer_proof_bridge: PeerProofBridge,
    network: Option<NetworkRuntime>,
}

/// Ordered platform events: config and teardown cannot overtake media.
#[derive(Default)]
struct PlaybackQueue {
    events: VecDeque<Vec<u8>>,
    bytes: usize,
    clock: Option<(i64, Instant)>,
    arrival_clock: Option<(i64, Instant)>,
}

impl PlaybackQueue {
    fn push(&mut self, kind: u8, payload: &[u8]) -> Result<(), ()> {
        let size = payload.len().checked_add(6).ok_or(())?;
        if self.events.len() >= MAX_PLAYBACK_EVENTS || size > MAX_PLAYBACK_BYTES - self.bytes {
            return Err(());
        }
        if matches!(kind, 3 | 4) {
            let pts = i64::from_be_bytes(payload.get(7..15).ok_or(())?.try_into().map_err(|_| ())?);
            let (origin, started) = *self.arrival_clock.get_or_insert((pts, Instant::now()));
            if i128::from(pts) - i128::from(origin) - started.elapsed().as_micros() as i128
                > 5_000_000
            {
                return Err(());
            }
        }
        let mut event = Vec::with_capacity(size);
        event.extend_from_slice(b"FAPE");
        event.extend_from_slice(&[1, kind]);
        event.extend_from_slice(payload);
        self.bytes += size;
        self.events.push_back(event);
        Ok(())
    }

    fn pop(&mut self) -> Option<Vec<u8>> {
        let next = self.events.front()?;
        if matches!(next[5], 3 | 4) {
            let pts = i64::from_be_bytes(next.get(13..21)?.try_into().ok()?);
            let (origin, started) = *self.clock.get_or_insert((pts, Instant::now()));
            if i128::from(pts) - i128::from(origin) > started.elapsed().as_micros() as i128 {
                return None;
            }
        }
        let event = self.events.pop_front()?;
        self.bytes -= event.len();
        if event[5] == 5 {
            self.clock = None;
            // A prepare reset may precede already queued frames. Their arrival
            // origin still bounds the current session until its next prepare.
            if self.events.is_empty() {
                self.arrival_clock = None;
            }
        }
        Some(event)
    }

    fn clear(&mut self) {
        self.events.clear();
        self.bytes = 0;
        self.clock = None;
        self.arrival_clock = None;
    }

    fn reset(&mut self, discard: bool) {
        if discard || self.push(5, &[]).is_err() {
            self.clear();
            // A reset must never be lost to producer backpressure.
            let _ = self.push(5, &[]);
        }
    }
}

fn enqueue_playback(kind: u8, payload: &[u8]) -> Result<(), ()> {
    let mut state = receiver_state().lock().map_err(|_| ())?;
    if state.stopping || state.session.is_none() {
        #[cfg(feature = "android-playback-probe")]
        playback_probe::record_failure("playback.inactive");
        return Err(());
    }
    let result = state.playback_queue.push(kind, payload);
    #[cfg(feature = "android-playback-probe")]
    if result.is_err() {
        playback_probe::record_failure("playback.queue_unavailable");
    }
    result
}

fn reset_playback(discard: bool) {
    if let Ok(mut state) = receiver_state().lock() {
        state.playback_queue.reset(discard);
    }
}

/// FAPE1 events contain ordered configuration, microsecond frames and reset.
/// The legacy FAMF polling seam is separate and is not the network player.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativePollPlaybackEvent(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jbyteArray {
    let Some(event) = receiver_state()
        .lock()
        .ok()
        .and_then(|mut state| state.playback_queue.pop())
    else {
        return std::ptr::null_mut();
    };
    env.byte_array_from_slice(&event)
        .map(|array| array.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

impl NativeReceiverState {
    fn is_active(&self) -> bool {
        self.stopping || self.session.is_some() || self.network.is_some()
    }
}

struct ReceiverShutdown {
    network: Option<NetworkRuntime>,
    transition_failed: bool,
}

fn begin_receiver_stop(state: &mut NativeReceiverState) -> Result<Option<ReceiverShutdown>, jint> {
    if state.stopping {
        return Err(-2);
    }
    if !state.is_active() {
        return Ok(None);
    }
    state.stopping = true;
    let transition_failed = state
        .session
        .take()
        .is_some_and(|mut session| session.transition(SessionState::Closed).is_err());
    state.media_queue.clear();
    state.playback_queue.clear();
    state.video_config = None;
    state.audio_config = None;
    state.peer_proof_bridge.clear();
    Ok(Some(ReceiverShutdown {
        network: state.network.take(),
        transition_failed,
    }))
}

fn finish_receiver_stop(state: &mut NativeReceiverState) {
    // A listener callback may have completed after begin_stop released the
    // lock. It has now been joined; remove all leftovers before reopening.
    state.media_queue.clear();
    state.playback_queue.clear();
    state.video_config = None;
    state.audio_config = None;
    state.peer_proof_bridge.clear();
    state.stopping = false;
}

struct NetworkRuntime {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    discovery: Option<DiscoveryRuntime>,
    port: u16,
    pairing_code: u32,
    certificate_der: Vec<u8>,
}

/// Checks a verified FANP proof against the Rust-owned public trust registry.
///
/// The proof has already passed challenge/signature verification before this
/// helper is called. A matching stable device id and derived fingerprint are
/// required; malformed identities fail closed and remain on the explicit
/// approval path.
fn proof_matches_trusted_registry(registry: &TrustRegistry, proof: &PeerIdentityProof) -> bool {
    let Ok(device_id) = DeviceId::try_from(proof.device_id.clone()) else {
        return false;
    };
    let Ok(fingerprint) = DeviceFingerprint::from_public_key(&proof.public_key) else {
        return false;
    };
    registry.is_trusted(&device_id, &fingerprint)
}

struct DiscoveryRuntime {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl DiscoveryRuntime {
    fn start(port: u16, certificate: &[u8]) -> Option<Self> {
        let service = PublishedService::new(DISCOVERY_INSTANCE_NAME, DISCOVERY_HOST_NAME, port)
            .and_then(|service| service.with_property("proto", "fanp1"))
            .and_then(|service| service.with_property("ver", "0.1"))
            .and_then(|service| service.with_property("tls", "pin"))
            .and_then(|service| service.with_property("media", "h264,opus,aac"))
            .ok()?;
        // Larger certificates still support manually shared tickets. No TLS downgrade.
        let service =
            frameark_native::discovery::advertise_certificate(service.clone(), certificate)
                .unwrap_or(service);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let (ready_sender, ready_receiver) = std::sync::mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("frameark-fanp-discovery".to_string())
            .spawn(move || {
                let Ok(discovery) = MdnsDiscovery::new(DiscoveryConfig::all_interfaces()) else {
                    let _ = ready_sender.send(false);
                    return;
                };
                let Ok(registration) = discovery.publish(&service) else {
                    let _ = ready_sender.send(false);
                    let _ = discovery.shutdown();
                    return;
                };
                if ready_sender.send(true).is_err() {
                    unregister_discovery(registration, discovery);
                    return;
                }
                while !stop_thread.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(250));
                }
                unregister_discovery(registration, discovery);
            })
            .ok()?;
        if ready_receiver.recv().ok() != Some(true) {
            let _ = join.join();
            return None;
        }
        Some(Self {
            stop,
            join: Some(join),
        })
    }

    fn stop(mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn unregister_discovery(registration: PublishedRegistration, discovery: MdnsDiscovery) {
    let _ = registration.unregister();
    let _ = discovery.shutdown();
}

impl NetworkRuntime {
    #[cfg(test)]
    fn start_with_approval(
        device_id: DeviceId,
        approval_required: bool,
        pairing_approval: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        Self::start_with_approval_and_proof(
            device_id,
            approval_required,
            pairing_approval,
            PeerProofBridge::default(),
            false,
            Arc::new(Mutex::new(TrustRegistry::default())),
        )
    }

    fn start_with_approval_and_proof(
        device_id: DeviceId,
        approval_required: bool,
        pairing_approval: Arc<AtomicBool>,
        peer_proof_bridge: PeerProofBridge,
        peer_proof_required: bool,
        trusted_registry: Arc<Mutex<TrustRegistry>>,
    ) -> Result<Self, String> {
        let pairing_code = PairingCode::generate();
        let pairing_value = pairing_code
            .as_str()
            .parse()
            .map_err(|_| "FANP pairing code".to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let peer_proof_bridge_thread = peer_proof_bridge.clone();
        let trusted_registry_thread = Arc::clone(&trusted_registry);
        let (ready_sender, ready_receiver) = std::sync::mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("frameark-fanp-receiver".to_string())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    let _ = ready_sender.send(Err("FANP runtime".to_string()));
                    return;
                };
                let bind_result = {
                    let _entered = runtime.enter();
                    let bind_address = if cfg!(test) {
                        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
                    } else {
                        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)
                    };
                    PairingServer::bind(
                        bind_address,
                        if cfg!(test) {
                            "localhost"
                        } else {
                            NETWORK_SERVER_NAME
                        },
                        pairing_code,
                    )
                };
                let server = match bind_result {
                    Ok(server) => server,
                    Err(error) => {
                        let _ = ready_sender.send(Err(format!("FANP listener: {error:?}")));
                        return;
                    }
                };
                let Ok(port) = server.local_addr().map(|address| address.port()) else {
                    let _ = ready_sender.send(Err("FANP address".to_string()));
                    return;
                };
                let certificate_der = server.certificate_der().to_vec();
                if ready_sender.send(Ok((port, certificate_der))).is_err() {
                    return;
                }
                runtime.block_on(async move {
                    while !stop_thread.load(Ordering::Acquire) {
                        let pairing_approval = Arc::clone(&pairing_approval);
                        let trusted_registry_attempt = Arc::clone(&trusted_registry_thread);
                        let accepted = until_stopped(&stop_thread, async {
                            if peer_proof_required {
                                let bridge = peer_proof_bridge_thread.clone();
                                let trusted_registry = Arc::clone(&trusted_registry_attempt);
                                server
                                    .accept_pairing_with_peer_proof(
                                        NETWORK_OPERATION_TIMEOUT,
                                        move |proof| {
                                            let bridge = bridge.clone();
                                            let trusted_registry = Arc::clone(&trusted_registry);
                                            async move {
                                                if trusted_registry.lock().ok().is_some_and(
                                                    |registry| {
                                                        proof_matches_trusted_registry(
                                                            &registry, &proof,
                                                        )
                                                    },
                                                ) {
                                                    return true;
                                                }
                                                let Some(request_id) = bridge.enqueue(&proof)
                                                else {
                                                    return false;
                                                };
                                                bridge
                                                    .wait_for_decision(
                                                        request_id,
                                                        NETWORK_OPERATION_TIMEOUT,
                                                    )
                                                    .await
                                            }
                                        },
                                    )
                                    .await
                            } else {
                                server
                                    .accept_pairing_with_approval(
                                        NETWORK_OPERATION_TIMEOUT,
                                        move |_| async move {
                                            !approval_required
                                                || pairing_approval.swap(false, Ordering::AcqRel)
                                        },
                                    )
                                    .await
                            }
                        })
                        .await;
                        let transport = match accepted {
                            Some(Ok(transport)) => transport,
                            Some(Err(_)) => continue,
                            None => break,
                        };
                        let receiver = match NativeMediaControlReceiver::new(
                            transport,
                            network_session(device_id.clone()),
                            network_policy(),
                            NetworkBackend,
                            NetworkRendererFactory,
                        ) {
                            Ok(receiver) => receiver,
                            Err(_) => continue,
                        };
                        let outcome =
                            until_stopped(&stop_thread, receiver.serve(NETWORK_OPERATION_TIMEOUT))
                                .await;
                        #[cfg(feature = "android-playback-probe")]
                        if let Some(Err(error)) = &outcome {
                            playback_probe::record_failure(error.code());
                        }
                        if outcome.is_none() {
                            break;
                        }
                    }
                    peer_proof_bridge_thread.clear();
                    server.close();
                });
            })
            .map_err(|_| "FANP worker".to_string())?;
        let (port, certificate_der) = ready_receiver
            .recv()
            .map_err(|_| "FANP startup".to_string())??;
        let discovery = DiscoveryRuntime::start(port, &certificate_der);
        Ok(Self {
            stop,
            join: Some(join),
            discovery,
            port,
            pairing_code: pairing_value,
            certificate_der,
        })
    }

    fn stop(mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(discovery) = self.discovery.take() {
            discovery.stop();
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Poll stop without restarting an in-progress handshake or media future.
async fn until_stopped<F: Future>(stop: &AtomicBool, work: F) -> Option<F::Output> {
    tokio::select! {
        biased;
        () = async {
            while !stop.load(Ordering::Acquire) {
                tokio::time::sleep(PROOF_DECISION_POLL_INTERVAL).await;
            }
        } => None,
        output = work => Some(output),
    }
}

fn network_session(device_id: DeviceId) -> Session {
    Session::new(
        SessionId::try_from("android-network-session").expect("static session id"),
        device_id,
    )
}

const fn network_policy() -> ReceiverPolicy {
    ReceiverPolicy {
        video: true,
        opus: true,
        aac: true,
        max_width: 3840,
        max_height: 2160,
        max_fps: 60,
        max_channels: 2,
    }
}

struct NetworkBackend;

impl SessionBackend for NetworkBackend {
    fn supports_video_reconfiguration(&self) -> bool {
        true
    }

    fn reconfigure_video(&mut self, video: VideoConfig) -> frameark_core::Result<()> {
        // The renderer enqueues the ordered video config; no audio/clock reset.
        video.validate()
    }

    fn prepare(&mut self, _offer: SessionOffer) -> frameark_core::Result<()> {
        reset_playback(true);
        Ok(())
    }

    fn start(&mut self) -> frameark_core::Result<()> {
        Ok(())
    }

    fn reset(&mut self) -> frameark_core::Result<()> {
        reset_playback(false);
        Ok(())
    }
}

struct NetworkVideoRenderer;

impl VideoRenderer for NetworkVideoRenderer {
    type Error = ();

    fn configure(&mut self, config: VideoConfig) -> std::result::Result<(), Self::Error> {
        set_video_config(config);
        enqueue_playback(1, &encode_video_config(config))
    }

    fn render(&mut self, frame: VideoFrame<'_>) -> std::result::Result<(), Self::Error> {
        enqueue_playback(
            3,
            &encode_media_frame(&NativeMediaFrame {
                kind: MEDIA_KIND_VIDEO,
                pts: frame
                    .time_base
                    .to_microseconds(frame.timestamp)
                    .map_err(|_| ())?,
                keyframe: frame.keyframe,
                payload: frame.data.to_vec(),
            }),
        )
    }

    fn reset(&mut self) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}

struct NetworkAudioRenderer;

impl AudioRenderer for NetworkAudioRenderer {
    type Error = ();

    fn configure(&mut self, config: AudioConfig) -> std::result::Result<(), Self::Error> {
        set_audio_config(config);
        enqueue_playback(2, &encode_audio_config(config))
    }

    fn render(&mut self, frame: AudioFrame<'_>) -> std::result::Result<(), Self::Error> {
        enqueue_playback(
            4,
            &encode_media_frame(&NativeMediaFrame {
                kind: MEDIA_KIND_AUDIO,
                pts: frame
                    .time_base
                    .to_microseconds(frame.timestamp)
                    .map_err(|_| ())?,
                keyframe: false,
                payload: frame.data.to_vec(),
            }),
        )
    }

    fn reset(&mut self) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}

struct NetworkRendererFactory;

impl MediaRendererFactory for NetworkRendererFactory {
    type Video = NetworkVideoRenderer;
    type Audio = NetworkAudioRenderer;

    fn prepare(
        &mut self,
        offer: SessionOffer,
    ) -> frameark_core::Result<MediaSession<Self::Video, Self::Audio>> {
        MediaSession::prepare(
            offer,
            offer.video.map(|_| {
                (
                    frameark_core::TrackId::new("video-0").expect("static track id"),
                    NetworkVideoRenderer,
                )
            }),
            offer.audio.map(|_| {
                (
                    frameark_core::TrackId::new("audio-0").expect("static track id"),
                    NetworkAudioRenderer,
                )
            }),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeMediaFrame {
    kind: u8,
    pts: i64,
    keyframe: bool,
    payload: Vec<u8>,
}

/// Encoded FANP sink that feeds the process-local JNI polling queue.
///
/// A native network/session coordinator can pass this sink to
/// `frameark_native::NativeEncodedMediaReceiver`; Android then polls the same
/// queue and submits the returned envelopes to `MediaCodec`/`AudioTrack`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JniMediaQueueSink;

impl EncodedMediaSink for JniMediaQueueSink {
    type Error = ();

    fn push_video(&mut self, frame: VideoFrame<'_>) -> std::result::Result<(), Self::Error> {
        if submit_media_frame(NativeMediaFrame {
            kind: MEDIA_KIND_VIDEO,
            pts: frame.timestamp,
            keyframe: frame.keyframe,
            payload: frame.data.to_vec(),
        }) == 0
        {
            Ok(())
        } else {
            Err(())
        }
    }

    fn push_audio(&mut self, frame: AudioFrame<'_>) -> std::result::Result<(), Self::Error> {
        if submit_media_frame(NativeMediaFrame {
            kind: MEDIA_KIND_AUDIO,
            pts: frame.timestamp,
            keyframe: false,
            payload: frame.data.to_vec(),
        }) == 0
        {
            Ok(())
        } else {
            Err(())
        }
    }
}

static RECEIVER_STATE: OnceLock<Mutex<NativeReceiverState>> = OnceLock::new();

fn receiver_state() -> &'static Mutex<NativeReceiverState> {
    RECEIVER_STATE.get_or_init(|| Mutex::new(NativeReceiverState::default()))
}

/// Starts the Rust-owned receiver lifecycle and returns 0 on success.
///
/// A return value of 1 means the receiver is already started; negative values
/// indicate an internal failure (-1), shutdown still in progress (-2),
/// network startup failure (-4), or missing
/// platform identity (-5). An identity is required before any sockets open.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeStartReceiver(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jint {
    let Ok(mut state) = receiver_state().lock() else {
        return -1;
    };
    if state.session.is_some() {
        return 1;
    }
    if dlna_runtime::is_running() {
        return -6;
    }
    let session = match prepare_receiver_session(&state) {
        Ok(session) => session,
        Err(code) => return code,
    };
    state.peer_proof_bridge.clear();
    let Ok(network) = NetworkRuntime::start_with_approval_and_proof(
        session.device_id().clone(),
        state.pairing_approval_required,
        Arc::clone(&state.pairing_approval),
        state.peer_proof_bridge.clone(),
        state.peer_proof_required,
        Arc::new(Mutex::new(state.trust_registry.clone())),
    ) else {
        return -4;
    };
    state.media_queue.clear();
    state.playback_queue.clear();
    state.video_config = None;
    state.audio_config = None;
    state.session = Some(session);
    state.network = Some(network);
    0
}

/// Stops the Rust-owned receiver lifecycle and returns 0 after the listener
/// has joined. Returns -2 if another caller is already stopping it.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeStopReceiver(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jint {
    let Ok(mut state) = receiver_state().lock() else {
        return -1;
    };
    let shutdown = match begin_receiver_stop(&mut state) {
        Ok(Some(shutdown)) => shutdown,
        Ok(None) => return 0,
        Err(code) => return code,
    };
    drop(state);
    if let Some(network) = shutdown.network {
        network.stop();
    }
    let Ok(mut state) = receiver_state().lock() else {
        return -1;
    };
    finish_receiver_stop(&mut state);
    if shutdown.transition_failed {
        return -1;
    }
    0
}

/// Returns 1 while the Rust receiver lifecycle is active or stopping, otherwise 0.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeReceiverRunning(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jint {
    let Ok(state) = receiver_state().lock() else {
        return -1;
    };
    i32::from(state.is_active())
}

/// Returns the ephemeral FANP listener port, or zero when the receiver is stopped.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeReceiverPort(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jint {
    let Ok(state) = receiver_state().lock() else {
        return -1;
    };
    state
        .network
        .as_ref()
        .map_or(0, |network| i32::from(network.port))
}

/// Returns the temporary six-digit pairing code, or zero when stopped.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeReceiverPairingCode(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jint {
    let Ok(state) = receiver_state().lock() else {
        return -1;
    };
    state
        .network
        .as_ref()
        .map_or(0, |network| network.pairing_code as jint)
}

/// Returns the pinned ephemeral server certificate, or null when stopped.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeReceiverCertificate(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jbyteArray {
    let Ok(state) = receiver_state().lock() else {
        return JObject::null().into_raw() as jbyteArray;
    };
    let Some(network) = state.network.as_ref() else {
        return JObject::null().into_raw() as jbyteArray;
    };
    match env.byte_array_from_slice(&network.certificate_der) {
        Ok(array) => array.into_raw(),
        Err(_) => JObject::null().into_raw() as jbyteArray,
    }
}

/// Queues one encoded video access unit for the Android renderer boundary.
///
/// A return value of 0 means accepted, 1 means bounded backpressure, and
/// negative values mean that the receiver is stopped or the frame is invalid.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeSubmitVideoFrame(
    mut env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    payload: JByteArray<'_>,
    pts: jlong,
    keyframe: jboolean,
) -> jint {
    submit_java_frame(&mut env, payload, MEDIA_KIND_VIDEO, pts, keyframe != 0)
}

/// Queues one encoded audio access unit for the Android renderer boundary.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeSubmitAudioFrame(
    mut env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    payload: JByteArray<'_>,
    pts: jlong,
) -> jint {
    submit_java_frame(&mut env, payload, MEDIA_KIND_AUDIO, pts, false)
}

/// Takes the oldest queued encoded frame as a bounded `FAMF` envelope.
///
/// The envelope is `FAMF`, version, kind, keyframe flag, signed-big-endian
/// PTS, u32 payload length, and payload bytes. A null Java byte array means the
/// queue is empty.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativePollMediaFrame(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jbyteArray {
    let Ok(mut state) = receiver_state().lock() else {
        return JObject::null().into_raw() as jbyteArray;
    };
    let Some(frame) = state.media_queue.pop_front() else {
        return JObject::null().into_raw() as jbyteArray;
    };
    let encoded = encode_media_frame(&frame);
    match env.byte_array_from_slice(&encoded) {
        Ok(array) => array.into_raw(),
        Err(_) => JObject::null().into_raw() as jbyteArray,
    }
}

/// Takes the newest negotiated video configuration as a bounded `FAMC` envelope.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativePollVideoConfig(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jbyteArray {
    let Some(encoded) = take_video_config() else {
        return JObject::null().into_raw() as jbyteArray;
    };
    match env.byte_array_from_slice(&encoded) {
        Ok(array) => array.into_raw(),
        Err(_) => JObject::null().into_raw() as jbyteArray,
    }
}

/// Takes the newest negotiated audio configuration as a bounded `FAMC` envelope.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativePollAudioConfig(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jbyteArray {
    let Some(encoded) = take_audio_config() else {
        return JObject::null().into_raw() as jbyteArray;
    };
    match env.byte_array_from_slice(&encoded) {
        Ok(array) => array.into_raw(),
        Err(_) => JObject::null().into_raw() as jbyteArray,
    }
}

fn submit_java_frame(
    env: &mut JNIEnv<'_>,
    payload: JByteArray<'_>,
    kind: u8,
    pts: jlong,
    keyframe: bool,
) -> jint {
    let Ok(payload) = env.convert_byte_array(payload) else {
        return -3;
    };
    submit_media_frame(NativeMediaFrame {
        kind,
        pts,
        keyframe,
        payload,
    })
}

fn submit_media_frame(frame: NativeMediaFrame) -> jint {
    let Ok(mut state) = receiver_state().lock() else {
        return -1;
    };
    enqueue_media_frame(&mut state, frame)
}

fn enqueue_media_frame(state: &mut NativeReceiverState, frame: NativeMediaFrame) -> jint {
    if frame.kind != MEDIA_KIND_VIDEO && frame.kind != MEDIA_KIND_AUDIO {
        return -3;
    }
    if frame.payload.is_empty() || frame.payload.len() > MAX_JNI_MEDIA_FRAME_BYTES {
        return -3;
    }
    if state.session.is_none() {
        return -2;
    }
    if state.media_queue.len() >= MAX_JNI_MEDIA_QUEUE_FRAMES {
        return 1;
    }
    state.media_queue.push_back(frame);
    0
}

fn encode_media_frame(frame: &NativeMediaFrame) -> Vec<u8> {
    let mut output = Vec::with_capacity(19 + frame.payload.len());
    output.extend_from_slice(MEDIA_FRAME_MAGIC);
    output.push(MEDIA_FRAME_VERSION);
    output.push(frame.kind);
    output.push(u8::from(frame.keyframe));
    output.extend_from_slice(&frame.pts.to_be_bytes());
    output.extend_from_slice(&(frame.payload.len() as u32).to_be_bytes());
    output.extend_from_slice(&frame.payload);
    output
}

fn codec_wire_value(codec: MediaCodec) -> u8 {
    match codec {
        MediaCodec::H264 => 1,
        MediaCodec::H265 => 2,
        MediaCodec::Aac => 3,
        MediaCodec::Opus => 4,
        MediaCodec::Pcm => 5,
        MediaCodec::Alac => 6,
        MediaCodec::Unknown => 255,
    }
}

fn encode_video_config(config: VideoConfig) -> Vec<u8> {
    let mut output = Vec::with_capacity(24);
    output.extend_from_slice(MEDIA_CONFIG_MAGIC);
    output.extend_from_slice(&[
        MEDIA_CONFIG_VERSION,
        MEDIA_KIND_VIDEO,
        codec_wire_value(config.codec),
        0,
    ]);
    output.extend_from_slice(&config.width.to_be_bytes());
    output.extend_from_slice(&config.height.to_be_bytes());
    output.extend_from_slice(&config.frame_rate_numerator.to_be_bytes());
    output.extend_from_slice(&config.frame_rate_denominator.to_be_bytes());
    output
}

fn encode_audio_config(config: AudioConfig) -> Vec<u8> {
    let mut output = Vec::with_capacity(16);
    output.extend_from_slice(MEDIA_CONFIG_MAGIC);
    output.extend_from_slice(&[
        MEDIA_CONFIG_VERSION,
        MEDIA_KIND_AUDIO,
        codec_wire_value(config.codec),
        0,
    ]);
    output.extend_from_slice(&config.sample_rate.to_be_bytes());
    output.extend_from_slice(&u32::from(config.channels).to_be_bytes());
    output
}

fn set_video_config(config: VideoConfig) {
    if let Ok(mut state) = receiver_state().lock() {
        state.video_config = Some(config);
    }
}

fn set_audio_config(config: AudioConfig) {
    if let Ok(mut state) = receiver_state().lock() {
        state.audio_config = Some(config);
    }
}

fn take_video_config() -> Option<Vec<u8>> {
    receiver_state()
        .lock()
        .ok()
        .and_then(|mut state| state.video_config.take().map(encode_video_config))
}

fn take_audio_config() -> Option<Vec<u8>> {
    receiver_state()
        .lock()
        .ok()
        .and_then(|mut state| state.audio_config.take().map(encode_audio_config))
}

#[cfg(test)]
mod tests {
    use frameark_core::{
        AudioConfig, CORE_ABI_VERSION, DeviceId, MediaCodec, Session, SessionId, SessionState,
        TimeBase, TrackId, VideoConfig,
    };
    use frameark_media::{AudioPacket, MediaPacket, MediaTimestamp, VideoFrame};
    use frameark_native::{NativeSender, media_wire};
    use frameark_transport::{PairingClient, PairingCode, PeerIdentity};
    use ring::signature::KeyPair;

    use super::*;

    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[test]
    fn ffi_uses_the_core_abi_version() {
        let _guard = test_guard();
        assert_eq!(CORE_ABI_VERSION, 1);
    }

    #[test]
    fn identity_fingerprint_validation_is_strict_and_lowercase() {
        let valid = "ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89";
        assert!(valid_identity_fingerprint(valid));
        assert!(!valid_identity_fingerprint(&valid.to_uppercase()));
        assert!(!valid_identity_fingerprint("00:11"));
        assert!(!valid_identity_fingerprint(
            "ab-cd-ef-01-23-45-67-89-ab-cd-ef-01-23-45-67-89-ab-cd-ef-01-23-45-67-89-ab-cd-ef-01-23-45-67-89"
        ));
    }

    #[test]
    fn peer_proof_bridge_bounds_events_and_keeps_public_key_only() {
        let bridge = PeerProofBridge::default();
        let proof = PeerIdentityProof {
            device_id: "sender-proof".to_string(),
            public_key: vec![7_u8; 32],
            signature: vec![9_u8; 64],
        };
        let request_id = bridge.enqueue(&proof).expect("proof request queued");
        let event = bridge.pop().expect("proof event available");
        assert_eq!(event.request_id, request_id);
        assert_eq!(event.device_id, proof.device_id);
        assert_eq!(event.public_key, proof.public_key);
        assert_eq!(
            encode_peer_proof_event(&event).len(),
            14 + proof.device_id.len() + 32
        );
        assert!(!bridge.decide(request_id + 1, true));
        assert!(bridge.decide(request_id, false));
        bridge.clear();

        for index in 0..MAX_JNI_PROOF_EVENTS {
            let proof = PeerIdentityProof {
                device_id: format!("sender-{index}"),
                public_key: vec![index as u8; 32],
                signature: vec![0_u8; 64],
            };
            assert!(bridge.enqueue(&proof).is_some());
        }
        let overflow = PeerIdentityProof {
            device_id: "sender-overflow".to_string(),
            public_key: vec![1_u8; 32],
            signature: vec![0_u8; 64],
        };
        assert!(bridge.enqueue(&overflow).is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn peer_proof_bridge_waits_for_android_decision() {
        let bridge = PeerProofBridge::default();
        let proof = PeerIdentityProof {
            device_id: "sender-approval".to_string(),
            public_key: vec![3_u8; 32],
            signature: vec![0_u8; 64],
        };
        let request_id = bridge.enqueue(&proof).expect("proof request queued");
        let waiter = {
            let bridge = bridge.clone();
            tokio::spawn(async move {
                bridge
                    .wait_for_decision(request_id, Duration::from_secs(1))
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(bridge.decide(request_id, true));
        assert!(waiter.await.expect("proof waiter"));
    }

    #[tokio::test]
    async fn proof_decisions_are_single_use_expiring_and_cancellation_safe() {
        let bridge = PeerProofBridge::default();
        let proof = PeerIdentityProof {
            device_id: "sender-lifecycle".to_string(),
            public_key: vec![9; 32],
            signature: vec![0; 64],
        };
        let id = bridge.enqueue(&proof).unwrap();
        assert!(bridge.is_pending(id));
        assert!(bridge.decide(id, false));
        assert!(!bridge.decide(id, true));
        assert!(!bridge.is_pending(id));
        assert!(bridge.pop().is_none());
        assert!(!bridge.wait_for_decision(id, Duration::from_secs(1)).await);
        assert!(bridge.state.lock().unwrap().pending.is_empty());

        let expired = bridge
            .enqueue_with_timeout(&proof, Duration::from_millis(10))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(!bridge.decide(expired, true));
        assert!(!bridge.is_pending(expired));
        assert!(bridge.pop().is_none());

        let cancelled = bridge.enqueue(&proof).unwrap();
        assert!(
            tokio::time::timeout(
                Duration::from_millis(20),
                bridge.wait_for_decision(cancelled, Duration::from_secs(1))
            )
            .await
            .is_err()
        );
        assert!(!bridge.is_pending(cancelled));
        assert!(!bridge.decide(cancelled, true));
        assert!(bridge.pop().is_none());
        let stopped = bridge.enqueue(&proof).unwrap();
        bridge.clear();
        assert!(
            !bridge
                .wait_for_decision(stopped, Duration::from_secs(1))
                .await
        );
        let restarted = bridge.enqueue(&proof).unwrap();
        assert!(restarted > stopped);
        assert!(!bridge.decide(stopped, true));
        assert!(bridge.is_pending(restarted));
    }

    #[tokio::test]
    async fn stop_cancels_pending_work_and_removes_proof_events() {
        let bridge = PeerProofBridge::default();
        let proof = PeerIdentityProof {
            device_id: "sender-stop".to_string(),
            public_key: vec![7; 32],
            signature: vec![0; 64],
        };
        let id = bridge.enqueue(&proof).unwrap();
        let stop = AtomicBool::new(false);
        let (result, ()) = tokio::join!(
            until_stopped(
                &stop,
                bridge.wait_for_decision(id, NETWORK_OPERATION_TIMEOUT)
            ),
            async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                stop.store(true, Ordering::Release);
            }
        );
        assert_eq!(result, None);
        assert!(bridge.pop().is_none());
        assert!(!bridge.is_pending(id));
        assert!(!bridge.decide(id, true));
    }

    #[test]
    fn proof_enabled_network_runtime_surfaces_verified_event_before_acceptance() {
        let _guard = test_guard();
        let bridge = PeerProofBridge::default();
        let network = NetworkRuntime::start_with_approval_and_proof(
            DeviceId::try_from("proof-receiver").expect("valid receiver identity"),
            false,
            Arc::new(AtomicBool::new(false)),
            bridge.clone(),
            true,
            Arc::new(Mutex::new(TrustRegistry::default())),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), network.port);
        let pairing = PairingCode::parse(format!("{:06}", network.pairing_code))
            .expect("generated pairing code is six digits");
        let certificate = network.certificate_der.clone();
        let key_pair =
            ring::signature::Ed25519KeyPair::from_seed_unchecked(&[11_u8; 32]).expect("test key");
        let public_key: [u8; 32] = key_pair
            .public_key()
            .as_ref()
            .try_into()
            .expect("ed25519 public key");
        std::thread::sleep(Duration::from_millis(50));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("proof runtime");
        runtime.block_on(async {
            let client_task = tokio::spawn(async move {
                PairingClient::connect_with_identity(
                    address,
                    "localhost",
                    &certificate,
                    pairing,
                    PeerIdentity {
                        device_id: "proof-sender".to_string(),
                        public_key,
                    },
                    move |transcript| {
                        let signature = key_pair.sign(transcript);
                        let mut output = [0_u8; 64];
                        output.copy_from_slice(signature.as_ref());
                        Ok(output)
                    },
                    Duration::from_secs(5),
                )
                .await
            });
            let event = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if let Some(event) = bridge.pop() {
                        break event;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("proof event timeout");
            assert_eq!(event.device_id, "proof-sender");
            assert_eq!(event.public_key, public_key);
            // A human decision must survive multiple UI polling intervals.
            tokio::time::sleep(Duration::from_millis(750)).await;
            assert!(bridge.decide(event.request_id, true));
            let client = client_task
                .await
                .expect("proof client task")
                .expect("proof accepted");
            client.close();
        });
        let stopping = Instant::now();
        network.stop();
        assert!(stopping.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn proof_policy_auto_accepts_only_an_exact_pretrusted_identity() {
        let mut registry = TrustRegistry::new(2).expect("bounded registry");
        let key = [17_u8; 32];
        let device_id = DeviceId::try_from("trusted-sender").expect("valid device id");
        registry
            .trust_public_key(device_id.clone(), &key, "Trusted sender")
            .expect("trust key");
        let trusted = PeerIdentityProof {
            device_id: device_id.to_string(),
            public_key: key.to_vec(),
            signature: vec![0_u8; 64],
        };
        assert!(proof_matches_trusted_registry(&registry, &trusted));

        let rotated = PeerIdentityProof {
            public_key: vec![18_u8; 32],
            ..trusted.clone()
        };
        assert!(!proof_matches_trusted_registry(&registry, &rotated));
        let renamed = PeerIdentityProof {
            device_id: "other-sender".to_string(),
            ..trusted
        };
        assert!(!proof_matches_trusted_registry(&registry, &renamed));
    }

    #[test]
    fn proof_enabled_network_runtime_auto_accepts_pretrusted_peer() {
        let _guard = test_guard();
        let bridge = PeerProofBridge::default();
        let key_pair =
            ring::signature::Ed25519KeyPair::from_seed_unchecked(&[19_u8; 32]).expect("test key");
        let public_key: [u8; 32] = key_pair
            .public_key()
            .as_ref()
            .try_into()
            .expect("ed25519 public key");
        let mut registry = TrustRegistry::new(4).expect("bounded registry");
        registry
            .trust_public_key(
                DeviceId::try_from("trusted-sender").expect("valid device id"),
                &public_key,
                "Trusted sender",
            )
            .expect("pre-enroll trusted sender");
        let network = NetworkRuntime::start_with_approval_and_proof(
            DeviceId::try_from("proof-receiver").expect("valid receiver identity"),
            false,
            Arc::new(AtomicBool::new(false)),
            bridge.clone(),
            true,
            Arc::new(Mutex::new(registry)),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), network.port);
        let pairing = PairingCode::parse(format!("{:06}", network.pairing_code))
            .expect("generated pairing code is six digits");
        let certificate = network.certificate_der.clone();
        std::thread::sleep(Duration::from_millis(50));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("proof runtime");
        runtime.block_on(async {
            let client_task = tokio::spawn(async move {
                PairingClient::connect_with_identity(
                    address,
                    "localhost",
                    &certificate,
                    pairing,
                    PeerIdentity {
                        device_id: "trusted-sender".to_string(),
                        public_key,
                    },
                    move |transcript| {
                        let signature = key_pair.sign(transcript);
                        let mut output = [0_u8; 64];
                        output.copy_from_slice(signature.as_ref());
                        Ok(output)
                    },
                    Duration::from_secs(5),
                )
                .await
            });
            let client = tokio::time::timeout(Duration::from_secs(5), client_task)
                .await
                .expect("trusted proof client timeout")
                .expect("trusted proof client task")
                .expect("trusted proof should be accepted");
            assert!(bridge.pop().is_none());
            client.close();
        });
        network.stop();
    }

    #[test]
    fn identity_is_required_and_key_derived_in_both_session_paths() {
        let mut state = NativeReceiverState::default();
        assert_eq!(prepare_receiver_session(&state).unwrap_err(), -5);
        let fingerprint = vec!["ab"; 32].join(":");
        assert_eq!(
            install_device_identity(&mut state, "frameark-device", &fingerprint),
            0
        );
        let session = prepare_receiver_session(&state).unwrap();
        assert_eq!(
            session.device_id().as_str(),
            format!("sha256-{}", "ab".repeat(32))
        );
        assert_eq!(
            network_session(session.device_id().clone()).device_id(),
            session.device_id()
        );
        assert_eq!(session.state(), SessionState::Connecting);
    }

    #[test]
    fn active_identity_is_idempotent_but_cannot_be_replaced() {
        let mut state = NativeReceiverState::default();
        let first = vec!["ab"; 32].join(":");
        let second = vec!["cd"; 32].join(":");
        assert_eq!(
            install_device_identity(&mut state, "frameark-device", &first),
            0
        );
        let first_id = state.device_id.clone();
        state.session = Some(prepare_receiver_session(&state).unwrap());
        assert_eq!(
            install_device_identity(&mut state, "frameark-device", &first),
            0
        );
        assert_eq!(
            install_device_identity(&mut state, "frameark-device", &second),
            -2
        );
        assert_eq!(state.device_id, first_id);
        assert_eq!(state.identity_fingerprint.as_deref(), Some(first.as_str()));
        state.session = None;
        assert_eq!(
            install_device_identity(&mut state, "frameark-device", &second),
            0
        );
        assert_ne!(state.device_id, first_id);
    }

    #[test]
    fn malformed_identity_does_not_mutate_installed_identity() {
        let mut state = NativeReceiverState::default();
        let fingerprint = vec!["ab"; 32].join(":");
        assert_eq!(
            install_device_identity(&mut state, "frameark-device", &fingerprint),
            0
        );
        let first_id = state.device_id.clone();
        for alias in [
            "",
            "INVALID",
            "é",
            "bad alias",
            "bad\nlabel",
            &"a".repeat(65),
        ] {
            assert_eq!(install_device_identity(&mut state, alias, &fingerprint), -3);
        }
        assert_eq!(
            install_device_identity(&mut state, "frameark-device", "bad"),
            -3
        );
        assert_eq!(state.device_id, first_id);
        assert_eq!(
            state.identity_fingerprint.as_deref(),
            Some(fingerprint.as_str())
        );
    }

    #[test]
    fn running_receiver_rejects_trust_mutation_before_touching_the_registry() {
        let mut state = NativeReceiverState::default();
        let device_id = DeviceId::try_from("sender-running").unwrap();
        let key = [4; 32];
        assert_eq!(
            mutate_trust_registry(&mut state, |registry| {
                registry
                    .trust_public_key(device_id.clone(), &key, "Sender")
                    .map(|_| ())
            }),
            0
        );
        let snapshot = export_trust_snapshot(&state).unwrap();
        state.session = Some(network_session(DeviceId::try_from("receiver").unwrap()));
        assert_eq!(
            mutate_trust_registry(&mut state, |registry| {
                registry.revoke(&device_id).map(|_| ())
            }),
            -5
        );
        assert_eq!(export_trust_snapshot(&state).unwrap(), snapshot);
    }

    #[test]
    fn stopping_keeps_trust_and_restart_closed_until_listener_join_finishes() {
        let _guard = test_guard();
        let mut state = NativeReceiverState::default();
        let first = vec!["ab"; 32].join(":");
        assert_eq!(
            install_device_identity(&mut state, "frameark-device", &first),
            0
        );
        let device_id = DeviceId::try_from("sender-shutdown").unwrap();
        assert_eq!(
            mutate_trust_registry(&mut state, |registry| {
                registry
                    .trust_public_key(device_id.clone(), &[8; 32], "Sender")
                    .map(|_| ())
            }),
            0
        );
        let snapshot = export_trust_snapshot(&state).unwrap();
        // A live listener alone must also close the mutation gate.
        state.network = Some(
            NetworkRuntime::start_with_approval(
                state.device_id.clone().unwrap(),
                false,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap(),
        );
        assert!(state.is_active());
        assert_eq!(
            mutate_trust_registry(&mut state, |_| panic!("must not mutate while listening")),
            -5
        );
        state.session = Some(prepare_receiver_session(&state).unwrap());
        let shutdown = begin_receiver_stop(&mut state).unwrap().unwrap();
        assert!(!shutdown.transition_failed);
        assert!(state.session.is_none() && state.network.is_none());
        assert!(state.is_active());
        assert_eq!(
            mutate_trust_registry(&mut state, |_| panic!("must not mutate during join")),
            -5
        );
        assert_eq!(import_trust_snapshot(&mut state, &snapshot), -2);
        assert_eq!(prepare_receiver_session(&state).unwrap_err(), -2);
        assert_eq!(
            install_device_identity(&mut state, "frameark-device", &vec!["cd"; 32].join(":")),
            -2
        );
        assert!(begin_receiver_stop(&mut state).is_err_and(|code| code == -2));
        shutdown.network.unwrap().stop();
        // Lock reacquisition is part of stop completion, not just taking the handle.
        assert_eq!(
            mutate_trust_registry(&mut state, |_| panic!("stop not yet published")),
            -5
        );
        state.audio_config = Some(AudioConfig {
            codec: MediaCodec::Opus,
            sample_rate: 48_000,
            channels: 2,
        });
        finish_receiver_stop(&mut state);
        assert!(!state.is_active());
        assert!(state.audio_config.is_none());
        assert_eq!(export_trust_snapshot(&state).unwrap(), snapshot);
        assert_eq!(
            mutate_trust_registry(&mut state, |registry| registry
                .revoke(&device_id)
                .map(|_| ())),
            0
        );
        assert!(prepare_receiver_session(&state).is_ok());
        assert!(begin_receiver_stop(&mut state).unwrap().is_none());
    }

    #[test]
    fn trust_registry_bridge_keeps_key_rotation_explicit_and_revocation_bounded() {
        let mut state = NativeReceiverState::default();
        let device_id = DeviceId::try_from("sender-1").unwrap();
        let fingerprint = DeviceFingerprint::new(
            "ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89",
        )
        .unwrap();
        assert!(
            state
                .trust_registry
                .trust(device_id.clone(), fingerprint.clone(), "Sender")
                .is_ok()
        );
        assert!(state.trust_registry.is_trusted(&device_id, &fingerprint));
        let rotated = DeviceFingerprint::new(
            "cd:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89",
        )
        .unwrap();
        assert_eq!(
            state
                .trust_registry
                .trust(device_id.clone(), rotated, "Rotated"),
            Err(TrustError::IdentityChanged)
        );
        assert_eq!(
            state.trust_registry.revoke(&device_id).unwrap().label,
            "Sender"
        );
        assert!(!state.trust_registry.is_trusted(&device_id, &fingerprint));
        assert_eq!(
            state.trust_registry.revoke(&device_id),
            Err(TrustError::UnknownDevice)
        );
    }

    #[test]
    fn trust_snapshot_bridge_round_trips_before_start_and_rejects_active_replace() {
        let mut source = NativeReceiverState::default();
        let device_id = DeviceId::try_from("sender-1").unwrap();
        let fingerprint = DeviceFingerprint::new(
            "ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89",
        )
        .unwrap();
        source
            .trust_registry
            .trust(device_id.clone(), fingerprint.clone(), "Sender")
            .unwrap();
        let snapshot = export_trust_snapshot(&source).unwrap();
        let mut restored = NativeReceiverState::default();
        assert_eq!(import_trust_snapshot(&mut restored, &snapshot), 0);
        assert!(restored.trust_registry.is_trusted(&device_id, &fingerprint));
        restored.session = Some(Session::new(
            SessionId::try_from("test-session").unwrap(),
            DeviceId::try_from("receiver").unwrap(),
        ));
        assert_eq!(import_trust_snapshot(&mut restored, &snapshot), -2);
    }

    #[test]
    fn network_runtime_exposes_bounded_pairing_material_and_stops_cleanly() {
        let _guard = test_guard();
        let network = NetworkRuntime::start_with_approval(
            DeviceId::try_from("test-device").expect("valid test identity"),
            false,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_ne!(network.port, 0);
        assert!((0..=999_999).contains(&network.pairing_code));
        assert!(!network.certificate_der.is_empty());
        network.stop();
    }

    #[test]
    fn network_runtime_approval_gate_consumes_one_arm() {
        let _guard = test_guard();
        let approval = Arc::new(AtomicBool::new(false));
        let network = NetworkRuntime::start_with_approval(
            DeviceId::try_from("approval-device").expect("valid test identity"),
            true,
            Arc::clone(&approval),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), network.port);
        let pairing = PairingCode::parse(format!("{:06}", network.pairing_code))
            .expect("generated pairing code is six digits");
        let certificate = network.certificate_der.clone();
        std::thread::sleep(Duration::from_millis(50));
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("approval runtime")
            .block_on(async {
                let rejected = PairingClient::connect(
                    address,
                    "localhost",
                    &certificate,
                    pairing.clone(),
                    Duration::from_secs(5),
                )
                .await;
                assert!(matches!(
                    rejected,
                    Err(frameark_transport::TransportError::PairingRejected)
                ));
                approval.store(true, Ordering::Release);
                let accepted = PairingClient::connect(
                    address,
                    "localhost",
                    &certificate,
                    pairing,
                    Duration::from_secs(5),
                )
                .await
                .expect("armed pairing");
                accepted.close();
                assert!(!approval.load(Ordering::Acquire));
            });
        network.stop();
    }

    #[test]
    fn network_runtime_completes_loopback_pairing_and_media_delivery() {
        let _guard = test_guard();
        {
            let mut state = receiver_state().lock().unwrap();
            state.session = Some(Session::new(
                SessionId::try_from("loopback-receiver").unwrap(),
                DeviceId::try_from("loopback-device").unwrap(),
            ));
            state.media_queue.clear();
            state.network = None;
        }
        let network = NetworkRuntime::start_with_approval(
            DeviceId::try_from("loopback-device").expect("valid test identity"),
            false,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), network.port);
        let pairing = PairingCode::parse(format!("{:06}", network.pairing_code))
            .expect("generated pairing code is six digits");
        let certificate = network.certificate_der.clone();
        std::thread::sleep(Duration::from_millis(50));
        let report = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("loopback runtime")
            .block_on(async {
                let transport = PairingClient::connect(
                    address,
                    "localhost",
                    &certificate,
                    pairing,
                    Duration::from_secs(5),
                )
                .await
                .expect("loopback FANP pairing");
                let mut sender = NativeSender::new(
                    transport,
                    Session::new(
                        SessionId::try_from("loopback-session").unwrap(),
                        DeviceId::try_from("loopback-sender").unwrap(),
                    ),
                )
                .unwrap();
                sender
                    .offer(
                        SessionOffer {
                            video: Some(VideoConfig {
                                codec: MediaCodec::H264,
                                width: 1280,
                                height: 720,
                                frame_rate_numerator: 30,
                                frame_rate_denominator: 1,
                            }),
                            audio: Some(AudioConfig {
                                codec: MediaCodec::Opus,
                                sample_rate: 48_000,
                                channels: 2,
                            }),
                            latency_ms: 120,
                        },
                        Duration::from_secs(5),
                    )
                    .await
                    .unwrap();
                sender.start(Duration::from_secs(5)).await.unwrap();
                let time_base = TimeBase::new(1, 90_000).unwrap();
                let video_track = TrackId::new("video-0").unwrap();
                let audio_track = TrackId::new("audio-0").unwrap();
                let video = VideoFrame::new(
                    video_track,
                    1,
                    MediaTimestamp::new(0, time_base),
                    None,
                    true,
                    vec![0, 0, 0, 1, 0x65, 0x88],
                )
                .unwrap();
                let audio = AudioPacket::new(
                    audio_track,
                    1,
                    MediaTimestamp::new(0, TimeBase::new(1, 48_000).unwrap()),
                    960,
                    vec![0xF8, 0xFF, 0xFE],
                )
                .unwrap();
                let mut media = sender.open_media_stream().await.unwrap();
                media
                    .send_frame(
                        &media_wire::encode(&MediaPacket::Video(video)).unwrap(),
                        Duration::from_secs(5),
                    )
                    .await
                    .unwrap();
                media
                    .send_frame(
                        &media_wire::encode(&MediaPacket::Audio(audio)).unwrap(),
                        Duration::from_secs(5),
                    )
                    .await
                    .unwrap();
                media.finish().unwrap();
                sender.stop(Duration::from_secs(5)).await.unwrap()
            });
        assert_eq!(report.state, SessionState::Closed);
        network.stop();
        let mut state = receiver_state().lock().unwrap();
        assert_eq!(state.video_config.unwrap().width, 1280);
        assert_eq!(state.audio_config.unwrap().sample_rate, 48_000);
        let events: Vec<_> = std::iter::from_fn(|| state.playback_queue.pop()).collect();
        assert_eq!(
            events.iter().map(|event| event[5]).collect::<Vec<_>>(),
            [5, 1, 2, 3, 4, 5]
        );
        assert!(state.media_queue.is_empty());
        state.session = None;
        state.media_queue.clear();
        state.network = None;
    }

    #[test]
    fn network_runtime_repeats_short_loopback_sessions_without_stale_state() {
        let _guard = test_guard();

        for cycle in 0..3 {
            {
                let mut state = receiver_state().lock().unwrap();
                state.session = Some(Session::new(
                    SessionId::try_from(format!("loopback-receiver-{cycle}")).unwrap(),
                    DeviceId::try_from("loopback-device").unwrap(),
                ));
                state.media_queue.clear();
                state.video_config = None;
                state.audio_config = None;
                state.network = None;
            }

            let network = NetworkRuntime::start_with_approval(
                DeviceId::try_from("loopback-device").expect("valid test identity"),
                false,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap_or_else(|error| panic!("loopback cycle {cycle} startup: {error}"));
            let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), network.port);
            let pairing = PairingCode::parse(format!("{:06}", network.pairing_code))
                .expect("generated pairing code is six digits");
            let certificate = network.certificate_der.clone();
            std::thread::sleep(Duration::from_millis(50));

            let report = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("loopback runtime")
                .block_on(async {
                    let transport = PairingClient::connect(
                        address,
                        "localhost",
                        &certificate,
                        pairing,
                        Duration::from_secs(5),
                    )
                    .await
                    .expect("loopback FANP pairing");
                    let mut sender = NativeSender::new(
                        transport,
                        Session::new(
                            SessionId::try_from(format!("loopback-session-{cycle}")).unwrap(),
                            DeviceId::try_from("loopback-sender").unwrap(),
                        ),
                    )
                    .unwrap();
                    sender
                        .offer(
                            SessionOffer {
                                video: Some(VideoConfig {
                                    codec: MediaCodec::H264,
                                    width: 1280,
                                    height: 720,
                                    frame_rate_numerator: 30,
                                    frame_rate_denominator: 1,
                                }),
                                audio: Some(AudioConfig {
                                    codec: MediaCodec::Opus,
                                    sample_rate: 48_000,
                                    channels: 2,
                                }),
                                latency_ms: 120,
                            },
                            Duration::from_secs(5),
                        )
                        .await
                        .expect("loopback offer");
                    sender
                        .start(Duration::from_secs(5))
                        .await
                        .expect("loopback start");

                    let video = VideoFrame::new(
                        TrackId::new("video-0").unwrap(),
                        1,
                        MediaTimestamp::new(0, TimeBase::new(1, 90_000).unwrap()),
                        None,
                        true,
                        vec![0, 0, 0, 1, 0x65, cycle],
                    )
                    .unwrap();
                    let audio = AudioPacket::new(
                        TrackId::new("audio-0").unwrap(),
                        1,
                        MediaTimestamp::new(0, TimeBase::new(1, 48_000).unwrap()),
                        960,
                        vec![0xF8, 0xFF, cycle],
                    )
                    .unwrap();
                    let mut media = sender.open_media_stream().await.unwrap();
                    media
                        .send_frame(
                            &media_wire::encode(&MediaPacket::Video(video)).unwrap(),
                            Duration::from_secs(5),
                        )
                        .await
                        .expect("loopback video");
                    media
                        .send_frame(
                            &media_wire::encode(&MediaPacket::Audio(audio)).unwrap(),
                            Duration::from_secs(5),
                        )
                        .await
                        .expect("loopback audio");
                    media.finish().unwrap();
                    sender
                        .stop(Duration::from_secs(5))
                        .await
                        .expect("loopback stop")
                });

            assert_eq!(report.state, SessionState::Closed);
            network.stop();

            let mut state = receiver_state().lock().unwrap();
            assert_eq!(state.video_config.unwrap().width, 1280);
            assert_eq!(state.audio_config.unwrap().sample_rate, 48_000);
            let events: Vec<_> = std::iter::from_fn(|| state.playback_queue.pop()).collect();
            assert_eq!(
                events.iter().map(|event| event[5]).collect::<Vec<_>>(),
                [5, 1, 2, 3, 4, 5]
            );
            assert!(state.media_queue.is_empty());
            state.session = None;
            state.video_config = None;
            state.audio_config = None;
            state.media_queue.clear();
            state.network = None;
        }
    }

    #[test]
    fn lifecycle_state_is_bounded_and_idempotent() {
        let _guard = test_guard();
        let state = receiver_state();
        let mut state = state.lock().unwrap();
        state.session = None;
        let session_id = SessionId::try_from("test-session").unwrap();
        let device_id = DeviceId::try_from("test-device").unwrap();
        let mut session = Session::new(session_id, device_id);
        session.transition(SessionState::Connecting).unwrap();
        assert_eq!(session.state(), SessionState::Connecting);
        session.transition(SessionState::Closed).unwrap();
        state.session = None;
        state.media_queue.clear();
    }

    #[test]
    fn media_queue_applies_backpressure_and_preserves_envelope() {
        let _guard = test_guard();
        let mut state = NativeReceiverState {
            stopping: false,
            session: Some(Session::new(
                SessionId::try_from("media-session").unwrap(),
                DeviceId::try_from("media-device").unwrap(),
            )),
            media_queue: VecDeque::new(),
            playback_queue: PlaybackQueue::default(),
            video_config: None,
            audio_config: None,
            identity_alias: None,
            device_id: None,
            identity_fingerprint: None,
            trust_registry: TrustRegistry::default(),
            pairing_approval_required: false,
            pairing_approval: Arc::new(AtomicBool::new(false)),
            peer_proof_required: false,
            peer_proof_bridge: PeerProofBridge::default(),
            network: None,
        };
        assert_eq!(
            enqueue_media_frame(
                &mut state,
                NativeMediaFrame {
                    kind: MEDIA_KIND_VIDEO,
                    pts: 90_000,
                    keyframe: true,
                    payload: vec![1, 2, 3],
                }
            ),
            0
        );
        let frame = state.media_queue.pop_front().unwrap();
        assert_eq!(
            encode_media_frame(&frame),
            [
                b'F', b'A', b'M', b'F', 1, 1, 1, 0, 0, 0, 0, 0, 1, 95, 144, 0, 0, 0, 3, 1, 2, 3
            ]
        );
        for _ in 0..MAX_JNI_MEDIA_QUEUE_FRAMES {
            assert_eq!(
                enqueue_media_frame(
                    &mut state,
                    NativeMediaFrame {
                        kind: MEDIA_KIND_AUDIO,
                        pts: 0,
                        keyframe: false,
                        payload: vec![9],
                    }
                ),
                0
            );
        }
        assert_eq!(
            enqueue_media_frame(
                &mut state,
                NativeMediaFrame {
                    kind: MEDIA_KIND_AUDIO,
                    pts: 0,
                    keyframe: false,
                    payload: vec![9],
                }
            ),
            1
        );
        state.session = None;
        state.media_queue.clear();
    }

    #[test]
    fn ordered_playback_queue_paces_microseconds_and_resets_its_clock() {
        let mut queue = PlaybackQueue::default();
        let frame = |pts| {
            encode_media_frame(&NativeMediaFrame {
                kind: MEDIA_KIND_VIDEO,
                pts,
                keyframe: true,
                payload: vec![1],
            })
        };
        queue.push(3, &frame(2_000_000)).unwrap();
        queue.push(3, &frame(3_000_000)).unwrap();
        assert!(
            queue.push(3, &frame(i64::MAX)).is_err(),
            "far future cannot pin outputs indefinitely"
        );
        queue.reset(false);
        assert!(queue.pop().is_some());
        assert!(queue.pop().is_none(), "future frame must wait");
        assert_eq!(queue.events.len(), 2, "reset cannot overtake the frame");
        queue.clock = Some((2_000_000, Instant::now() - Duration::from_secs(2)));
        assert!(queue.pop().is_some());
        assert_eq!(queue.pop().unwrap(), b"FAPE\x01\x05");
        assert!(queue.clock.is_none());
        queue.push(3, &frame(50_000_000)).unwrap();
        assert!(queue.pop().is_some(), "new timeline starts immediately");
    }

    #[test]
    fn video_config_boundary_preserves_audio_and_both_playback_clocks() {
        let mut queue = PlaybackQueue::default();
        let audio = encode_media_frame(&NativeMediaFrame {
            kind: MEDIA_KIND_AUDIO,
            pts: 0,
            keyframe: false,
            payload: vec![1],
        });
        queue.push(4, &audio).unwrap();
        assert_eq!(queue.pop().unwrap()[5], 4);
        let clock = queue.clock;
        let arrival = queue.arrival_clock;
        queue.push(4, &audio).unwrap();
        let video = VideoConfig {
            codec: MediaCodec::H264,
            width: 720,
            height: 1280,
            frame_rate_numerator: 30,
            frame_rate_denominator: 1,
        };
        queue.push(1, &encode_video_config(video)).unwrap();
        queue.push(4, &audio).unwrap();
        for kind in [4, 1, 4] {
            assert_eq!(queue.pop().unwrap()[5], kind);
        }
        assert_eq!(queue.clock, clock);
        assert_eq!(queue.arrival_clock, arrival);
        assert_eq!(queue.bytes, 0);
    }

    #[test]
    fn network_renderers_convert_both_track_clocks_to_microseconds() {
        let _guard = test_guard();
        {
            let mut state = receiver_state().lock().unwrap();
            state.session = Some(network_session(DeviceId::try_from("clock-test").unwrap()));
            state.playback_queue.clear();
        }
        let track_id = TrackId::new("video-0").unwrap();
        NetworkVideoRenderer
            .render(frameark_api::VideoFrame {
                track_id: &track_id,
                data: &[1],
                timestamp: 90_000,
                time_base: TimeBase::new(1, 90_000).unwrap(),
                keyframe: true,
            })
            .unwrap();
        NetworkAudioRenderer
            .render(AudioFrame {
                track_id: &track_id,
                data: &[2],
                timestamp: 48_000,
                time_base: TimeBase::new(1, 48_000).unwrap(),
            })
            .unwrap();
        let mut state = receiver_state().lock().unwrap();
        for kind in [3, 4] {
            let event = state.playback_queue.pop().unwrap();
            assert_eq!(event[5], kind);
            assert_eq!(
                i64::from_be_bytes(event[13..21].try_into().unwrap()),
                1_000_000
            );
        }
        state.session = None;
        state.playback_queue.clear();
    }

    #[test]
    fn ordered_playback_queue_bounds_bytes_and_never_loses_reset() {
        let mut queue = PlaybackQueue::default();
        for _ in 0..MAX_PLAYBACK_EVENTS {
            queue.push(5, &[]).unwrap();
        }
        assert!(queue.push(5, &[]).is_err());
        queue.reset(false);
        assert_eq!(queue.pop().unwrap(), b"FAPE\x01\x05");
        assert!(queue.pop().is_none());
        assert_eq!(queue.bytes, 0);
        let payload = vec![0; MAX_JNI_MEDIA_FRAME_BYTES];
        for _ in 0..3 {
            queue.push(3, &payload).unwrap();
        }
        assert!(queue.push(3, &payload).is_err());
        queue.reset(true);
        assert_eq!(queue.events.len(), 1);
        assert_eq!(queue.bytes, 6);
    }

    #[test]
    fn jni_media_queue_sink_accepts_encoded_platform_samples() {
        let _guard = test_guard();
        let state = receiver_state();
        state.lock().unwrap().session = Some(Session::new(
            SessionId::try_from("sink-session").unwrap(),
            DeviceId::try_from("sink-device").unwrap(),
        ));
        let track_id = TrackId::new("video-0").unwrap();
        let mut sink = JniMediaQueueSink;
        sink.push_video(frameark_api::VideoFrame {
            track_id: &track_id,
            data: &[7, 8],
            timestamp: 90_000,
            time_base: TimeBase::new(1, 90_000).unwrap(),
            keyframe: true,
        })
        .unwrap();
        let mut state = state.lock().unwrap();
        let queued = state.media_queue.pop_front().unwrap();
        assert_eq!(queued.kind, MEDIA_KIND_VIDEO);
        assert_eq!(queued.pts, 90_000);
        assert_eq!(queued.payload, vec![7, 8]);
        state.session = None;
        state.media_queue.clear();
    }
}
