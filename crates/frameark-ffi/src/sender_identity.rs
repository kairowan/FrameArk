//! One request per worker; the OS signer runs on the Kotlin producer thread.
//! No JVM callback blocks the Rust executor, and no private key crosses JNI.
use super::*;
use frameark_transport::{PlatformPeerIdentity, TransportError};

// DER SubjectPublicKeyInfo: id-ecPublicKey + namedCurve prime256v1 + BIT STRING.
const SPKI_PREFIX: [u8; 26] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
];

fn parse_identity(spki: &[u8]) -> Option<PlatformPeerIdentity> {
    if spki.len() != 91 || !spki.starts_with(&SPKI_PREFIX) || spki[26] != 4 {
        return None;
    }
    let public_key = spki[26..].to_vec();
    let fingerprint = DeviceFingerprint::from_public_key(&public_key).ok()?;
    let hex = fingerprint.as_str().replace(':', "");
    Some(PlatformPeerIdentity {
        device_id: format!("sender-{}", &hex[..32]),
        public_key,
    })
}

fn jni_identity(env: &JNIEnv<'_>, spki: &JByteArray<'_>) -> Option<PlatformPeerIdentity> {
    if env.get_array_length(spki).ok()? != 91 {
        return None;
    }
    parse_identity(&env.convert_byte_array(spki).ok()?)
}

#[derive(Default)]
enum SignState {
    #[default]
    Idle,
    Pending {
        transcript: Option<Vec<u8>>,
        signature: Option<Vec<u8>>,
    },
    Closed,
}

#[derive(Default)]
pub(super) struct SigningBridge(Mutex<SignState>);

struct SignGuard<'a>(&'a SigningBridge);
impl Drop for SignGuard<'_> {
    fn drop(&mut self) {
        self.0.close();
    }
}

