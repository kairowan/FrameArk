//! Output-acknowledgment fixtures, not decoder or device interoperability proof.

use super::*;
use crate::tests::{renderer_description, soap_request};
use std::sync::Mutex;

#[test]
fn accepted_split_request_gets_configured_timeout_not_scheduler_remainder() {
    let mut server = MediaRendererTcpServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        MediaRendererHttpService::new(renderer_description()).unwrap(),
        Duration::from_secs(1),
    )
    .unwrap();
    let mut socket = TcpStream::connect(server.local_addr().unwrap()).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    socket
        .write_all(b"GET /device.xml HTTP/1.1\r\nHost: ")
        .unwrap();
    let worker = std::thread::spawn(move || server.serve_for(Duration::from_millis(5), 1).unwrap());
    std::thread::sleep(Duration::from_millis(30));
    socket.write_all(b"fixture\r\n\r\n").unwrap();
    assert_eq!(read_http_response(&mut socket).unwrap().status, 200);
    assert_eq!(worker.join().unwrap(), 1);
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Seen {
    Uri(String, String),
    Play,
    Pause,
    Stop,
    Seek(u64),
    Volume(u8),
}

#[derive(Default)]
struct Probe {
    accepted: Vec<Seen>,
    reject_next: Option<PlaybackBackendError>,
    reject_play: Option<PlaybackBackendError>,
    shutdowns: usize,
    feedback: Option<PlaybackFeedback>,
}

struct Backend(Arc<Mutex<Probe>>);

impl MediaPlaybackBackend for Backend {
    fn feedback(&mut self) -> Option<PlaybackFeedback> {
        self.0.lock().unwrap().feedback.take()
    }
    fn apply(&mut self, command: PlaybackCommand<'_>) -> Result<(), PlaybackBackendError> {
        let mut probe = self.0.lock().unwrap();
        if let Some(error) = probe.reject_next.take() {
            return Err(error);
        }
        if matches!(command, PlaybackCommand::Play)
            && let Some(error) = probe.reject_play.take()
        {
            return Err(error);
        }
        probe.accepted.push(match command {
            PlaybackCommand::SetUri { uri, metadata } => Seen::Uri(uri.into(), metadata.into()),
            PlaybackCommand::Play => Seen::Play,
            PlaybackCommand::Pause => Seen::Pause,
            PlaybackCommand::Stop => Seen::Stop,
            PlaybackCommand::Seek { position_seconds } => Seen::Seek(position_seconds),
            PlaybackCommand::SetVolume { volume } => Seen::Volume(volume),
        });
        Ok(())
    }

    fn shutdown(&mut self) {
        self.0.lock().unwrap().shutdowns += 1;
    }
}

#[test]
fn actual_progress_end_and_failure_reconcile_without_position_events() {
    let (mut service, probe) = bound();
    assert_eq!(service.handle(&load()).status, 200);
    assert_eq!(
        service.handle(&action("Play", "<Speed>1</Speed>")).status,
        200
    );
    subscribe(&mut service, "/upnp/event/avtransport");
    probe.lock().unwrap().feedback = Some(PlaybackFeedback::Progress {
        position_seconds: 3,
        duration_seconds: Some(59),
    });
    service.poll_output();
    assert_eq!(service.state().duration_seconds, Some(59));
    assert_eq!(service.state().position_seconds, 3);
    assert_eq!(service.drain_events().len(), 1);
    probe.lock().unwrap().feedback = Some(PlaybackFeedback::Progress {
        position_seconds: 4,
        duration_seconds: Some(59),
    });
    service.poll_output();
    assert!(service.drain_events().is_empty());
    probe.lock().unwrap().feedback = Some(PlaybackFeedback::Ended);
    service.poll_output();
    assert_eq!(service.state().transport_state, TransportState::Stopped);
    assert_eq!(service.state().position_seconds, 59);
    assert_eq!(service.drain_events().len(), 1);
    probe.lock().unwrap().feedback = Some(PlaybackFeedback::Failed);
    service.poll_output();
    assert!(service.state().current_uri.is_empty());
    assert_eq!(service.playback_report().fatal_failures, 1);
    assert_eq!(probe.lock().unwrap().shutdowns, 1);
    drop(service);
    assert_eq!(probe.lock().unwrap().shutdowns, 1);
}

