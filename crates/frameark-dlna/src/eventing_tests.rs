//! GENA controller/callback fixtures; not decoder or named-device evidence.

use super::*;
use crate::tests::{acknowledging_test_service, renderer_description, soap_request};

#[test]
fn drained_events_recheck_live_lease_but_preserve_initial_snapshot() {
    let mut service = acknowledging_test_service();
    let sid = accepted_sid(&mut service, "/upnp/event/renderingcontrol");
    let initial = service.drain_events().remove(0);
    volume(&mut service, 35);
    let update = service.drain_events().remove(0);
    assert!(service.event_is_deliverable(&update));
    assert_eq!(
        service
            .handle(&request(
                "UNSUBSCRIBE",
                "/upnp/event/renderingcontrol",
                &format!("SID: {sid}\r\n")
            ))
            .status,
        200
    );
    assert!(service.event_is_deliverable(&initial));
    assert!(!service.event_is_deliverable(&update));
    assert_eq!(
        callback_literal_address("http://127.0.0.1/cb").unwrap(),
        "127.0.0.1".parse::<IpAddr>().unwrap()
    );
    assert!(callback_literal_address("http://private.invalid/cb").is_err());
}

#[test]
fn ssdp_required_host_ext_and_media_renderer_type_round_trip() {
    assert!(GenaCallbackClient::new(Duration::MAX).is_err());
    assert!(
        MediaRendererTcpServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            acknowledging_test_service(),
            Duration::MAX
        )
        .is_err()
    );
    let advertisement = SsdpAdvertisement::new(
        "upnp:rootdevice",
        "uuid:fixture::upnp:rootdevice",
        "http://192.168.1.2/device.xml",
        300,
        "fixture/1 UPnP/1.0 FrameArk/0.1",
    )
    .unwrap();
    for alive in [true, false] {
        let wire = advertisement.notify(alive).encode().unwrap();
        assert_eq!(
            SsdpMessage::parse(&wire)
                .unwrap()
                .headers
                .get("host")
                .map(String::as_str),
            Some("239.255.255.250:1900")
        );
    }
    let wire = advertisement.response().encode().unwrap();
    assert_eq!(
        SsdpMessage::parse(&wire)
            .unwrap()
            .headers
            .get("ext")
            .map(String::as_str),
        Some("")
    );
    assert!(SsdpMessage::parse(b"HTTP/1.1 200 OK\r\nST: \r\n\r\n").is_err());
    assert!(
        renderer_description()
            .to_xml()
            .unwrap()
            .contains("<deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>")
    );
}

#[test]
fn callback_slow_trickle_cannot_extend_total_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let worker = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(1))
                }
                other => panic!("fixture accept {other:?}"),
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        read_http_request(&mut socket).unwrap();
        for byte in b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n" {
            if socket.write_all(&[*byte]).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    });
    let mut service = acknowledging_test_service();
    service.handle(&subscribe(
        "/upnp/event/avtransport",
        &format!("http://{address}/cb"),
    ));
    let event = service.drain_events().remove(0);
    let started = Instant::now();
    assert!(
        GenaCallbackClient::new(Duration::from_millis(100))
            .unwrap()
            .send(&event)
            .is_err()
    );
    assert!(started.elapsed() < Duration::from_millis(700));
    worker.join().unwrap();
}

#[test]
fn http_request_slow_trickle_cannot_hold_renderer_after_deadline() {
    let mut server = MediaRendererTcpServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        acknowledging_test_service(),
        Duration::from_millis(100),
    )
    .unwrap();
    let address = server.local_addr().unwrap();
    let worker = std::thread::spawn(move || server.serve_for(Duration::from_secs(1), 1).unwrap());
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_write_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let started = Instant::now();
    for byte in b"GET /device.xml HTTP/1.1\r\nHost: fixture\r\n\r\n" {
        if worker.is_finished() || socket.write_all(&[*byte]).is_err() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(worker.join().unwrap(), 1);
    assert!(started.elapsed() < Duration::from_millis(700));
}

fn subscribe(path: &str, callback: &str) -> HttpRequest {
    request(
        "SUBSCRIBE",
        path,
        &format!("NT: upnp:event\r\nCALLBACK: <{callback}>\r\nTIMEOUT: Second-120\r\n"),
    )
}

