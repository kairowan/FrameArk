//! Experimental FANP QUIC/TLS transport with one-shot pairing.
//!
//! This crate owns the first protocol handshake, framing limits, and temporary
//! pairing state. It intentionally does not persist device identity, carry media
//! tracks, or provide an internet relay. The certificate is generated per server
//! process and must be pinned by the caller for the session.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use frameark_core::{Session, SessionState, SessionTransition};
use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::{ClientConfig, Connection, Endpoint, ServerConfig};
use rand::Rng;
use rcgen::generate_simple_self_signed;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use tokio::time::timeout;

mod control;
mod negotiation;

pub use control::{ControlMessage, PendingControl};

pub use negotiation::{
    CapabilityOffer, FANP_CAPABILITY_VERSION, MAX_CAPABILITIES, NegotiatedCapabilities,
    NegotiationError,
};

/// ALPN identifier for the versioned FrameArk Native Protocol transport.
pub const FANP_ALPN: &[u8] = b"frameark/1";
/// Current wire version for the pairing handshake.
pub const FANP_PROTOCOL_VERSION: u8 = 1;
/// Maximum encoded payload accepted in one FANP control frame.
pub const MAX_FRAME_PAYLOAD: usize = 256;
const FRAME_HEADER_LEN: usize = 8;
const PAIRING_CODE_LEN: usize = 6;
const MAX_CERTIFICATE_BYTES: usize = 16 * 1024;
const MAX_SERVER_NAME_BYTES: usize = 253;
const PAIR_REQUEST: u8 = 1;
const PAIR_ACCEPTED: u8 = 2;
const PAIR_REJECTED: u8 = 3;
const CLIENT_HELLO: u8 = 4;
const SERVER_HELLO: u8 = 5;
const NEGOTIATION_REJECTED: u8 = 6;

/// Errors returned by the bounded FANP transport and pairing handshake.
#[derive(Debug, Eq, PartialEq)]
pub enum TransportError {
    /// The pairing code was not exactly six ASCII decimal digits.
    InvalidPairingCode,
    /// A frame violated the FANP envelope or declared an oversized payload.
    InvalidFrame,
    /// A peer selected a protocol version this crate does not implement.
    UnsupportedVersion(u8),
    /// The peer rejected the temporary pairing code.
    PairingRejected,
    /// The peer rejected or could not complete capability negotiation.
    NegotiationRejected,
    /// Local capability validation failed before or during negotiation.
    Negotiation(NegotiationError),
    /// A QUIC stream or connection closed before the handshake completed.
    ConnectionClosed,
    /// The endpoint could not be created or configured.
    Endpoint,
    /// TLS certificate generation or configuration failed.
    TlsConfiguration,
    /// A bounded operation exceeded its caller-supplied timeout.
    Timeout,
    /// The endpoint was closed before a connection arrived.
    EndpointClosed,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPairingCode => f.write_str("invalid temporary pairing code"),
            Self::InvalidFrame => f.write_str("invalid or oversized FANP frame"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported FANP protocol version {version}")
            }
            Self::PairingRejected => f.write_str("temporary pairing was rejected"),
            Self::NegotiationRejected => f.write_str("FANP capability negotiation was rejected"),
            Self::Negotiation(error) => write!(f, "FANP capability negotiation error: {error}"),
            Self::ConnectionClosed => f.write_str("FANP connection closed during pairing"),
            Self::Endpoint => f.write_str("FANP QUIC endpoint error"),
            Self::TlsConfiguration => f.write_str("FANP TLS configuration error"),
            Self::Timeout => f.write_str("FANP operation timed out"),
            Self::EndpointClosed => f.write_str("FANP endpoint is closed"),
        }
    }
}

impl std::error::Error for TransportError {}

/// A six-digit, process-local pairing code.
#[derive(Clone, Eq, PartialEq)]
pub struct PairingCode(String);

impl fmt::Debug for PairingCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairingCode([REDACTED])")
    }
}

impl PairingCode {
    /// Generates a cryptographically seeded six-digit code for display by a UI.
    pub fn generate() -> Self {
        let mut rng = rand::rng();
        let value = rng.random_range(0..1_000_000_u32);
        Self(format!("{value:06}"))
    }

