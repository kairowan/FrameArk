//! Explicitly enabled LAN renderer host. Never starts with the Native listener.

use super::*;
use frameark_dlna::{
    AVTRANSPORT_SERVICE, CONNECTION_MANAGER_SERVICE, DeviceDescription, GenaCallbackClient,
    MediaFormatCapabilities, MediaRendererHttpService, MediaRendererTcpServer,
    RENDERING_CONTROL_SERVICE, ServiceDescription, SsdpAdvertisement, SsdpMessage, SsdpPublisher,
    callback_literal_address,
};
use jni::sys::jstring;
use std::net::UdpSocket;
use std::sync::atomic::AtomicU64;

#[derive(Default)]
struct Diagnostics {
    requests: AtomicU64,
    callbacks: AtomicU64,
    callback_failures: AtomicU64,
    playback: Mutex<frameark_dlna::PlaybackDispatchReport>,
}

fn increment(counter: &AtomicU64, value: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
        Some(old.saturating_add(value))
    });
}

#[derive(Default)]
struct RuntimeState {
    runtime: Option<Runtime>,
    stopping: bool,
}
static STATE: OnceLock<Mutex<RuntimeState>> = OnceLock::new();
fn state() -> &'static Mutex<RuntimeState> {
    STATE.get_or_init(|| Mutex::new(RuntimeState::default()))
}
pub(super) fn is_running() -> bool {
    state()
        .lock()
        .map_or(true, |state| state.stopping || state.runtime.is_some())
}

struct Runtime {
    stop: Arc<AtomicBool>,
    output: dlna_output::OutputBridge,
    http: JoinHandle<()>,
    discovery: Option<JoinHandle<()>>,
    address: SocketAddr,
    diagnostics: Arc<Diagnostics>,
}

fn description(identity: &str) -> Result<DeviceDescription, ()> {
    if !valid_identity_fingerprint(identity) {
        return Err(());
    }
    let identity = identity.replace(':', "");
    Ok(DeviceDescription {
        friendly_name: "FrameArk Receiver (Experimental DLNA)".into(),
        manufacturer: "FrameArk".into(),
        model_name: "FrameArk Android Receiver".into(),
        // UUID v8 derived from the installed public-key SHA-256 fingerprint.
        udn: format!(
            "uuid:{}-{}-8{}-a{}-{}",
            &identity[..8],
            &identity[8..12],
            &identity[13..16],
            &identity[17..20],
            &identity[20..32]
        ),
        services: [
            (AVTRANSPORT_SERVICE, "AVTransport", "avtransport"),
            (
                RENDERING_CONTROL_SERVICE,
                "RenderingControl",
                "renderingcontrol",
            ),
            (
                CONNECTION_MANAGER_SERVICE,
                "ConnectionManager",
                "connectionmanager",
            ),
        ]
        .into_iter()
        .map(|(service_type, id, path)| ServiceDescription {
            service_type: service_type.into(),
            service_id: format!("urn:upnp-org:serviceId:{id}"),
            control_url: format!("/upnp/control/{path}"),
            event_sub_url: format!("/upnp/event/{path}"),
            scpd_url: format!("/upnp/{path}.xml"),
        })
        .collect(),
    })
}

fn permitted(address: IpAddr, probe: bool) -> bool {
    match address {
        IpAddr::V4(ip) => ip.is_private() || (probe && ip.is_loopback()),
        _ => false,
    }
}