#[test]
fn invalid_decoder_position_detaches_output() {
    let (mut service, probe) = bound();
    service.handle(&load());
    probe.lock().unwrap().feedback = Some(PlaybackFeedback::Progress {
        position_seconds: 61,
        duration_seconds: Some(60),
    });
    service.poll_output();
    assert!(service.state().current_uri.is_empty());
    assert_eq!(probe.lock().unwrap().shutdowns, 1);
}

#[test]
fn transport_peer_gate_rejects_before_backend_or_resource_access() {
    let (service, probe) = bound();
    let mut server = MediaRendererTcpServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        service,
        Duration::from_secs(1),
    )
    .unwrap();
    server.set_request_filter(|peer, request| {
        assert!(peer.ip().is_loopback());
        request.target == "/device.xml"
    });
    let address = server.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        assert_eq!(server.serve_for(Duration::from_secs(2), 2).unwrap(), 2);
    });
    for (request, status) in [
        (load(), 403),
        (
            HttpRequest::parse(b"GET /device.xml HTTP/1.1\r\nHost: fixture\r\n\r\n").unwrap(),
            200,
        ),
    ] {
        let body = request.body;
        let mut bytes = format!("{} {} HTTP/1.1\r\n", request.method, request.target);
        for (key, value) in request.headers {
            bytes.push_str(&format!("{key}: {value}\r\n"));
        }
        bytes.push_str("\r\n");
        let mut socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket.write_all(bytes.as_bytes()).unwrap();
        socket.write_all(&body).unwrap();
        assert_eq!(read_http_response(&mut socket).unwrap().status, status);
    }
    worker.join().unwrap();
    assert!(probe.lock().unwrap().accepted.is_empty());
}

fn bound() -> (MediaRendererHttpService, Arc<Mutex<Probe>>) {
    let probe = Arc::new(Mutex::new(Probe::default()));
    let service =
        MediaRendererHttpService::with_backend(renderer_description(), Backend(Arc::clone(&probe)))
            .unwrap();
    (service, probe)
}

#[test]
fn replacement_preserves_transport_and_failed_resume_invalidates_new_output() {
    for state in [
        TransportState::Stopped,
        TransportState::Playing,
        TransportState::PausedPlayback,
    ] {
        let (mut service, probe) = bound();
        assert_eq!(service.handle(&load()).status, 200);
        if state != TransportState::Stopped {
            assert_eq!(
                service.handle(&action("Play", "<Speed>1</Speed>")).status,
                200
            );
        }
        if state == TransportState::PausedPlayback {
            assert_eq!(service.handle(&action("Pause", "")).status, 200);
        }
        assert_eq!(service.handle(&load()).status, 200);
        assert_eq!(service.state().transport_state, state);
        if state == TransportState::Playing {
            assert_eq!(probe.lock().unwrap().accepted.last(), Some(&Seen::Play));
        }
    }
    for error in [PlaybackBackendError::Rejected, PlaybackBackendError::Failed] {
        let (mut service, probe) = bound();
        service.handle(&load());
        service.handle(&action("Play", "<Speed>1</Speed>"));
        subscribe(&mut service, "/upnp/event/avtransport");
        probe.lock().unwrap().reject_play = Some(error);
        assert_fault(service.handle(&load()), 501);
        assert!(service.state().current_uri.is_empty());
        assert!(service.state().transport_error);
        assert_eq!(service.playback_report().fatal_failures, 1);
        assert_eq!(probe.lock().unwrap().shutdowns, 1);
        let events = service.drain_events();
        assert_eq!(events.len(), 1);
        assert!(events[0].body.contains("ERROR_OCCURRED"));
    }
}

