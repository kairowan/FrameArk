//! Experimental FANP session control, using the shared core lifecycle.
//!
//! This crate negotiates configuration and drives platform-owned backends. It
//! carries bounded FAM1 media to those backends but does not decode codecs or
//! own platform surfaces.
pub mod media_control;
pub mod media_receiver;
pub mod media_session;
pub mod media_wire;
mod offer;
pub mod wire;

use frameark_core::{
    CoreEvent, DiagnosticEvent, ErrorKind, EventLevel, FrameArkError, Result, Session, SessionState,
};
use frameark_transport::{PairingSession, TransportError};
use std::collections::VecDeque;
use std::time::Duration;
use wire::{Command, Request, Response, Status};

pub use media_control::{
    MediaRendererFactory, NativeMediaControlReceiver, NativeMediaSessionReport,
};
pub use media_receiver::{MediaStreamReport, NativeMediaReceiver};
pub use offer::{ReceiverPolicy, SessionOffer};

/// Maximum retained lifecycle/diagnostic events per connection.
pub const MAX_EVENTS: usize = 64;
/// Maximum requests served per connection, including status queries.
pub const MAX_REQUESTS: u32 = 1024;

/// Platform adapter boundary. Implementations must return promptly and must
/// not run blocking I/O on the async executor. Failure summaries are redacted.
/// A successful prepare means the output is actually ready for start.
pub trait SessionBackend {
    /// Validate and prepare platform resources for this exact configuration.
    fn prepare(&mut self, offer: SessionOffer) -> Result<()>;
    /// Activate a previously prepared output.
    fn start(&mut self) -> Result<()>;
    /// Release all owned resources; must be idempotent and safe after partial prepare.
    fn reset(&mut self) -> Result<()>;
}

/// Bounded, redacted summary; contains neither peer address nor pairing code.
#[derive(Clone, Debug)]
pub struct SessionReport {
    /// Final state.
    pub state: SessionState,
    /// Number of sequenced control requests accepted.
    pub requests: u32,
    /// Recent core events (oldest are evicted at MAX_EVENTS).
    pub events: Vec<CoreEvent>,
}

struct Lifecycle {
    session: Session,
    events: VecDeque<CoreEvent>,
}

impl Lifecycle {
    fn new(transport: &PairingSession, mut session: Session) -> Result<Self> {
        // Only a fresh, locally owned session can be attached to this connection.
        if session.state() != SessionState::Idle {
            return Err(FrameArkError::invalid_state(
                "fanp.attach",
                "a fresh core session is required",
            ));
        }
        let transitions = transport.advance_core_session(&mut session)?;
        let events = transitions
            .into_iter()
            .map(CoreEvent::SessionTransition)
            .collect();
        Ok(Self { session, events })
    }
    fn event(&mut self, event: CoreEvent) {
        if self.events.len() == MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
    fn transition(&mut self, state: SessionState) -> Result<()> {
        let event = self.session.transition(state)?;
        self.event(CoreEvent::SessionTransition(event));
        Ok(())
    }
    fn diagnostic(&mut self, code: &'static str) {
        self.event(CoreEvent::Diagnostic(DiagnosticEvent::new(
            EventLevel::Info,
            code,
            "native control lifecycle",
        )));
    }
    fn close(&mut self) {
        if self.session.state() != SessionState::Closed {
            let _ = self.transition(SessionState::Closed);
        }
    }
    fn report(&self, requests: u32) -> SessionReport {
        SessionReport {
            state: self.session.state(),
            requests,
            events: self.events.iter().cloned().collect(),
        }
    }
}

fn transport_error(error: TransportError) -> FrameArkError {
    let kind = if error == TransportError::Timeout {
        ErrorKind::Timeout
    } else {
        ErrorKind::Transport
    };
    FrameArkError::new(kind, "fanp.transport", error.to_string())
}
fn protocol_error() -> FrameArkError {
    FrameArkError::invalid_argument("fanp.response", "unexpected native response")
}
fn state_error() -> FrameArkError {
    FrameArkError::invalid_state(
        "fanp.command_state",
        "command not permitted in current state",
    )
}

/// Sender-side lifecycle owner for a single paired QUIC connection.
pub struct NativeSender {
    transport: PairingSession,
    lifecycle: Lifecycle,
    next_id: u32,
}

impl NativeSender {
    /// Attaches a freshly created core session after pairing/capability negotiation.
    pub fn new(transport: PairingSession, session: Session) -> Result<Self> {
        let lifecycle = Lifecycle::new(&transport, session)?;
        Ok(Self {
            transport,
            lifecycle,
            next_id: 1,
        })
    }