impl Runtime {
    fn start(bind: Ipv4Addr, device: DeviceId, identity: &str, probe: bool) -> Result<Self, ()> {
        if !permitted(IpAddr::V4(bind), probe) {
            return Err(());
        }
        let output = dlna_output::OutputBridge::new().map_err(|_| ())?;
        let origin = Arc::new(Mutex::new(IpAddr::V4(bind)));
        let description = description(identity)?;
        let backend =
            dlna_output::AndroidDlnaBackend::new(output.clone(), origin.clone(), device, probe);
        let mut service =
            MediaRendererHttpService::with_backend(description.clone(), backend).map_err(|_| ())?;
        service
            .set_capabilities(
                MediaFormatCapabilities::new(vec![
                    "http-get:*:video/mp4:*".into(),
                    "http-get:*:audio/mpeg:*".into(),
                    "http-get:*:audio/wav:*".into(),
                    "http-get:*:image/jpeg:*".into(),
                    "http-get:*:image/png:*".into(),
                ])
                .map_err(|_| ())?,
            )
            .map_err(|_| ())?;
        let mut server = MediaRendererTcpServer::bind(
            SocketAddr::new(bind.into(), 0),
            service,
            Duration::from_secs(1),
        )
        .map_err(|_| ())?;
        let address = server.local_addr().map_err(|_| ())?;
        server.set_request_filter(move |peer, request| {
            if !permitted(peer.ip(), probe) {
                return false;
            }
            if let Some(callback) = request.header("callback") {
                let Some(url) = callback
                    .strip_prefix('<')
                    .and_then(|value| value.strip_suffix('>'))
                else {
                    return false;
                };
                if url
                    .get(..7)
                    .is_none_or(|scheme| !scheme.eq_ignore_ascii_case("http://"))
                {
                    return false;
                }
                if callback_literal_address(url).ok() != Some(peer.ip()) {
                    return false;
                }
            }
            if let Ok(mut current) = origin.lock() {
                *current = peer.ip();
                true
            } else {
                false
            }
        });
        let stop = Arc::new(AtomicBool::new(false));
        // Bind discovery before publishing or handing the renderer to a worker.
        let discovery = if probe {
            None
        } else {
            Some(discovery_socket(bind, address, &description.udn)?)
        };
        let http_stop = stop.clone();
        let diagnostics = Arc::new(Diagnostics::default());
        let http_diagnostics = diagnostics.clone();
        dlna_output::install(Some(output.clone()));
        let http = thread::Builder::new()
            .name("frameark-dlna-http".into())
            .spawn(move || {
                let mut events = VecDeque::new();
                let callback =
                    GenaCallbackClient::new(Duration::from_millis(200)).expect("fixed timeout");
                let mut next_notification = Instant::now();
                while !http_stop.load(Ordering::Acquire) {
                    match server.serve_for(Duration::from_millis(250), 1) {
                        Ok(count) => increment(&http_diagnostics.requests, count as u64),
                        Err(_) => break,
                    }
                    server.service_mut().poll_output();
                    if let Ok(mut report) = http_diagnostics.playback.lock() {
                        *report = server.service().playback_report();
                    }
                    if events.is_empty() {
                        events.extend(server.service_mut().drain_events());
                    }
                    if Instant::now() >= next_notification
                        && let Some(event) = events.pop_front()
                    {
                        if server.service_mut().event_is_deliverable(&event)
                            && !http_stop.load(Ordering::Acquire)
                        {
                            // Creation required a literal address equal to its peer;
                            // no DNS, redirects or alternate destinations are used.
                            increment(&http_diagnostics.callbacks, 1);
                            if !callback
                                .send(&event)
                                .is_ok_and(|response| (200..300).contains(&response.status))
                            {
                                increment(&http_diagnostics.callback_failures, 1);
                            }
                        }
                        next_notification = Instant::now() + Duration::from_millis(200);
                    }
                }
                http_stop.store(true, Ordering::Release);
                // Drop releases output and sources before the worker terminates.
            })
            .map_err(|_| {
                output.close();
                dlna_output::install(None);
            })?;
        let discovery = if let Some((socket, publishers)) = discovery {
            let discovery_stop = stop.clone();
            match thread::Builder::new()
                .name("frameark-dlna-ssdp".into())
                .spawn(move || {
                    let mut bytes = [0_u8; 4096];
                    let mut announce_at = Instant::now();
                    let mut last_search = Instant::now() - Duration::from_secs(1);
                    while !discovery_stop.load(Ordering::Acquire) {
                        if Instant::now() >= announce_at {
                            for publisher in &publishers {
                                let _ = publisher.notify_multicast(true);
                            }
                            announce_at = Instant::now() + Duration::from_secs(120);
                        }
                        if let Ok((length, peer)) = socket.recv_from(&mut bytes)
                            && permitted(peer.ip(), false)
                            && Instant::now().duration_since(last_search)
                                >= Duration::from_millis(100)
                            && let Ok(request) = SsdpMessage::parse(&bytes[..length])
                        {
                            last_search = Instant::now();
                            for publisher in &publishers {
                                let _ = publisher.respond_to_search(&request, peer);
                            }
                        }
                    }
                    for publisher in &publishers {
                        let _ = publisher.notify_multicast(false);
                    }
                }) {
                Ok(worker) => Some(worker),
                Err(_) => {
                    stop.store(true, Ordering::Release);
                    output.close();
                    let _ = http.join();
                    dlna_output::install(None);
                    return Err(());
                }
            }
        } else {
            None
        };
        Ok(Self {
            stop,
            output,
            http,
            discovery,
            address,
            diagnostics,
        })
    }