fn request(method: &str, path: &str, headers: &str) -> HttpRequest {
    HttpRequest::parse(
        format!("{method} {path} HTTP/1.1\r\nHost: fixture\r\n{headers}\r\n").as_bytes(),
    )
    .unwrap()
}

fn volume(service: &mut MediaRendererHttpService, value: u8) {
    let request = soap_request(
        "/upnp/control/renderingcontrol",
        RENDERING_CONTROL_SERVICE,
        "SetVolume",
        &format!(
            "<InstanceID>0</InstanceID><Channel>Master</Channel><DesiredVolume>{value}</DesiredVolume>"
        ),
    );
    assert_eq!(service.handle(&request).status, 200);
}

fn accepted_sid(service: &mut MediaRendererHttpService, path: &str) -> String {
    let response = service.handle(&subscribe(path, "http://127.0.0.1:9000/events"));
    assert_eq!(response.status, 200);
    assert!(
        response
            .headers
            .get("server")
            .unwrap()
            .contains("UPnP/1.0 FrameArk/")
    );
    let sid = response.headers.get("sid").unwrap().clone();
    assert_eq!(sid.len(), 41);
    assert!(sid.starts_with("uuid:"));
    assert_eq!(&sid[19..20], "4");
    assert!(matches!(&sid[24..25], "8" | "9" | "a" | "b"));
    sid
}

#[test]
fn subscriptions_follow_advertised_event_urls_and_are_service_scoped() {
    let mut description = renderer_description();
    description.services[0].event_sub_url = "/custom/events/transport".to_string();
    let mut service = MediaRendererHttpService::new(description).unwrap();
    let description = service.handle(&request("GET", "/device.xml", ""));
    assert!(
        String::from_utf8(description.body)
            .unwrap()
            .contains("<eventSubURL>/custom/events/transport</eventSubURL>")
    );
    assert_eq!(
        service
            .handle(&subscribe(
                "/upnp/control/avtransport",
                "http://127.0.0.1:9000/events"
            ))
            .status,
        404
    );
    let sid = accepted_sid(&mut service, "/custom/events/transport");
    for method in ["SUBSCRIBE", "UNSUBSCRIBE"] {
        let response = service.handle(&request(
            method,
            "/upnp/event/renderingcontrol",
            &format!("SID: {sid}\r\n"),
        ));
        assert_eq!(response.status, 412);
        assert_eq!(service.subscription_count(), 1);
    }
    let renewal = service.handle(&request(
        "SUBSCRIBE",
        "/custom/events/transport",
        &format!("SID: {sid}\r\nTIMEOUT: Second-60\r\n"),
    ));
    assert_eq!(renewal.status, 200);
    assert_eq!(renewal.headers.get("sid"), Some(&sid));
    assert_eq!(service.drain_events().len(), 1); // renewal creates no extra initial
    assert_eq!(
        service
            .handle(&request(
                "UNSUBSCRIBE",
                "/custom/events/transport",
                &format!("SID: {sid}\r\n")
            ))
            .status,
        200
    );
    assert_eq!(service.subscription_count(), 0);
    assert_eq!(
        service
            .handle(&request(
                "SUBSCRIBE",
                "/custom/events/transport",
                &format!("SID: {sid}\r\n")
            ))
            .status,
        412
    );
}

#[test]
fn each_subscriber_gets_only_its_own_initial_and_new_uuid() {
    let mut service = acknowledging_test_service();
    let first = accepted_sid(&mut service, "/upnp/event/avtransport");
    let second = accepted_sid(&mut service, "/upnp/event/avtransport");
    assert_ne!(first, second);
    let events = service.drain_events();
    assert_eq!(events.len(), 2);
    assert_eq!(
        events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![0, 0]
    );
    assert_eq!(events.iter().filter(|event| event.sid == first).count(), 1);
    assert_eq!(events.iter().filter(|event| event.sid == second).count(), 1);
    let mut another = acknowledging_test_service();
    assert_ne!(accepted_sid(&mut another, "/upnp/event/avtransport"), first);
}