    /// Returns effective state; a cancelled/disconnected transport is terminal.
    pub fn state(&self) -> SessionState {
        if self.transport.is_closed() {
            SessionState::Closed
        } else {
            self.lifecycle.session.state()
        }
    }

    /// Proposes an exact media configuration, without silent quality fallback.
    pub async fn offer(&mut self, offer: SessionOffer, budget: Duration) -> Result<()> {
        if self.state() != SessionState::Negotiating {
            return Err(state_error());
        }
        offer.validate_capabilities(self.transport.negotiated_capabilities())?;
        self.exchange(
            Command::Offer(offer),
            SessionState::Preparing,
            Some(offer),
            budget,
        )
        .await?;
        self.lifecycle.transition(SessionState::Preparing)?;
        self.lifecycle.diagnostic("fanp.offer.accepted");
        Ok(())
    }

    /// Starts the receiver only after its backend successfully prepared.
    pub async fn start(&mut self, budget: Duration) -> Result<()> {
        if self.state() != SessionState::Preparing {
            return Err(state_error());
        }
        self.exchange(Command::Start, SessionState::Streaming, None, budget)
            .await?;
        self.lifecycle.transition(SessionState::Streaming)?;
        self.lifecycle.diagnostic("fanp.started");
        Ok(())
    }

    /// Opens the sender-owned media stream only after the control plane entered
    /// Streaming. The returned stream carries bounded FAM1 records.
    pub async fn open_media_stream(&self) -> Result<frameark_transport::MediaSender> {
        if self.state() != SessionState::Streaming {
            return Err(state_error());
        }
        self.transport
            .open_media_stream()
            .await
            .map_err(transport_error)
    }

    /// Queries receiver state and validates it against the local shared lifecycle.
    pub async fn query_status(&mut self, budget: Duration) -> Result<SessionState> {
        let state = self.state();
        if state == SessionState::Closed {
            return Err(state_error());
        }
        self.exchange(Command::Status, state, None, budget).await?;
        Ok(state)
    }

    /// Ends the session, waits for remote teardown, and returns a bounded summary.
    pub async fn stop(&mut self, budget: Duration) -> Result<SessionReport> {
        if self.state() == SessionState::Closed {
            return Err(state_error());
        }
        self.exchange(Command::Stop, SessionState::Closed, None, budget)
            .await?;
        if self.lifecycle.session.state() == SessionState::Streaming {
            self.lifecycle.transition(SessionState::Draining)?;
        }
        // Keep the endpoint alive until the receiver acknowledges FIN and closes.
        let closed = self.transport.wait_closed(budget).await;
        self.lifecycle.close();
        closed.map_err(transport_error)?;
        self.lifecycle.diagnostic("fanp.stopped");
        Ok(self.lifecycle.report(self.next_id - 1))
    }

    /// Local cancellation without sending another message.
    pub fn abort(&mut self) {
        self.transport.close();
        self.lifecycle.close();
        self.lifecycle.diagnostic("fanp.aborted");
    }