    fn stop(self) {
        self.stop.store(true, Ordering::Release);
        self.output.close();
        let _ = self.http.join();
        if let Some(discovery) = self.discovery {
            let _ = discovery.join();
        }
        dlna_output::install(None);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_DlnaNative_diagnostic(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jstring {
    let result = || {
        let state = state().lock().ok()?;
        let runtime = state.runtime.as_ref()?;
        let counters = &runtime.diagnostics;
        let report = *counters.playback.lock().ok()?;
        let text = format!(
            "Rust DLNA: requests={}; callbacks={}; callback failures={}; output={report:?}; source={:?}; subtitles={:?}",
            counters.requests.load(Ordering::Relaxed),
            counters.callbacks.load(Ordering::Relaxed),
            counters.callback_failures.load(Ordering::Relaxed),
            runtime.output.source_report(),
            runtime
                .output
                .subtitle_report()
                .map(|r| (r.tracks, r.cues, r.selected))
        );
        env.new_string(text).ok().map(JString::into_raw)
    };
    result().unwrap_or(std::ptr::null_mut())
}

fn discovery_socket(
    bind: Ipv4Addr,
    http: SocketAddr,
    udn: &str,
) -> Result<(UdpSocket, Vec<SsdpPublisher>), ()> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 1900)).map_err(|_| ())?;
    socket
        .join_multicast_v4(&Ipv4Addr::new(239, 255, 255, 250), &bind)
        .map_err(|_| ())?;
    socket
        .set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|_| ())?;
    let mut publishers = Vec::new();
    for service in [
        "upnp:rootdevice",
        udn,
        "urn:schemas-upnp-org:device:MediaRenderer:1",
        AVTRANSPORT_SERVICE,
        RENDERING_CONTROL_SERVICE,
        CONNECTION_MANAGER_SERVICE,
    ] {
        let usn = if service == udn {
            udn.to_string()
        } else {
            format!("{udn}::{service}")
        };
        let advertisement = SsdpAdvertisement::new(
            service,
            usn,
            format!("http://{http}/device.xml"),
            300,
            "Android/unknown UPnP/1.0 FrameArk/0.1",
        )
        .map_err(|_| ())?;
        publishers.push(
            SsdpPublisher::bind(SocketAddr::new(bind.into(), 0), advertisement).map_err(|_| ())?,
        );
    }
    Ok((socket, publishers))
}

