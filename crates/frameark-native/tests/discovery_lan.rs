//! Opt-in real multicast and private IPv4 QUIC test, never a physical-device gate.
use frameark_core::{DeviceId, MediaCodec, Session, SessionId, SessionState, VideoConfig};
use frameark_discovery::{DiscoveryConfig, DiscoveryEvent, MdnsDiscovery, PublishedService};
use frameark_native::discovery::{advertise_certificate, candidate_tickets};
use frameark_native::{NativeReceiver, NativeSender, ReceiverPolicy, SessionBackend, SessionOffer};
use frameark_transport::{PairingClient, PairingCode, PairingServer};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Backend(Arc<Mutex<Vec<&'static str>>>);
impl SessionBackend for Backend {
    fn prepare(&mut self, _: SessionOffer) -> frameark_core::Result<()> {
        self.0.lock().unwrap().push("prepare");
        Ok(())
    }
    fn start(&mut self) -> frameark_core::Result<()> {
        self.0.lock().unwrap().push("start");
        Ok(())
    }
    fn reset(&mut self) -> frameark_core::Result<()> {
        self.0.lock().unwrap().push("reset");
        Ok(())
    }
}
fn session(name: &str) -> Session {
    Session::new(
        SessionId::try_from(name).unwrap(),
        DeviceId::try_from("discovery-test").unwrap(),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires an operator-authorized multicast/private IPv4 LAN"]
async fn multicast_candidate_pairs_negotiates_and_stops() {
    let server = Arc::new(
        PairingServer::bind(
            "0.0.0.0:0".parse().unwrap(),
            "frameark.local",
            PairingCode::parse("123456").unwrap(),
        )
        .unwrap(),
    );
    let discovery = MdnsDiscovery::new(DiscoveryConfig::all_interfaces()).unwrap();
    let name = format!("FrameArk-test-{}", std::process::id());
    let service = advertise_certificate(
        PublishedService::new(
            &name,
            format!("frameark-test-{}.local.", std::process::id()),
            server.local_addr().unwrap().port(),
        )
        .unwrap()
        .with_property("proto", "fanp1")
        .unwrap()
        .with_property("ver", "0.1")
        .unwrap()
        .with_property("tls", "pin")
        .unwrap(),
        server.certificate_der(),
    )
    .unwrap();
    let registration = discovery.publish(&service).unwrap();
    let fullname = registration.fullname().to_owned();
    // A separate daemon/browser verifies actual mDNS encoding and observation.
    let browser_daemon = MdnsDiscovery::new(DiscoveryConfig::all_interfaces()).unwrap();
    let browser = browser_daemon.browse().unwrap();
    let result = async {
        let ticket = tokio::task::spawn_blocking(move || {
            let deadline = Instant::now() + Duration::from_secs(12);
            while Instant::now() < deadline {
                if let Some(DiscoveryEvent::ServiceResolved(service)) =
                    browser.recv_timeout(Duration::from_millis(100)).ok()?
                    && service.fullname == fullname
                    && let Some(ticket) = candidate_tickets(&service).into_iter().next()
                {
                    return Some(ticket);
                }
            }
            None
        })
        .await
        .map_err(|_| "browse worker")?
        .ok_or("no private IPv4 mDNS candidate")?;
        // This test compares to its known producer. Production requires user comparison.
        if ticket.certificate() != server.certificate_der() {
            return Err("certificate mismatch");
        }
        let calls = Arc::new(Mutex::new(Vec::new()));
        let receiver_calls = calls.clone();
        let accepting = server.clone();
        let receiver = tokio::spawn(async move {
            let transport = accepting
                .accept_pairing(Duration::from_secs(5))
                .await
                .map_err(|_| "accept")?;
            NativeReceiver::new(
                transport,
                session("rx-discovery"),
                ReceiverPolicy {
                    video: true,
                    opus: true,
                    aac: true,
                    max_width: 1920,
                    max_height: 1080,
                    max_fps: 60,
                    max_channels: 2,
                },
                Backend(receiver_calls),
            )
            .map_err(|_| "receiver")?
            .serve(Duration::from_secs(5))
            .await
            .map_err(|_| "serve")
        });
        let transport = PairingClient::connect(
            ticket.address().into(),
            "frameark.local",
            ticket.certificate(),
            PairingCode::parse("123456").unwrap(),
            Duration::from_secs(5),
        )
        .await
        .map_err(|_| "connect")?;
        let mut sender =
            NativeSender::new(transport, session("tx-discovery")).map_err(|_| "sender")?;
        sender
            .offer(
                SessionOffer {
                    video: Some(VideoConfig {
                        codec: MediaCodec::H264,
                        width: 1920,
                        height: 1080,
                        frame_rate_numerator: 30,
                        frame_rate_denominator: 1,
                    }),
                    audio: None,
                    latency_ms: 120,
                },
                Duration::from_secs(2),
            )
            .await
            .map_err(|_| "offer")?;
        sender
            .start(Duration::from_secs(2))
            .await
            .map_err(|_| "start")?;
        if sender
            .query_status(Duration::from_secs(2))
            .await
            .map_err(|_| "status")?
            != SessionState::Streaming
        {
            return Err("state");
        }
        sender
            .stop(Duration::from_secs(2))
            .await
            .map_err(|_| "stop")?;
        receiver.await.map_err(|_| "receiver task")??;
        if *calls.lock().unwrap() != ["prepare", "start", "reset"] {
            return Err("cleanup");
        }
        Ok(())
    }
    .await;
    server.close();
    let unregistered = registration.unregister();
    let browser_stopped = browser_daemon.shutdown();
    let publisher_stopped = discovery.shutdown();
    result.unwrap();
    unregistered.unwrap();
    browser_stopped.unwrap();
    publisher_stopped.unwrap();
}