impl SigningBridge {
    pub(super) async fn sign(&self, transcript: Vec<u8>) -> Result<Vec<u8>, TransportError> {
        {
            let mut state = self
                .0
                .lock()
                .map_err(|_| TransportError::PeerAuthenticationRejected)?;
            if !matches!(*state, SignState::Idle) || transcript.is_empty() || transcript.len() > 256
            {
                return Err(TransportError::PeerAuthenticationRejected);
            }
            *state = SignState::Pending {
                transcript: Some(transcript),
                signature: None,
            };
        }
        let _guard = SignGuard(self);
        loop {
            {
                let mut state = self
                    .0
                    .lock()
                    .map_err(|_| TransportError::PeerAuthenticationRejected)?;
                match &mut *state {
                    SignState::Pending { signature, .. } => {
                        if let Some(signature) = signature.take() {
                            return Ok(signature);
                        }
                    }
                    _ => return Err(TransportError::PeerAuthenticationRejected),
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn poll(&self) -> Option<Vec<u8>> {
        match &mut *self.0.lock().ok()? {
            SignState::Pending { transcript, .. } => transcript.take(),
            _ => None,
        }
    }

    fn submit(&self, value: Vec<u8>) -> bool {
        if !(8..=72).contains(&value.len()) {
            return false;
        }
        let Ok(mut state) = self.0.lock() else {
            return false;
        };
        match &mut *state {
            SignState::Pending {
                transcript: None,
                signature,
            } if signature.is_none() => {
                *signature = Some(value);
                true
            }
            _ => false,
        }
    }

    pub(super) fn close(&self) {
        if let Ok(mut state) = self.0.lock() {
            *state = SignState::Closed;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_sender_SenderNative_identitySummary(
    env: JNIEnv<'_>,
    _object: JObject<'_>,
    spki: JByteArray<'_>,
) -> jstring {
    jni_identity(&env, &spki)
        .and_then(|identity| {
            let fingerprint = DeviceFingerprint::from_public_key(&identity.public_key).ok()?;
            env.new_string(format!(
                "{}\nSHA-256 (sender):\n{}",
                identity.device_id, fingerprint
            ))
            .ok()
        })
        .map_or(std::ptr::null_mut(), |text| text.into_raw())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_sender_SenderNative_startWithIdentity(
    mut env: JNIEnv<'_>,
    _object: JObject<'_>,
    ticket: JString<'_>,
    pin: JString<'_>,
    width: jint,
    height: jint,
    fps: jint,
    audio: jboolean,
    reference: jboolean,
    spki: JByteArray<'_>,
) -> jlong {
    let Some(identity) = jni_identity(&env, &spki) else {
        return -1;
    };
    let Some(ticket) = bounded_text(&mut env, &ticket, MAX_PAIRING_TICKET_BYTES)
        .and_then(|s| PairingTicket::parse(&s).ok())
    else {
        return -1;
    };
    let Some(pin) = bounded_text(&mut env, &pin, 6).and_then(|s| PairingCode::parse(s).ok()) else {
        return -1;
    };
    let config = if reference != 0 {
        if fps != 30 || audio == 0 || !matches!((width, height), (1280, 720) | (1920, 1080)) {
            return -1;
        }
        SenderConfig {
            lifetime: Duration::from_secs(120),
            ..SenderConfig::reference(width as u32, height as u32)
        }
    } else {
        let Some(config) = SenderConfig::capture(width, height, fps, audio != 0) else {
            return -1;
        };
        config
    };
    start_sender_with_identity(ticket, pin, config, Some(identity))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_sender_SenderNative_pollSigningRequest(
    env: JNIEnv<'_>,
    _object: JObject<'_>,
    id: jlong,
) -> jni::sys::jbyteArray {
    SENDER
        .lock()
        .ok()
        .and_then(|slot| {
            let worker = slot.as_ref().filter(|worker| {
                worker.id == id
                    && !worker.cancel.load(Ordering::Acquire)
                    && worker.status.load(Ordering::Acquire) == 1
            })?;
            worker.signing.poll()
        })
        .and_then(|bytes| env.byte_array_from_slice(&bytes).ok())
        .map_or(std::ptr::null_mut(), |array| array.into_raw())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_sender_SenderNative_submitSignature(
    env: JNIEnv<'_>,
    _object: JObject<'_>,
    id: jlong,
    signature: JByteArray<'_>,
) -> jint {
    if !matches!(env.get_array_length(&signature), Ok(8..=72)) {
        return -1;
    }
    let Ok(value) = env.convert_byte_array(&signature) else {
        return -1;
    };
    let Ok(slot) = SENDER.lock() else {
        return -1;
    };
    let Some(worker) = slot.as_ref().filter(|worker| {
        worker.id == id
            && !worker.cancel.load(Ordering::Acquire)
            && worker.status.load(Ordering::Acquire) == 1
    }) else {
        return -1;
    };
    if worker.signing.submit(value) { 0 } else { -1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_canonical_p256_spki_is_accepted() {
        let mut spki = SPKI_PREFIX.to_vec();
        spki.extend_from_slice(&[4; 65]);
        let identity = parse_identity(&spki).unwrap();
        assert_eq!(identity.device_id.len(), 39);
        assert_eq!(identity, parse_identity(&spki).unwrap());
        for length in 0..91 {
            assert!(parse_identity(&spki[..length]).is_none());
        }
        for index in 0..27 {
            let mut changed = spki.clone();
            changed[index] ^= 1;
            assert!(parse_identity(&changed).is_none());
        }
        spki.push(0);
        assert!(parse_identity(&spki).is_none());
    }

    #[tokio::test]
    async fn signing_is_single_use_bounded_and_cancel_safe() {
        let bridge = Arc::new(SigningBridge::default());
        assert!(!bridge.submit(vec![1; 72]));
        let child = bridge.clone();
        let job = tokio::spawn(async move { child.sign(vec![1; 200]).await });
        tokio::task::yield_now().await;
        assert!(!bridge.submit(vec![1; 72])); // Cannot reply before poll.
        assert_eq!(bridge.poll(), Some(vec![1; 200]));
        assert!(bridge.poll().is_none());
        assert!(!bridge.submit(vec![1; 73]));
        assert!(bridge.submit(vec![2; 72]));
        assert!(!bridge.submit(vec![2; 72]));
        assert_eq!(job.await.unwrap().unwrap(), vec![2; 72]);
        assert!(!bridge.submit(vec![2; 72]));
        assert!(bridge.sign(vec![1]).await.is_err());
        let bridge = SigningBridge::default();
        assert!(
            tokio::time::timeout(Duration::from_millis(20), bridge.sign(vec![1]))
                .await
                .is_err()
        );
        assert!(bridge.poll().is_none());
        assert!(!bridge.submit(vec![1; 72]));
    }
}