    async fn exchange(
        &mut self,
        command: Command,
        expected_state: SessionState,
        expected_offer: Option<SessionOffer>,
        budget: Duration,
    ) -> Result<()> {
        if self.next_id > MAX_REQUESTS {
            self.abort();
            return Err(protocol_error());
        }
        let id = self.next_id;
        self.next_id += 1;
        let request = Request { id, command }.encode()?;
        let result = async {
            let message = self
                .transport
                .request_control(request, budget)
                .await
                .map_err(transport_error)?;
            let response = Response::decode(&message)?;
            if response.id != id {
                return Err(protocol_error());
            }
            if response.status != Status::Ok {
                return Err(FrameArkError::new(
                    match response.status {
                        Status::Unsupported => ErrorKind::Unsupported,
                        Status::InvalidState => ErrorKind::InvalidState,
                        Status::Backend => ErrorKind::Media,
                        _ => ErrorKind::InvalidArgument,
                    },
                    "fanp.rejected",
                    "receiver rejected the control operation",
                ));
            }
            if response.state != expected_state || response.offer != expected_offer {
                return Err(protocol_error());
            }
            Ok(())
        }
        .await;
        if result.is_err() {
            self.abort();
        }
        result
    }
}

/// Receiver-side owner of one session and one platform backend.
pub struct NativeReceiver<B: SessionBackend> {
    transport: PairingSession,
    lifecycle: Lifecycle,
    policy: ReceiverPolicy,
    backend: B,
    backend_dirty: bool,
}

impl<B: SessionBackend> NativeReceiver<B> {
    /// Attaches explicit platform limits and a fresh local core session.
    pub fn new(
        transport: PairingSession,
        session: Session,
        policy: ReceiverPolicy,
        backend: B,
    ) -> Result<Self> {
        policy.validate()?;
        let lifecycle = Lifecycle::new(&transport, session)?;
        Ok(Self {
            transport,
            lifecycle,
            policy,
            backend,
            backend_dirty: false,
        })
    }

    /// Serves sequenced requests until Stop, failure, cancellation, or idle timeout.
    /// Consuming self ensures a cancelled future cannot leave a reusable backend.
    pub async fn serve(mut self, budget: Duration) -> Result<SessionReport> {
        for expected_id in 1..=MAX_REQUESTS {
            let pending = self
                .transport
                .accept_control(budget)
                .await
                .map_err(transport_error)?;
            let request = Request::decode(pending.request())?;
            if request.id != expected_id {
                return Err(protocol_error());
            }
            let outcome = self.apply(request.command);
            let status = match &outcome {
                Ok(_) => Status::Ok,
                Err(error) => match error.kind() {
                    ErrorKind::Unsupported => Status::Unsupported,
                    ErrorKind::InvalidState => Status::InvalidState,
                    ErrorKind::Media => Status::Backend,
                    _ => Status::Invalid,
                },
            };
            let selected = outcome.as_ref().ok().copied().flatten();
            if outcome.is_err() {
                self.cleanup();
            }
            pending
                .respond(
                    Response {
                        id: request.id,
                        status,
                        state: self.lifecycle.session.state(),
                        offer: selected,
                    }
                    .encode()?,
                    budget,
                )
                .await
                .map_err(transport_error)?;
            outcome?;
            if self.lifecycle.session.state() == SessionState::Closed {
                self.lifecycle.diagnostic("fanp.stopped");
                return Ok(self.lifecycle.report(expected_id));
            }
        }
        Err(FrameArkError::invalid_argument(
            "fanp.request_limit",
            "control request budget exhausted",
        ))
    }

    fn apply(&mut self, command: Command) -> Result<Option<SessionOffer>> {
        match command {
            Command::Offer(offer) => {
                if self.lifecycle.session.state() != SessionState::Negotiating {
                    return Err(state_error());
                }
                offer.validate_capabilities(self.transport.negotiated_capabilities())?;
                self.policy.accepts(&offer)?;
                self.backend_dirty = true; // reset even when prepare fails part-way through
                self.backend.prepare(offer).map_err(|_| {
                    FrameArkError::new(
                        ErrorKind::Media,
                        "fanp.backend_prepare",
                        "receiver preparation failed",
                    )
                })?;
                self.lifecycle.transition(SessionState::Preparing)?;
                self.lifecycle.diagnostic("fanp.offer.accepted");
                Ok(Some(offer))
            }
            Command::Start => {
                if self.lifecycle.session.state() != SessionState::Preparing {
                    return Err(state_error());
                }
                self.backend.start().map_err(|_| {
                    FrameArkError::new(
                        ErrorKind::Media,
                        "fanp.backend_start",
                        "receiver start failed",
                    )
                })?;
                self.lifecycle.transition(SessionState::Streaming)?;
                self.lifecycle.diagnostic("fanp.started");
                Ok(None)
            }
            Command::Status => Ok(None),
            Command::Stop => {
                if self.lifecycle.session.state() == SessionState::Streaming {
                    self.lifecycle.transition(SessionState::Draining)?;
                }
                if self.backend_dirty {
                    self.backend.reset().map_err(|_| {
                        FrameArkError::new(
                            ErrorKind::Media,
                            "fanp.backend_reset",
                            "receiver reset failed",
                        )
                    })?;
                    self.backend_dirty = false;
                }
                self.lifecycle.close();
                Ok(None)
            }
        }
    }