fn action(name: &str, args: &str) -> HttpRequest {
    let (path, service) =
        if name.ends_with("Volume") || name.ends_with("Preset") || name == "ListPresets" {
            ("/upnp/control/renderingcontrol", RENDERING_CONTROL_SERVICE)
        } else {
            ("/upnp/control/avtransport", AVTRANSPORT_SERVICE)
        };
    soap_request(
        path,
        service,
        name,
        &format!("<InstanceID>0</InstanceID>{args}"),
    )
}

#[test]
fn mandatory_presets_and_track_seek_are_output_acknowledged() {
    let preset = action("SelectPreset", "<PresetName>FactoryDefaults</PresetName>");
    let seek = action("Seek", "<Unit>TRACK_NR</Unit><Target>1</Target>");
    let mut unbound = MediaRendererHttpService::new(renderer_description()).unwrap();
    assert_fault(unbound.handle(&preset), 501);
    let (mut service, probe) = bound();
    assert_eq!(service.handle(&load()).status, 200);
    assert_eq!(
        service
            .handle(&action(
                "SetVolume",
                "<Channel>Master</Channel><DesiredVolume>27</DesiredVolume>"
            ))
            .status,
        200
    );
    assert_eq!(
        service
            .handle(&action(
                "Seek",
                "<Unit>REL_TIME</Unit><Target>00:00:19</Target>"
            ))
            .status,
        200
    );
    subscribe(&mut service, "/upnp/event/avtransport");
    subscribe(&mut service, "/upnp/event/renderingcontrol");
    let before = service.state().clone();
    for request in [&preset, &seek] {
        for error in [
            PlaybackBackendError::Rejected,
            PlaybackBackendError::Unsupported,
            PlaybackBackendError::Unavailable,
        ] {
            probe.lock().unwrap().reject_next = Some(error);
            assert_fault(service.handle(request), 501);
            assert_eq!(service.state(), &before);
            assert!(service.drain_events().is_empty());
        }
    }
    for name in ["Next", "Previous"] {
        assert_fault(service.handle(&action(name, "")), 711);
    }
    for target in ["0", "2", "4294967296", "-1", "text"] {
        assert_fault(
            service.handle(&action(
                "Seek",
                &format!("<Unit>TRACK_NR</Unit><Target>{target}</Target>"),
            )),
            711,
        );
    }
    assert_eq!(probe.lock().unwrap().accepted.len(), 3);
    assert_eq!(service.state(), &before);
    assert_eq!(service.handle(&preset).status, 200);
    assert_eq!(service.state().volume, 50);
    let event = service.drain_events().remove(0);
    assert_eq!(event.service_type, RENDERING_CONTROL_SERVICE);
    assert!(event.body.contains("FactoryDefaults"));
    assert!(event.body.contains("val=&quot;50&quot;"));
    assert_eq!(service.handle(&seek).status, 200);
    assert_eq!(service.state().position_seconds, 0);
    assert_eq!(
        service.drain_events().remove(0).service_type,
        AVTRANSPORT_SERVICE
    );
    assert_eq!(
        probe.lock().unwrap().accepted[3..],
        [Seen::Volume(50), Seen::Seek(0)]
    );
    probe.lock().unwrap().feedback = Some(PlaybackFeedback::Failed);
    service.poll_output();
    let info = service.handle(&action("GetTransportInfo", ""));
    assert!(
        String::from_utf8(info.body)
            .unwrap()
            .contains("<CurrentTransportStatus>ERROR_OCCURRED</CurrentTransportStatus>")
    );
    assert!(
        service
            .drain_events()
            .remove(0)
            .body
            .contains("ERROR_OCCURRED")
    );
}