    /// Parses and validates a user-entered six-digit code.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, TransportError> {
        let value = value.as_ref();
        if value.len() != PAIRING_CODE_LEN || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(TransportError::InvalidPairingCode);
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the code in display/input form.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A temporary authenticated FANP session.
///
/// The session is intentionally not a persistent trust grant. Callers must
/// close it when the media session ends and must build a separate policy for
/// future connections.
pub struct PairingSession {
    connection: Connection,
    negotiated: NegotiatedCapabilities,
    // A client endpoint must outlive its connection. Server endpoints are held
    // by PairingServer, so this is `Some` only for client-created sessions.
    _endpoint: Option<Endpoint>,
}

impl Drop for PairingSession {
    fn drop(&mut self) {
        self.close();
    }
}

impl PairingSession {
    /// Whether QUIC has recorded terminal connection closure.
    pub fn is_closed(&self) -> bool {
        self.connection.close_reason().is_some()
    }

    /// Waits for peer shutdown while retaining endpoint ownership.
    pub async fn wait_closed(&self, budget: Duration) -> Result<(), TransportError> {
        timeout(budget, self.connection.closed())
            .await
            .map_err(|_| TransportError::Timeout)?;
        Ok(())
    }

    /// Returns the peer address selected by QUIC after the handshake.
    pub fn remote_address(&self) -> SocketAddr {
        self.connection.remote_address()
    }

    /// Closes the session with an application-level reason.
    pub fn close(&self) {
        self.connection
            .close(0u32.into(), b"frameark session closed");
    }

    /// Indicates that this session has no persistent identity or trust record.
    pub const fn is_temporary(&self) -> bool {
        true
    }

    /// Returns the capability intersection selected during the handshake.
    pub fn negotiated_capabilities(&self) -> &NegotiatedCapabilities {
        &self.negotiated
    }

    /// Advances a shared core session through transport, authentication, and
    /// capability negotiation states.
    ///
    /// Protocol adapters call this after `PairingClient::connect` or
    /// `PairingServer::accept_pairing`. The core remains the owner of the
    /// lifecycle rules; this helper only applies the known FANP milestones.
    pub fn advance_core_session(
        &self,
        session: &mut Session,
    ) -> frameark_core::Result<Vec<SessionTransition>> {
        let mut transitions = Vec::new();
        loop {
            let next = match session.state() {
                SessionState::Idle => SessionState::Connecting,
                SessionState::Connecting => SessionState::Authenticating,
                SessionState::Authenticating => SessionState::Negotiating,
                SessionState::Recovering => SessionState::Connecting,
                SessionState::Negotiating => break,
                _ => {
                    return Err(frameark_core::FrameArkError::invalid_state(
                        "fanp.session_sync",
                        format!(
                            "cannot apply a completed FANP handshake to {} session",
                            session.state()
                        ),
                    ));
                }
            };
            transitions.push(session.transition(next)?);
        }
        Ok(transitions)
    }
}

/// A QUIC/TLS listener that accepts one-shot temporary pairings.
pub struct PairingServer {
    endpoint: Endpoint,
    pairing_code: PairingCode,
    certificate_der: Vec<u8>,
    capabilities: CapabilityOffer,
}

impl PairingServer {
    /// Binds a local QUIC listener with an ephemeral self-signed certificate.
    ///
    /// `server_name` must match the name used by the client for TLS hostname
    /// verification (normally the mDNS host name or `localhost` in tests).
    pub fn bind(
        bind_addr: SocketAddr,
        server_name: impl Into<String>,
        pairing_code: PairingCode,
    ) -> Result<Self, TransportError> {
        Self::bind_with_capabilities(
            bind_addr,
            server_name,
            pairing_code,
            CapabilityOffer::default_capabilities(),
        )
    }

    /// Binds a listener with an explicit capability offer.
    pub fn bind_with_capabilities(
        bind_addr: SocketAddr,
        server_name: impl Into<String>,
        pairing_code: PairingCode,
        capabilities: CapabilityOffer,
    ) -> Result<Self, TransportError> {
        let server_name = server_name.into();
        if !valid_server_name(&server_name) {
            return Err(TransportError::TlsConfiguration);
        }
        let certificate = generate_simple_self_signed(vec![server_name])
            .map_err(|_| TransportError::TlsConfiguration)?;
        let certificate_der = certificate.cert.der().to_vec();
        let cert = CertificateDer::from(certificate_der.clone());
        let private_key = PrivatePkcs8KeyDer::from(certificate.key_pair.serialize_der());
        let mut tls_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert], private_key.into())
            .map_err(|_| TransportError::TlsConfiguration)?;
        tls_config.alpn_protocols = vec![FANP_ALPN.to_vec()];
        let quic_config =
            QuicServerConfig::try_from(tls_config).map_err(|_| TransportError::TlsConfiguration)?;
        let endpoint =
            Endpoint::server(ServerConfig::with_crypto(Arc::new(quic_config)), bind_addr)
                .map_err(|_| TransportError::Endpoint)?;