#[test]
fn malformed_subscription_headers_and_callbacks_do_not_mutate_leases() {
    let mut service = acknowledging_test_service();
    let path = "/upnp/event/avtransport";
    let sid = accepted_sid(&mut service, path);
    for method in ["SUBSCRIBE", "UNSUBSCRIBE"] {
        for forbidden in ["NT: upnp:event", "CALLBACK: <http://127.0.0.1:9000/events>"] {
            assert_eq!(
                service
                    .handle(&request(
                        method,
                        path,
                        &format!("SID: {sid}\r\n{forbidden}\r\n")
                    ))
                    .status,
                400
            );
        }
    }
    for callback in [
        "http://user:pass@127.0.0.1:9000/events",
        "http://127.0.0.1:9000/events#fragment",
        "file:///private",
        "http://127.0.0.1:99999/events",
        "http://[::1/events",
    ] {
        assert_eq!(service.handle(&subscribe(path, callback)).status, 412);
    }
    assert_eq!(
        service.handle(&request("UNSUBSCRIBE", path, "")).status,
        412
    );
    let mut body = request("SUBSCRIBE", path, &format!("SID: {sid}\r\n"));
    body.body = vec![1];
    assert_eq!(service.handle(&body).status, 400);
    assert_eq!(service.subscription_count(), 1);
}

#[test]
fn sequence_uses_unsigned_range_and_wraps_to_one_not_zero() {
    let mut registry = GenaRegistry::default();
    let sub = registry
        .subscribe(AVTRANSPORT_SERVICE, "http://127.0.0.1:9000/events", None)
        .unwrap();
    let properties = BTreeMap::from([("LastChange".into(), "fixture".into())]);
    assert_eq!(
        registry.publish(AVTRANSPORT_SERVICE, &properties).unwrap()[0].sequence,
        0
    );
    registry
        .subscriptions
        .get_mut(&sub.sid)
        .unwrap()
        .next_sequence = u32::MAX;
    let event = registry
        .publish(AVTRANSPORT_SERVICE, &properties)
        .unwrap()
        .remove(0);
    assert_eq!(event.sequence, u32::MAX);
    let wire = HttpRequest::parse(&event.encode_http_notify().unwrap()).unwrap();
    assert_eq!(wire.header("seq"), Some("4294967295"));
    assert_eq!(
        registry.publish(AVTRANSPORT_SERVICE, &properties).unwrap()[0].sequence,
        1
    );
}

#[test]
fn coalescing_keeps_initial_snapshot_and_latest_unsent_state_without_seq_gaps() {
    let mut service = acknowledging_test_service();
    let sid = accepted_sid(&mut service, "/upnp/event/renderingcontrol");
    for value in 0..=100 {
        volume(&mut service, value);
    }
    assert_eq!(service.pending_events.len(), 2);
    let events = service.drain_events();
    assert_eq!(events[0].sequence, 0);
    assert!(events[0].body.contains("val=&quot;50&quot;"));
    assert_eq!(events[1].sequence, 1);
    assert!(events[1].body.contains("val=&quot;100&quot;"));
    assert_eq!(events[1].sid, sid);
    volume(&mut service, 73);
    let next = service.drain_events();
    assert_eq!(next[0].sequence, 2);
    assert_eq!(
        next[0].body,
        include_str!("../tests/fixtures/rcs-volume-73.xml").trim()
    );
}

#[test]
fn pending_initials_survive_unsubscribe_but_admission_bounds_memory() {
    let mut service = acknowledging_test_service();
    let path = "/upnp/event/avtransport";
    for _ in 0..MAX_GENA_PENDING_EVENTS - 1 {
        let sid = accepted_sid(&mut service, path);
        assert_eq!(
            service
                .handle(&request("UNSUBSCRIBE", path, &format!("SID: {sid}\r\n")))
                .status,
            200
        );
    }
    assert_eq!(service.subscription_count(), 0);
    assert_eq!(
        service
            .handle(&subscribe(path, "http://127.0.0.1:9000/events"))
            .status,
        503
    );
    let events = service.drain_events();
    assert_eq!(events.len(), MAX_GENA_PENDING_EVENTS - 1);
    assert!(events.iter().all(|event| event.sequence == 0));
    accepted_sid(&mut service, path);
}

#[test]
fn full_registry_reserves_all_initial_and_update_slots() {
    let mut service = acknowledging_test_service();
    for _ in 0..MAX_GENA_SUBSCRIPTIONS {
        accepted_sid(&mut service, "/upnp/event/renderingcontrol");
    }
    assert_eq!(
        service
            .handle(&subscribe(
                "/upnp/event/renderingcontrol",
                "http://127.0.0.1:9000/events"
            ))
            .status,
        503
    );
    for value in 0..=100 {
        volume(&mut service, value);
    }
    assert_eq!(service.pending_events.len(), MAX_GENA_PENDING_EVENTS);
    let events = service.drain_events();
    assert_eq!(
        events.iter().filter(|event| event.sequence == 0).count(),
        MAX_GENA_SUBSCRIPTIONS
    );
    assert_eq!(
        events.iter().filter(|event| event.sequence == 1).count(),
        MAX_GENA_SUBSCRIPTIONS
    );
}