fn load() -> HttpRequest {
    action(
        "SetAVTransportURI",
        "<CurrentURI>http://192.168.1.2/media.mp4?token=fixture</CurrentURI><CurrentURIMetaData>&lt;res duration=&quot;00:01:00&quot;&gt;fixture&lt;/res&gt;</CurrentURIMetaData>",
    )
}

fn mutations() -> Vec<HttpRequest> {
    vec![
        load(),
        action("Play", "<Speed>1</Speed>"),
        action("Pause", ""),
        action("Seek", "<Unit>REL_TIME</Unit><Target>00:00:12</Target>"),
        action(
            "SetVolume",
            "<Channel>Master</Channel><DesiredVolume>73</DesiredVolume>",
        ),
        action("Stop", ""),
    ]
}

fn subscribe(service: &mut MediaRendererHttpService, path: &str) {
    let request = HttpRequest::parse(format!(
        "SUBSCRIBE {path} HTTP/1.1\r\nHost: fixture\r\nNT: upnp:event\r\nCALLBACK: <http://127.0.0.1:9000/events>\r\nTIMEOUT: Second-120\r\n\r\n"
    ).as_bytes()).unwrap();
    assert_eq!(service.handle(&request).status, 200);
    assert_eq!(service.drain_events().len(), 1);
}

fn assert_fault(response: HttpResponse, code: u16) {
    assert_eq!(response.status, 500);
    let body = String::from_utf8(response.body).unwrap();
    assert!(body.contains(&format!("<errorCode>{code}</errorCode>")));
    assert!(!body.contains("token=fixture"));
}

#[test]
fn unbound_service_never_claims_output_success() {
    let mut service = MediaRendererHttpService::new(renderer_description()).unwrap();
    assert_fault(service.handle(&load()), 501);
    assert_fault(service.handle(&mutations()[4]), 501);
    assert_eq!(service.state(), &MediaRendererState::default());
    assert_eq!(service.playback_report().attempted, 2);
    assert_eq!(service.playback_report().rejected, 2);
    assert_eq!(service.playback_report().accepted, 0);
    assert_eq!(
        service.playback_report().last_error,
        Some(PlaybackBackendError::Unavailable)
    );
    let info = service.handle(&action("GetTransportInfo", ""));
    assert_eq!(info.status, 200);
    assert!(
        String::from_utf8(info.body)
            .unwrap()
            .contains("NO_MEDIA_PRESENT")
    );
    let request = HttpRequest::parse(b"GET /device.xml HTTP/1.1\r\nHost: fixture\r\n\r\n").unwrap();
    assert_eq!(service.handle(&request).status, 200);
    assert_eq!(service.playback_report().attempted, 2);
}

#[test]
fn accepted_commands_dispatch_once_before_committing_and_eventing() {
    let (mut service, probe) = bound();
    subscribe(&mut service, "/upnp/event/avtransport");
    subscribe(&mut service, "/upnp/event/renderingcontrol");
    for request in mutations() {
        assert_eq!(service.handle(&request).status, 200);
        assert_eq!(service.drain_events().len(), 1);
    }
    assert_eq!(service.state().transport_state, TransportState::Stopped);
    assert_eq!(service.state().position_seconds, 12);
    assert_eq!(service.state().duration_seconds, Some(60));
    assert_eq!(service.state().volume, 73);
    assert_eq!(
        probe.lock().unwrap().accepted,
        vec![
            Seen::Uri(
                "http://192.168.1.2/media.mp4?token=fixture".into(),
                "<res duration=\"00:01:00\">fixture</res>".into()
            ),
            Seen::Play,
            Seen::Pause,
            Seen::Seek(12),
            Seen::Volume(73),
            Seen::Stop,
        ]
    );
    assert_eq!(
        service.playback_report(),
        PlaybackDispatchReport {
            attempted: 6,
            accepted: 6,
            ..PlaybackDispatchReport::default()
        }
    );
    let report = service.playback_report();
    for name in ["GetTransportInfo", "GetPositionInfo", "GetMediaInfo"] {
        assert_eq!(service.handle(&action(name, "")).status, 200);
    }
    assert_eq!(
        service
            .handle(&action("GetVolume", "<Channel>Master</Channel>"))
            .status,
        200
    );
    assert_eq!(service.playback_report(), report);
    drop(service);
    assert_eq!(probe.lock().unwrap().shutdowns, 1);
}