        Ok(Self {
            endpoint,
            pairing_code,
            certificate_der,
            capabilities,
        })
    }

    /// Returns the UDP address selected by the operating system.
    pub fn local_addr(&self) -> Result<SocketAddr, TransportError> {
        self.endpoint
            .local_addr()
            .map_err(|_| TransportError::Endpoint)
    }

    /// Returns the DER certificate that an in-scope client must pin for this session.
    pub fn certificate_der(&self) -> &[u8] {
        &self.certificate_der
    }

    /// Waits for one incoming connection and completes temporary pairing.
    pub async fn accept_pairing(
        &self,
        operation_timeout: Duration,
    ) -> Result<PairingSession, TransportError> {
        let incoming = timeout(operation_timeout, self.endpoint.accept())
            .await
            .map_err(|_| TransportError::Timeout)?
            .ok_or(TransportError::EndpointClosed)?;
        let connection = timeout(operation_timeout, incoming)
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|_| TransportError::ConnectionClosed)?;
        let (mut send, mut receive) = timeout(operation_timeout, connection.accept_bi())
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|_| TransportError::ConnectionClosed)?;
        let frame = read_frame_with_timeout(&mut receive, operation_timeout).await?;
        if frame.kind != PAIR_REQUEST {
            reject(&mut send).await?;
            return Err(TransportError::InvalidFrame);
        }
        let supplied_code = PairingCode::parse(String::from_utf8_lossy(&frame.payload))
            .map_err(|_| TransportError::InvalidPairingCode)?;
        if supplied_code != self.pairing_code {
            reject(&mut send).await?;
            // Let the client observe the finished rejection frame before the
            // server drops its connection handle. The delay is deliberately
            // short and bounded; rejected peers never become persistent state.
            let _ = timeout(operation_timeout, send.stopped()).await;
            return Err(TransportError::PairingRejected);
        }
        write_frame_with_timeout(&mut send, PAIR_ACCEPTED, &[], operation_timeout).await?;
        let (mut hello_send, mut hello_receive) =
            timeout(operation_timeout, connection.accept_bi())
                .await
                .map_err(|_| TransportError::Timeout)?
                .map_err(|_| TransportError::ConnectionClosed)?;
        let hello = read_frame_with_timeout(&mut hello_receive, operation_timeout).await?;
        if hello.kind != CLIENT_HELLO {
            reject_negotiation(&mut hello_send).await?;
            return Err(TransportError::NegotiationRejected);
        }
        let client_capabilities = match CapabilityOffer::decode(&hello.payload) {
            Ok(offer) => offer,
            Err(error) => {
                reject_negotiation(&mut hello_send).await?;
                let _ = timeout(operation_timeout, hello_send.stopped()).await;
                return Err(TransportError::Negotiation(error));
            }
        };
        let negotiated = match self.capabilities.intersect(&client_capabilities) {
            Ok(capabilities) => capabilities,
            Err(error) => {
                reject_negotiation(&mut hello_send).await?;
                let _ = timeout(operation_timeout, hello_send.stopped()).await;
                return Err(TransportError::Negotiation(error));
            }
        };
        write_frame_with_timeout(
            &mut hello_send,
            SERVER_HELLO,
            &negotiated.encode(),
            operation_timeout,
        )
        .await?;
        Ok(PairingSession {
            connection,
            negotiated,
            _endpoint: None,
        })
    }

    /// Stops accepting new connections and closes existing sessions.
    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"frameark server closed");
    }
}

/// A client that pins the server's ephemeral certificate for one pairing.
pub struct PairingClient;

impl PairingClient {
    /// Connects, verifies the pinned certificate, and exchanges the pairing code.
    pub async fn connect(
        server_addr: SocketAddr,
        server_name: &str,
        certificate_der: &[u8],
        pairing_code: PairingCode,
        operation_timeout: Duration,
    ) -> Result<PairingSession, TransportError> {
        Self::connect_with_capabilities(
            server_addr,
            server_name,
            certificate_der,
            pairing_code,
            CapabilityOffer::default_capabilities(),
            operation_timeout,
        )
        .await
    }