#[test]
fn lastchange_is_double_escaped_and_metadata_expansion_is_bounded() {
    let mut service = acknowledging_test_service();
    let metadata = format!(
        "<item title=\"帧舟 & fixture\">\n{}</item>",
        "\"".repeat(4000)
    );
    assert!(metadata.len() <= MAX_RENDERER_METADATA_BYTES);
    // Encoder-only worst-case stored-value boundary. This is deliberately not
    // network input: its escaped SOAP representation exceeds the body limit.
    service.state.current_uri = "http://192.168.1.2/video?a=1&b=2".into();
    service.state.current_uri_metadata = metadata;
    let sid = accepted_sid(&mut service, "/upnp/event/avtransport");
    let event = service.drain_events().remove(0);
    assert_eq!(event.sid, sid);
    assert!(event.body.contains("<LastChange>&lt;Event"));
    assert!(
        event
            .body
            .contains("urn:schemas-upnp-org:metadata-1-0/AVT/")
    );
    assert!(event.body.contains("a=1&amp;amp;b=2"));
    assert!(event.body.contains("&amp;#10;"));
    assert!(!event.body.contains("RelativeTimePosition"));
    assert!(event.body.len() <= MAX_GENA_EVENT_BYTES);
    HttpRequest::parse(&event.encode_http_notify().unwrap()).unwrap();
    let invalid = BTreeMap::from([("9invalid".into(), "value".into())]);
    assert!(gena_property_set(&invalid).is_err());
    let invalid = BTreeMap::from([("LastChange".into(), "bad\0value".into())]);
    assert!(gena_property_set(&invalid).is_err());
}

#[test]
fn metadata_limits_reject_before_output_and_accept_xml_whitespace() {
    let mut service = acknowledging_test_service();
    for metadata in [
        "x".repeat(MAX_RENDERER_METADATA_BYTES + 1),
        "bad\0metadata".to_string(),
    ] {
        let action = soap_request(
            "/upnp/control/avtransport",
            AVTRANSPORT_SERVICE,
            "SetAVTransportURI",
            &format!(
                "<InstanceID>0</InstanceID><CurrentURI>http://127.0.0.1/media</CurrentURI><CurrentURIMetaData>{metadata}</CurrentURIMetaData>"
            ),
        );
        assert_eq!(service.handle(&action).status, 500);
        assert_eq!(service.playback_report().attempted, 0);
        assert!(service.state.current_uri.is_empty());
    }
    let metadata = format!("x\n\t\r{}", "x".repeat(MAX_RENDERER_METADATA_BYTES - 4));
    let action = soap_request(
        "/upnp/control/avtransport",
        AVTRANSPORT_SERVICE,
        "SetAVTransportURI",
        &format!(
            "<InstanceID>0</InstanceID><CurrentURI>http://127.0.0.1/media</CurrentURI><CurrentURIMetaData>{metadata}</CurrentURIMetaData>"
        ),
    );
    assert_eq!(service.handle(&action).status, 200);
    assert_eq!(service.playback_report().attempted, 1);
    assert_eq!(
        service.state.current_uri_metadata,
        metadata.replace('\r', "\n")
    );
    accepted_sid(&mut service, "/upnp/event/avtransport");
    assert_eq!(service.drain_events().len(), 1);
}

#[test]
fn callback_default_port_and_query_only_target_are_preserved() {
    assert_eq!(
        callback_socket_address("127.0.0.1").unwrap(),
        "127.0.0.1:80".parse().unwrap()
    );
    assert_eq!(
        callback_socket_address("[::1]").unwrap(),
        "[::1]:80".parse().unwrap()
    );
    assert_eq!(
        callback_socket_address("127.0.0.1:9000").unwrap(),
        "127.0.0.1:9000".parse().unwrap()
    );
    assert_eq!(
        callback_authority_and_target("http://127.0.0.1?subscription=fixture").unwrap(),
        ("127.0.0.1".into(), "/?subscription=fixture".into())
    );
    assert!(callback_authority_and_target("http://user@127.0.0.1/events").is_err());
}