#[test]
fn every_nonfatal_rejection_preserves_state_and_gena_sequence() {
    for error in [
        PlaybackBackendError::Unavailable,
        PlaybackBackendError::Unsupported,
        PlaybackBackendError::Rejected,
    ] {
        for request in mutations() {
            let (mut service, probe) = bound();
            assert_eq!(service.handle(&load()).status, 200);
            assert_eq!(
                service.handle(&action("Play", "<Speed>1</Speed>")).status,
                200
            );
            subscribe(&mut service, "/upnp/event/avtransport");
            subscribe(&mut service, "/upnp/event/renderingcontrol");
            let before = service.state().clone();
            probe.lock().unwrap().reject_next = Some(error);
            assert_fault(service.handle(&request), 501);
            assert_eq!(service.state(), &before);
            assert!(service.drain_events().is_empty());
            assert_eq!(probe.lock().unwrap().accepted.len(), 2);
            assert_eq!(probe.lock().unwrap().shutdowns, 0);
            assert_eq!(service.playback_report().last_error, Some(error));
            assert_eq!(service.handle(&action("Pause", "")).status, 200);
            let events = service.drain_events();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].sequence, 1);
            assert_eq!(service.playback_report().last_error, None);
            assert_eq!(service.playback_report().rejected, 1);
        }
    }
}

#[test]
fn invalid_commands_never_reach_output() {
    let (mut service, probe) = bound();
    assert_eq!(service.handle(&load()).status, 200);
    let before = service.state().clone();
    // Network parsing already rejects this body size. Exercise the public
    // parsed-request API without asking the fixture parser to accept it.
    let mut oversized = load();
    oversized.body = format!(
        "<Envelope><Body><SetAVTransportURI><InstanceID>0</InstanceID><CurrentURI>http://fixture/video</CurrentURI><CurrentURIMetaData>{}</CurrentURIMetaData></SetAVTransportURI></Body></Envelope>",
        "x".repeat(MAX_SOAP_ARGUMENT_BYTES + 1)
    ).into_bytes();
    let requests = vec![
        action(
            "SetAVTransportURI",
            "<CurrentURI>file:///private</CurrentURI>",
        ),
        oversized,
        action("Play", "<Speed>2</Speed>"),
        action("Seek", "<Unit>ABS_TIME</Unit><Target>00:00:12</Target>"),
        action("Seek", "<Unit>REL_TIME</Unit><Target>00:01:01</Target>"),
        action("Seek", "<Unit>REL_TIME</Unit><Target>bad</Target>"),
        action(
            "SetVolume",
            "<Channel>Other</Channel><DesiredVolume>20</DesiredVolume>",
        ),
        action(
            "SetVolume",
            "<Channel>Master</Channel><DesiredVolume>101</DesiredVolume>",
        ),
        action(
            "SetVolume",
            "<Channel>Master</Channel><DesiredVolume>255</DesiredVolume>",
        ),
        action(
            "SetVolume",
            "<Channel>Master</Channel><DesiredVolume>256</DesiredVolume>",
        ),
        action(
            "SetVolume",
            "<Channel>Master</Channel><DesiredVolume>-1</DesiredVolume>",
        ),
        soap_request(
            "/upnp/control/avtransport",
            AVTRANSPORT_SERVICE,
            "Play",
            "<InstanceID>1</InstanceID>",
        ),
        soap_request(
            "/upnp/control/renderingcontrol",
            AVTRANSPORT_SERVICE,
            "Play",
            "<InstanceID>0</InstanceID>",
        ),
    ];
    for request in requests {
        assert_eq!(service.handle(&request).status, 500);
        assert_eq!(service.state(), &before);
    }
    assert_eq!(probe.lock().unwrap().accepted.len(), 1);
    assert_eq!(service.playback_report().attempted, 1);
    for volume in [0, 100] {
        let request = action(
            "SetVolume",
            &format!("<Channel>Master</Channel><DesiredVolume>{volume}</DesiredVolume>"),
        );
        assert_eq!(service.handle(&request).status, 200);
        assert_eq!(service.state().volume, volume);
    }
}