    /// Connects with an explicit capability offer and performs FANP negotiation.
    pub async fn connect_with_capabilities(
        server_addr: SocketAddr,
        server_name: &str,
        certificate_der: &[u8],
        pairing_code: PairingCode,
        capabilities: CapabilityOffer,
        operation_timeout: Duration,
    ) -> Result<PairingSession, TransportError> {
        if !valid_server_name(server_name)
            || certificate_der.is_empty()
            || certificate_der.len() > MAX_CERTIFICATE_BYTES
        {
            return Err(TransportError::TlsConfiguration);
        }
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(CertificateDer::from(certificate_der.to_vec()))
            .map_err(|_| TransportError::TlsConfiguration)?;
        let mut tls_config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        tls_config.alpn_protocols = vec![FANP_ALPN.to_vec()];
        let quic_config =
            QuicClientConfig::try_from(tls_config).map_err(|_| TransportError::TlsConfiguration)?;
        let bind_addr = match server_addr {
            SocketAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
            SocketAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
        };
        let mut endpoint = Endpoint::client(bind_addr).map_err(|_| TransportError::Endpoint)?;
        endpoint.set_default_client_config(ClientConfig::new(Arc::new(quic_config)));
        let connecting = endpoint
            .connect(server_addr, server_name)
            .map_err(|_| TransportError::Endpoint)?;
        let connection = timeout(operation_timeout, connecting)
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|_| TransportError::ConnectionClosed)?;
        let (mut send, mut receive) = timeout(operation_timeout, connection.open_bi())
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|_| TransportError::ConnectionClosed)?;
        write_frame_with_timeout(
            &mut send,
            PAIR_REQUEST,
            pairing_code.as_str().as_bytes(),
            operation_timeout,
        )
        .await?;
        let frame = read_frame_with_timeout(&mut receive, operation_timeout).await?;
        match frame.kind {
            PAIR_ACCEPTED if frame.payload.is_empty() => {
                let (mut hello_send, mut hello_receive) =
                    timeout(operation_timeout, connection.open_bi())
                        .await
                        .map_err(|_| TransportError::Timeout)?
                        .map_err(|_| TransportError::ConnectionClosed)?;
                write_frame_with_timeout(
                    &mut hello_send,
                    CLIENT_HELLO,
                    &capabilities.encode(),
                    operation_timeout,
                )
                .await?;
                let hello = read_frame_with_timeout(&mut hello_receive, operation_timeout).await?;
                let negotiated = match hello.kind {
                    SERVER_HELLO => {
                        let server_capabilities = CapabilityOffer::decode(&hello.payload)
                            .map_err(TransportError::Negotiation)?;
                        capabilities
                            .intersect(&server_capabilities)
                            .map_err(TransportError::Negotiation)?
                    }
                    NEGOTIATION_REJECTED => {
                        connection.close(0u32.into(), b"capability negotiation rejected");
                        return Err(TransportError::NegotiationRejected);
                    }
                    _ => return Err(TransportError::InvalidFrame),
                };
                Ok(PairingSession {
                    connection,
                    negotiated,
                    _endpoint: Some(endpoint),
                })
            }
            PAIR_REJECTED => {
                connection.close(0u32.into(), b"pairing rejected");
                Err(TransportError::PairingRejected)
            }
            _ => Err(TransportError::InvalidFrame),
        }
    }
}

async fn reject(send: &mut quinn::SendStream) -> Result<(), TransportError> {
    write_frame(send, PAIR_REJECTED, &[]).await
}

async fn reject_negotiation(send: &mut quinn::SendStream) -> Result<(), TransportError> {
    write_frame(send, NEGOTIATION_REJECTED, &[]).await
}

async fn write_frame(
    send: &mut quinn::SendStream,
    kind: u8,
    payload: &[u8],
) -> Result<(), TransportError> {
    if payload.len() > MAX_FRAME_PAYLOAD {
        return Err(TransportError::InvalidFrame);
    }
    let length = u16::try_from(payload.len()).map_err(|_| TransportError::InvalidFrame)?;
    let mut header = [0_u8; FRAME_HEADER_LEN];
    header[..4].copy_from_slice(b"FANP");
    header[4] = FANP_PROTOCOL_VERSION;
    header[5] = kind;
    header[6..].copy_from_slice(&length.to_be_bytes());
    send.write_all(&header)
        .await
        .map_err(|_| TransportError::ConnectionClosed)?;
    send.write_all(payload)
        .await
        .map_err(|_| TransportError::ConnectionClosed)?;
    send.finish().map_err(|_| TransportError::ConnectionClosed)
}