fn start(bind: &str, probe: bool) -> jint {
    let Ok(receiver) = receiver_state().lock() else {
        return -1;
    };
    if receiver.session.is_some() || receiver.stopping {
        return -2;
    }
    let Some(device) = receiver.device_id.clone() else {
        return -3;
    };
    let Some(identity) = receiver.identity_fingerprint.as_deref() else {
        return -3;
    };
    let Ok(mut state) = state().lock() else {
        return -1;
    };
    if state.stopping || state.runtime.is_some() {
        return -2;
    }
    let Ok(bind) = bind.parse::<Ipv4Addr>() else {
        return -4;
    };
    match Runtime::start(bind, device, identity, probe) {
        Ok(runtime) => {
            state.runtime = Some(runtime);
            0
        }
        Err(()) => -4,
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_DlnaNative_start(
    mut env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    bind: JString<'_>,
) -> jint {
    let Ok(bind) = env.get_string(&bind) else {
        return -4;
    };
    start(&bind.to_string_lossy(), false)
}

#[cfg(feature = "android-playback-probe")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_DlnaNative_startProbe(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jint {
    start("127.0.0.1", true)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_DlnaNative_stop(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) {
    let runtime = {
        let Ok(mut state) = state().lock() else {
            return;
        };
        if state.stopping {
            return;
        }
        state.stopping = true;
        state.runtime.take()
    };
    if let Some(runtime) = runtime {
        runtime.stop();
    }
    if let Ok(mut state) = state().lock() {
        state.stopping = false;
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_DlnaNative_descriptionUrl(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jstring {
    let result = || {
        let state = state().lock().ok()?;
        let runtime = state.runtime.as_ref()?;
        if runtime.stop.load(Ordering::Acquire) {
            return None;
        }
        let address = runtime.address;
        env.new_string(format!("http://{address}/device.xml"))
            .ok()
            .map(JString::into_raw)
    };
    result().unwrap_or(std::ptr::null_mut())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_uses_existing_colon_fingerprint_and_uuid_device_type() {
        let fingerprint = ["ab"; 32].join(":");
        let first = description(&fingerprint).unwrap();
        assert_eq!(first.udn, "uuid:abababab-abab-8bab-abab-abababababab");
        assert_eq!(first.udn, description(&fingerprint).unwrap().udn);
        assert!(
            first
                .to_xml()
                .unwrap()
                .contains("<deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>")
        );
        assert!(description("not-a-fingerprint").is_err());
    }

    #[test]
    fn normal_listener_rejects_public_loopback_and_ipv6_addresses() {
        for address in [
            "127.0.0.1",
            "0.0.0.0",
            "169.254.1.1",
            "8.8.8.8",
            "::1",
            "fc00::1",
        ] {
            assert!(!permitted(address.parse().unwrap(), false));
        }
        assert!(permitted("127.0.0.1".parse().unwrap(), true));
        assert!(permitted("192.168.1.2".parse().unwrap(), false));
    }

    #[test]
    fn real_loopback_host_starts_serves_description_and_closes_port() {
        use std::io::{Read, Write};
        let runtime = Runtime::start(
            Ipv4Addr::LOCALHOST,
            DeviceId::try_from("fixture-dlna".to_string()).unwrap(),
            &["ab"; 32].join(":"),
            true,
        )
        .unwrap();
        let address = runtime.address;
        let mut socket = std::net::TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket
            .write_all(b"GET /device.xml HTTP/1.1\r\nHost: fixture\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"));
        assert!(response.contains("MediaRenderer:1"));
        let body = "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><u:GetProtocolInfo xmlns:u=\"urn:schemas-upnp-org:service:ConnectionManager:1\"/></s:Body></s:Envelope>";
        let mut protocol = std::net::TcpStream::connect(address).unwrap();
        protocol
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        protocol.write_all(format!("POST /upnp/control/connectionmanager HTTP/1.1\r\nHost: fixture\r\nSOAPACTION: \"urn:schemas-upnp-org:service:ConnectionManager:1#GetProtocolInfo\"\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).unwrap();
        let mut capabilities = String::new();
        protocol.read_to_string(&mut capabilities).unwrap();
        assert!(capabilities.starts_with("HTTP/1.1 200"));
        for mime in [
            "video/mp4",
            "audio/mpeg",
            "audio/wav",
            "image/jpeg",
            "image/png",
        ] {
            assert!(capabilities.contains(&format!("http-get:*:{mime}:*")));
        }
        for callback in [
            "http://192.168.1.2/cb",
            "http://localhost/cb",
            "https://127.0.0.1/cb",
        ] {
            let mut socket = std::net::TcpStream::connect(address).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            socket.write_all(format!("SUBSCRIBE /upnp/event/avtransport HTTP/1.1\r\nHost: fixture\r\nNT: upnp:event\r\nCALLBACK: <{callback}>\r\n\r\n").as_bytes()).unwrap();
            let mut response = String::new();
            socket.read_to_string(&mut response).unwrap();
            assert!(response.starts_with("HTTP/1.1 403"));
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let callback = listener.local_addr().unwrap();
        let mut subscriber = std::net::TcpStream::connect(address).unwrap();
        subscriber
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        subscriber.write_all(format!("SUBSCRIBE /upnp/event/avtransport HTTP/1.1\r\nHost: fixture\r\nNT: upnp:event\r\nCALLBACK: <http://{callback}/cb>\r\n\r\n").as_bytes()).unwrap();
        let mut subscription = String::new();
        subscriber.read_to_string(&mut subscription).unwrap();
        assert!(subscription.starts_with("HTTP/1.1 200"));
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut callback = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(1))
                }
                other => panic!("bounded callback accept: {other:?}"),
            }
        };
        callback.set_nonblocking(false).unwrap();
        callback
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut wire = Vec::new();
        let notification = loop {
            if let Ok(request) = frameark_dlna::HttpRequest::parse(&wire) {
                break request;
            }
            assert!(wire.len() < 128 * 1024);
            let mut buffer = [0_u8; 4096];
            let length = callback.read(&mut buffer).unwrap();
            assert_ne!(length, 0);
            wire.extend_from_slice(&buffer[..length]);
        };
        assert_eq!(notification.method, "NOTIFY");
        assert_eq!(notification.header("seq"), Some("0"));
        callback
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
        let diagnostics = runtime.diagnostics.clone();
        runtime.stop();
        assert_eq!(diagnostics.callbacks.load(Ordering::Relaxed), 1);
        assert_eq!(diagnostics.callback_failures.load(Ordering::Relaxed), 0);
        assert!(
            std::net::TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_err()
        );
    }
}