#[test]
fn fatal_failure_clears_media_detaches_backend_and_events_only_terminal_state() {
    for request in mutations() {
        let (mut service, probe) = bound();
        assert_eq!(service.handle(&load()).status, 200);
        assert_eq!(
            service.handle(&action("Play", "<Speed>1</Speed>")).status,
            200
        );
        subscribe(&mut service, "/upnp/event/avtransport");
        subscribe(&mut service, "/upnp/event/renderingcontrol");
        probe.lock().unwrap().reject_next = Some(PlaybackBackendError::Failed);
        assert_fault(service.handle(&request), 501);
        assert_eq!(
            service.state(),
            &MediaRendererState {
                transport_error: true,
                ..MediaRendererState::default()
            }
        );
        let events = service.drain_events();
        assert_eq!(events.len(), 1);
        assert!(events[0].body.contains("NO_MEDIA_PRESENT"));
        assert!(events[0].body.contains("ERROR_OCCURRED"));
        assert!(!events[0].body.contains("token=fixture"));
        assert_eq!(service.playback_report().fatal_failures, 1);
        assert_eq!(
            service.playback_report().last_error,
            Some(PlaybackBackendError::Failed)
        );
        assert_fault(service.handle(&load()), 501);
        assert!(service.state().transport_error);
        assert_eq!(
            service.playback_report().last_error,
            Some(PlaybackBackendError::Unavailable)
        );
        assert_eq!(probe.lock().unwrap().accepted.len(), 2);
        drop(service);
        assert_eq!(probe.lock().unwrap().shutdowns, 1);
    }
}

#[test]
fn time_bounds_are_checked_before_output_dispatch() {
    let (mut service, probe) = bound();
    assert_eq!(service.handle(&load()).status, 200);
    let before = service.state().clone();
    for target in [
        format!("{}:00:00", u64::MAX),
        format!("{}:59:59", u64::MAX / 3600),
    ] {
        let request = action(
            "Seek",
            &format!("<Unit>REL_TIME</Unit><Target>{target}</Target>"),
        );
        assert_fault(service.handle(&request), 711);
        assert_eq!(service.state(), &before);
    }
    assert_eq!(probe.lock().unwrap().accepted.len(), 1);
    // Unsupported metadata duration remains unknown, never a wrapped value.
    let request = action(
        "SetAVTransportURI",
        &format!(
            "<CurrentURI>http://192.168.1.2/next.mp4</CurrentURI><CurrentURIMetaData>&lt;res duration=&quot;{}:00:00&quot;&gt;fixture&lt;/res&gt;</CurrentURIMetaData>",
            u64::MAX
        ),
    );
    assert_eq!(service.handle(&request).status, 200);
    assert_eq!(service.state().duration_seconds, None);
    assert_eq!(
        parse_upnp_time(&format_upnp_time(u64::MAX)).unwrap(),
        u64::MAX
    );
}