async fn read_frame_with_timeout(
    receive: &mut quinn::RecvStream,
    operation_timeout: Duration,
) -> Result<Frame, TransportError> {
    timeout(operation_timeout, read_frame(receive))
        .await
        .map_err(|_| TransportError::Timeout)?
}

async fn write_frame_with_timeout(
    send: &mut quinn::SendStream,
    kind: u8,
    payload: &[u8],
    operation_timeout: Duration,
) -> Result<(), TransportError> {
    timeout(operation_timeout, write_frame(send, kind, payload))
        .await
        .map_err(|_| TransportError::Timeout)?
}

async fn read_frame(receive: &mut quinn::RecvStream) -> Result<Frame, TransportError> {
    let mut header = [0_u8; FRAME_HEADER_LEN];
    receive
        .read_exact(&mut header)
        .await
        .map_err(|_| TransportError::ConnectionClosed)?;
    let (kind, payload_len) = decode_header(&header)?;
    let mut payload = vec![0_u8; payload_len];
    receive
        .read_exact(&mut payload)
        .await
        .map_err(|_| TransportError::ConnectionClosed)?;
    // One frame per stream: trailing bytes, duplicate frames, and absent FIN
    // are never silently interpreted as a successful request.
    if receive
        .read(&mut [0_u8; 1])
        .await
        .map_err(|_| TransportError::ConnectionClosed)?
        .is_some()
    {
        return Err(TransportError::InvalidFrame);
    }
    Ok(Frame { kind, payload })
}

fn decode_header(header: &[u8; FRAME_HEADER_LEN]) -> Result<(u8, usize), TransportError> {
    if &header[..4] != b"FANP" {
        return Err(TransportError::InvalidFrame);
    }
    if header[4] != FANP_PROTOCOL_VERSION {
        return Err(TransportError::UnsupportedVersion(header[4]));
    }
    let payload_len = u16::from_be_bytes([header[6], header[7]]) as usize;
    if payload_len > MAX_FRAME_PAYLOAD {
        return Err(TransportError::InvalidFrame);
    }
    Ok((header[5], payload_len))
}

struct Frame {
    kind: u8,
    payload: Vec<u8>,
}