    fn cleanup(&mut self) {
        if self.backend_dirty {
            let _ = self.backend.reset();
            self.backend_dirty = false;
        }
        self.lifecycle.close();
    }
}

impl<B: SessionBackend> Drop for NativeReceiver<B> {
    fn drop(&mut self) {
        self.cleanup();
        self.transport.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frameark_core::{AudioConfig, DeviceId, MediaCodec, SessionId, VideoConfig};
    use frameark_transport::{CapabilityOffer, PairingClient, PairingCode, PairingServer};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;

    struct FakeBackend {
        calls: Arc<std::sync::Mutex<Vec<&'static str>>>,
    }

    impl SessionBackend for FakeBackend {
        fn prepare(&mut self, _offer: SessionOffer) -> Result<()> {
            self.calls.lock().unwrap().push("prepare");
            Ok(())
        }

        fn start(&mut self) -> Result<()> {
            self.calls.lock().unwrap().push("start");
            Ok(())
        }

        fn reset(&mut self) -> Result<()> {
            self.calls.lock().unwrap().push("reset");
            Ok(())
        }
    }

    fn session(id: &str) -> Session {
        Session::new(
            SessionId::try_from(id).unwrap(),
            DeviceId::try_from("receiver-1").unwrap(),
        )
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn native_control_reaches_prepare_start_status_and_stop() {
        let server = Arc::new(
            PairingServer::bind_with_capabilities(
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
                "localhost",
                PairingCode::parse("123456").unwrap(),
                CapabilityOffer::default_capabilities(),
            )
            .unwrap(),
        );
        let address = server.local_addr().unwrap();
        let certificate = server.certificate_der().to_vec();
        let accepting = Arc::clone(&server);
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let receiver_calls = Arc::clone(&calls);
        let receiver_task = tokio::spawn(async move {
            let transport = accepting
                .accept_pairing(Duration::from_secs(5))
                .await
                .map_err(transport_error)?;
            let policy = ReceiverPolicy {
                video: true,
                opus: true,
                aac: false,
                max_width: 1920,
                max_height: 1080,
                max_fps: 60,
                max_channels: 2,
            };
            NativeReceiver::new(
                transport,
                session("receiver-session"),
                policy,
                FakeBackend {
                    calls: receiver_calls,
                },
            )?
            .serve(Duration::from_secs(5))
            .await
        });

        let transport = PairingClient::connect(
            address,
            "localhost",
            &certificate,
            PairingCode::parse("123456").unwrap(),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        let mut sender = NativeSender::new(transport, session("sender-session")).unwrap();
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
                Duration::from_secs(2),
            )
            .await
            .unwrap();
        sender.start(Duration::from_secs(2)).await.unwrap();
        assert_eq!(
            sender.query_status(Duration::from_secs(2)).await.unwrap(),
            SessionState::Streaming
        );
        let report = sender.stop(Duration::from_secs(2)).await.unwrap();
        assert_eq!(report.state, SessionState::Closed);
        let receiver_report = receiver_task.await.unwrap().unwrap();
        assert_eq!(receiver_report.state, SessionState::Closed);
        assert_eq!(&*calls.lock().unwrap(), &["prepare", "start", "reset"]);
        server.close();
    }

    #[test]
    fn session_offer_rejects_unsupported_codec_and_range() {
        let offer = SessionOffer {
            video: Some(VideoConfig {
                codec: MediaCodec::H265,
                width: 1280,
                height: 720,
                frame_rate_numerator: 30,
                frame_rate_denominator: 1,
            }),
            audio: None,
            latency_ms: 120,
        };
        assert_eq!(offer.validate().unwrap_err().kind(), ErrorKind::Unsupported);
        assert!(
            SessionOffer {
                video: None,
                audio: None,
                latency_ms: 120,
            }
            .validate()
            .is_err()
        );
    }
}