#[test]
fn rejected_description_releases_backend_and_counters_saturate() {
    let probe = Arc::new(Mutex::new(Probe::default()));
    let mut description = renderer_description();
    description.friendly_name = "x".repeat(MAX_FIELD_BYTES + 1);
    assert!(
        MediaRendererHttpService::with_backend(description, Backend(Arc::clone(&probe))).is_err()
    );
    assert_eq!(probe.lock().unwrap().shutdowns, 1);
    let (mut service, probe) = bound();
    service.playback_report = PlaybackDispatchReport {
        attempted: u64::MAX,
        accepted: u64::MAX,
        rejected: u64::MAX,
        fatal_failures: u64::MAX,
        last_error: None,
    };
    assert_eq!(service.handle(&load()).status, 200);
    probe.lock().unwrap().reject_next = Some(PlaybackBackendError::Rejected);
    assert_fault(service.handle(&load()), 501);
    probe.lock().unwrap().reject_next = Some(PlaybackBackendError::Failed);
    assert_fault(service.handle(&load()), 501);
    let report = service.playback_report();
    assert_eq!(
        [
            report.attempted,
            report.accepted,
            report.rejected,
            report.fatal_failures
        ],
        [u64::MAX; 4]
    );
}

#[test]
fn real_tcp_dispatches_output_commands_and_returns_backend_rejection() {
    let (service, probe) = bound();
    let mut server = MediaRendererTcpServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        service,
        Duration::from_secs(1),
    )
    .unwrap();
    let address = server.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        assert_eq!(server.serve_for(Duration::from_secs(5), 8).unwrap(), 8);
        server.service().playback_report()
    });
    let exchange = |request: HttpRequest| {
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut wire = format!("{} {} HTTP/1.1\r\n", request.method, request.target);
        for (name, value) in request.headers {
            wire.push_str(&format!("{name}: {value}\r\n"));
        }
        wire.push_str("\r\n");
        stream.write_all(wire.as_bytes()).unwrap();
        stream.write_all(&request.body).unwrap();
        read_http_response(&mut stream).unwrap()
    };
    for request in mutations() {
        assert_eq!(exchange(request).status, 200);
    }
    probe.lock().unwrap().reject_next = Some(PlaybackBackendError::Unsupported);
    assert_fault(exchange(action("Play", "<Speed>1</Speed>")), 501);
    let response = exchange(action("GetTransportInfo", ""));
    assert!(
        String::from_utf8(response.body)
            .unwrap()
            .contains("STOPPED")
    );
    let report = worker.join().unwrap();
    assert_eq!(report.accepted, 6);
    assert_eq!(report.rejected, 1);
    assert_eq!(probe.lock().unwrap().accepted.len(), 6);
    assert_eq!(probe.lock().unwrap().shutdowns, 1);
}

#[test]
fn nonblocking_accepted_socket_waits_for_split_request() {
    let (service, probe) = bound();
    let mut server = MediaRendererTcpServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        service,
        Duration::from_secs(1),
    )
    .unwrap();
    let address = server.local_addr().unwrap();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let (stream, peer) = server.listener.accept().unwrap();
        // Model platforms that inherit the non-blocking listener mode. Force
        // it here so Windows/Linux can reproduce the macOS CI failure too.
        stream.set_nonblocking(true).unwrap();
        ready_tx.send(()).unwrap();
        server.serve_connection(stream, peer)
    });
    let request = load();
    let mut wire = format!("{} {} HTTP/1.1\r\n", request.method, request.target);
    for (name, value) in request.headers {
        wire.push_str(&format!("{name}: {value}\r\n"));
    }
    wire.push_str("\r\n");
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    ready_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    let header_write = stream.write_all(wire.as_bytes());
    std::thread::sleep(Duration::from_millis(20));
    let body_write = stream.write_all(&request.body);
    let response = read_http_response(&mut stream);
    let served = worker.join().unwrap();
    assert!(
        served.is_ok(),
        "accepted stream must be blocking: {served:?}"
    );
    header_write.unwrap();
    body_write.unwrap();
    assert_eq!(response.unwrap().status, 200);
    assert_eq!(probe.lock().unwrap().accepted.len(), 1);
    assert_eq!(probe.lock().unwrap().shutdowns, 1);
}