fn valid_server_name(server_name: &str) -> bool {
    !server_name.trim().is_empty()
        && server_name.len() <= MAX_SERVER_NAME_BYTES
        && server_name.is_ascii()
        && !server_name.bytes().any(|byte| byte.is_ascii_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;
    use frameark_core::{Capability, DeviceId, Session, SessionId, SessionState};
    use std::sync::Arc;

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    #[test]
    fn pairing_code_is_bounded_and_zero_padded() {
        let code = PairingCode::parse("004207").expect("valid code");
        assert_eq!(code.as_str(), "004207");
        assert!(PairingCode::parse("4207").is_err());
        assert!(PairingCode::parse("00420a").is_err());
        assert_eq!(PairingCode::generate().as_str().len(), PAIRING_CODE_LEN);
    }

    #[test]
    fn frame_header_rejects_bad_magic_versions_and_lengths() {
        let mut header = [0_u8; FRAME_HEADER_LEN];
        header[..4].copy_from_slice(b"NOPE");
        assert_eq!(decode_header(&header), Err(TransportError::InvalidFrame));

        header[..4].copy_from_slice(b"FANP");
        header[4] = FANP_PROTOCOL_VERSION + 1;
        assert_eq!(
            decode_header(&header),
            Err(TransportError::UnsupportedVersion(
                FANP_PROTOCOL_VERSION + 1
            ))
        );

        header[4] = FANP_PROTOCOL_VERSION;
        header[6..].copy_from_slice(&257_u16.to_be_bytes());
        assert_eq!(decode_header(&header), Err(TransportError::InvalidFrame));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pinned_quic_pairing_completes_a_temporary_session() {
        let server = Arc::new(
            PairingServer::bind(
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
                "localhost",
                PairingCode::parse("004207").unwrap(),
            )
            .unwrap(),
        );
        let address = server.local_addr().unwrap();
        let certificate = server.certificate_der().to_vec();
        let accepting = Arc::clone(&server);
        let server_task = tokio::spawn(async move { accepting.accept_pairing(TEST_TIMEOUT).await });

        let client = PairingClient::connect(
            address,
            "localhost",
            &certificate,
            PairingCode::parse("004207").unwrap(),
            TEST_TIMEOUT,
        )
        .await
        .unwrap();
        assert!(client.is_temporary());
        assert_eq!(
            client.remote_address().ip(),
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        );
        let server_session = server_task.await.unwrap().unwrap();
        assert!(server_session.is_temporary());
        client.close();
        server_session.close();
        server.close();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn incorrect_code_is_rejected_without_persisting_trust() {
        let server = Arc::new(
            PairingServer::bind(
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
                "localhost",
                PairingCode::parse("004207").unwrap(),
            )
            .unwrap(),
        );
        let address = server.local_addr().unwrap();
        let certificate = server.certificate_der().to_vec();
        let accepting = Arc::clone(&server);
        let server_task = tokio::spawn(async move { accepting.accept_pairing(TEST_TIMEOUT).await });

        let client_error = match PairingClient::connect(
            address,
            "localhost",
            &certificate,
            PairingCode::parse("004208").unwrap(),
            TEST_TIMEOUT,
        )
        .await
        {
            Err(error) => error,
            Ok(session) => {
                session.close();
                panic!("incorrect pairing code unexpectedly succeeded")
            }
        };
        assert_eq!(client_error, TransportError::PairingRejected);
        let server_result = server_task.await.unwrap();
        assert!(matches!(
            server_result,
            Err(TransportError::PairingRejected)
        ));
        server.close();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn capability_intersection_advances_the_shared_core_session() {
        let server_offer = CapabilityOffer::new([Capability::Video, Capability::Audio]).unwrap();
        let client_offer =
            CapabilityOffer::new([Capability::Audio, Capability::RemoteControl]).unwrap();
        let server = Arc::new(
            PairingServer::bind_with_capabilities(
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
                "localhost",
                PairingCode::parse("004207").unwrap(),
                server_offer,
            )
            .unwrap(),
        );
        let address = server.local_addr().unwrap();
        let certificate = server.certificate_der().to_vec();
        let accepting = Arc::clone(&server);
        let server_task = tokio::spawn(async move { accepting.accept_pairing(TEST_TIMEOUT).await });

        let client = PairingClient::connect_with_capabilities(
            address,
            "localhost",
            &certificate,
            PairingCode::parse("004207").unwrap(),
            client_offer,
            TEST_TIMEOUT,
        )
        .await
        .unwrap();
        assert!(client.negotiated_capabilities().supports(Capability::Audio));
        assert!(!client.negotiated_capabilities().supports(Capability::Video));
        assert!(
            !client
                .negotiated_capabilities()
                .supports(Capability::RemoteControl)
        );

        let mut core_session = Session::new(
            SessionId::try_from("fanp-session").unwrap(),
            DeviceId::try_from("receiver-1").unwrap(),
        );
        let transitions = client.advance_core_session(&mut core_session).unwrap();
        assert_eq!(transitions.len(), 3);
        assert_eq!(core_session.state(), SessionState::Negotiating);
        let server_session = server_task.await.unwrap().unwrap();
        assert_eq!(
            server_session.negotiated_capabilities(),
            client.negotiated_capabilities()
        );
        client.close();
        server_session.close();
        server.close();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn no_common_capability_is_rejected() {
        let server = Arc::new(
            PairingServer::bind_with_capabilities(
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
                "localhost",
                PairingCode::parse("004207").unwrap(),
                CapabilityOffer::new([Capability::Video]).unwrap(),
            )
            .unwrap(),
        );
        let address = server.local_addr().unwrap();
        let certificate = server.certificate_der().to_vec();
        let accepting = Arc::clone(&server);
        let server_task = tokio::spawn(async move { accepting.accept_pairing(TEST_TIMEOUT).await });

        let client_error = match PairingClient::connect_with_capabilities(
            address,
            "localhost",
            &certificate,
            PairingCode::parse("004207").unwrap(),
            CapabilityOffer::new([Capability::Audio]).unwrap(),
            TEST_TIMEOUT,
        )
        .await
        {
            Err(error) => error,
            Ok(session) => {
                session.close();
                panic!("incompatible capability offers unexpectedly succeeded")
            }
        };
        assert_eq!(client_error, TransportError::NegotiationRejected);
        let server_error = match server_task.await.unwrap() {
            Err(error) => error,
            Ok(session) => {
                session.close();
                panic!("server accepted incompatible capability offers")
            }
        };
        assert_eq!(
            server_error,
            TransportError::Negotiation(NegotiationError::NoCommonCapability)
        );
        server.close();
    }
}