#[test]
fn connection_manager_initial_and_capability_updates_use_its_own_properties() {
    let mut service = acknowledging_test_service();
    accepted_sid(&mut service, "/upnp/event/connectionmanager");
    let initial = service.drain_events().remove(0);
    assert!(
        initial
            .body
            .contains("<SourceProtocolInfo></SourceProtocolInfo>")
    );
    assert!(initial.body.contains("<SinkProtocolInfo>"));
    assert!(
        initial
            .body
            .contains("<CurrentConnectionIDs>0</CurrentConnectionIDs>")
    );
    assert!(!initial.body.contains("LastChange"));
    service
        .set_capabilities(
            MediaFormatCapabilities::new(vec!["http-get:*:video/webm:*".into()]).unwrap(),
        )
        .unwrap();
    let update = service.drain_events().remove(0);
    assert_eq!(update.sequence, 1);
    assert!(update.body.contains("http-get:*:video/webm:*"));
    for (service_type, expected) in [
        (AVTRANSPORT_SERVICE, "LastChange"),
        (RENDERING_CONTROL_SERVICE, "LastChange"),
        (CONNECTION_MANAGER_SERVICE, "SinkProtocolInfo"),
    ] {
        assert!(scpd_xml(service_type).contains(&format!(
            "<stateVariable sendEvents=\"yes\"><name>{expected}</name>"
        )));
    }
}

#[test]
fn real_http_subscription_closes_response_then_delivers_initial_notify() {
    let callback = TcpListener::bind("127.0.0.1:0").unwrap();
    let callback_address = callback.local_addr().unwrap();
    callback.set_nonblocking(true).unwrap();
    let mut server = MediaRendererTcpServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        acknowledging_test_service(),
        Duration::from_secs(1),
    )
    .unwrap();
    let address = server.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        assert_eq!(server.serve_for(Duration::from_secs(3), 1).unwrap(), 1);
        // serve_for has closed the response connection before callback I/O.
        let event = server.service_mut().drain_events().remove(0);
        GenaCallbackClient::new(Duration::from_secs(2))
            .unwrap()
            .send(&event)
            .unwrap()
    });
    let mut client = TcpStream::connect_timeout(&address, Duration::from_secs(1)).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client.write_all(format!("SUBSCRIBE /upnp/event/renderingcontrol HTTP/1.1\r\nHost: fixture\r\nNT: upnp:event\r\nCALLBACK: <http://{callback_address}/events>\r\n\r\n").as_bytes()).unwrap();
    let mut response_wire = Vec::new();
    client
        .take(MAX_HTTP_BYTES as u64)
        .read_to_end(&mut response_wire)
        .unwrap();
    let response = HttpResponse::parse(&response_wire).unwrap();
    assert_eq!(response.status, 200);
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut notification = loop {
        match callback.accept() {
            Ok((stream, _)) => break stream,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(1))
            }
            Err(error) => panic!("callback accept failed: {error:?}"),
        }
    };
    notification.set_nonblocking(false).unwrap();
    notification
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    notification
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let event = read_http_request(&mut notification).unwrap();
    assert_eq!(event.method, "NOTIFY");
    assert_eq!(
        event.header("sid"),
        response.headers.get("sid").map(String::as_str)
    );
    assert_eq!(event.header("seq"), Some("0"));
    assert!(
        String::from_utf8(event.body)
            .unwrap()
            .contains("<LastChange>")
    );
    notification
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
        .unwrap();
    assert_eq!(worker.join().unwrap().status, 200);
}

#[test]
fn failed_subscription_response_releases_initial_and_lease() {
    let mut server = MediaRendererTcpServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        acknowledging_test_service(),
        Duration::from_secs(1),
    )
    .unwrap();
    let mut client =
        TcpStream::connect_timeout(&server.local_addr().unwrap(), Duration::from_secs(1)).unwrap();
    client
        .set_write_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    client.write_all(b"SUBSCRIBE /upnp/event/avtransport HTTP/1.1\r\nHost: fixture\r\nNT: upnp:event\r\nCALLBACK: <http://127.0.0.1:9000/events>\r\n\r\n").unwrap();
    let (connection, peer) = server.listener.accept().unwrap();
    connection.shutdown(std::net::Shutdown::Write).unwrap();
    assert!(server.serve_connection(connection, peer).is_err());
    assert_eq!(server.service().subscription_count(), 0);
    assert!(server.service_mut().drain_events().is_empty());
}
