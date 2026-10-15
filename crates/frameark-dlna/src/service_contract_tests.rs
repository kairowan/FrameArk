//! Wire-level service contract and rejection regression tests, not certification.

use super::*;
use crate::tests::{acknowledging_test_service, renderer_description, soap_request};

fn call(service: &mut MediaRendererHttpService, name: &str, args: &str) -> HttpResponse {
    let (kind, path) = match name {
        "GetProtocolInfo" | "GetCurrentConnectionIDs" | "GetCurrentConnectionInfo" => (
            CONNECTION_MANAGER_SERVICE,
            "/upnp/control/connectionmanager",
        ),
        "ListPresets" | "SelectPreset" | "GetVolume" | "SetVolume" => {
            (RENDERING_CONTROL_SERVICE, "/upnp/control/renderingcontrol")
        }
        _ => (AVTRANSPORT_SERVICE, "/upnp/control/avtransport"),
    };
    service.handle(&soap_request(path, kind, name, args))
}

fn outputs(response: HttpResponse) -> Vec<(String, String)> {
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    let text = String::from_utf8(response.body).unwrap();
    let doc = roxmltree::Document::parse(&text).unwrap();
    let action = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name().ends_with("Response"))
        .unwrap();
    action
        .children()
        .filter(|n| n.is_element())
        .map(|n| {
            (
                n.tag_name().name().into(),
                n.text().unwrap_or_default().into(),
            )
        })
        .collect()
}

fn fault(response: HttpResponse, code: u16) {
    assert_eq!(response.status, 500);
    let text = String::from_utf8(response.body).unwrap();
    let doc = roxmltree::Document::parse(&text).unwrap();
    assert_eq!(
        doc.descendants()
            .find(|n| n.has_tag_name(("urn:schemas-upnp-org:control-1-0", "errorCode")))
            .unwrap()
            .text(),
        Some(code.to_string().as_str())
    );
}

const INSTANCE: &str = "<InstanceID>0</InstanceID>";

#[test]
fn soap_depth_is_bounded_before_recursive_xml_work_on_small_stacks() {
    std::thread::Builder::new().stack_size(128 * 1024).spawn(|| {
        let header = format!("\"{AVTRANSPORT_SERVICE}#GetTransportInfo\"");
        for depth in [5, 16, 64, 129, 512] {
            let body = format!("{}0{}", "<node>".repeat(depth), "</node>".repeat(depth));
            assert!(parse_soap_action(&header, body.as_bytes()).is_err());
        }
        // '<' and '>' in comments, CDATA and quoted attributes are not nesting.
        let request = format!("<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" marker=\"&lt; > /&gt;\"><!-- <fake><fake> --><s:Body><a:GetTransportInfo xmlns:a=\"{AVTRANSPORT_SERVICE}\"><InstanceID><![CDATA[0]]></InstanceID></a:GetTransportInfo></s:Body></s:Envelope>");
        assert_eq!(parse_soap_action(&header, request.as_bytes()).unwrap().arg("InstanceID"), Some("0"));
    }).unwrap().join().unwrap();
}

#[test]
fn lastchange_covers_all_indirectly_evented_declared_state() {
    let service = acknowledging_test_service();
    for kind in [AVTRANSPORT_SERVICE, RENDERING_CONTROL_SERVICE] {
        let schema = scpd_xml(kind);
        let doc = roxmltree::Document::parse(&schema).unwrap();
        let event = service.state_event_body(kind).unwrap();
        let outer = roxmltree::Document::parse(&event).unwrap();
        let inner_text = outer
            .descendants()
            .find(|n| n.has_tag_name("LastChange"))
            .unwrap()
            .text()
            .unwrap();
        let inner = roxmltree::Document::parse(inner_text).unwrap();
        let values: BTreeMap<_, _> = inner
            .descendants()
            .filter(|n| {
                n.is_element()
                    && n.tag_name().name() != "InstanceID"
                    && n.attribute("val").is_some()
            })
            .map(|n| (n.tag_name().name(), n.attribute("val").unwrap()))
            .collect();
        for variable in doc
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "stateVariable")
        {
            let name = variable
                .children()
                .find(|n| n.is_element() && n.tag_name().name() == "name")
                .unwrap()
                .text()
                .unwrap();
            if name == "LastChange" || name.starts_with("A_ARG_TYPE_") || name.ends_with("Position")
            {
                assert!(!values.contains_key(name));
                continue;
            }
            let value = values
                .get(name)
                .expect("initial snapshot missing declared state");
            let allowed: Vec<_> = variable
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "allowedValue")
                .map(|n| n.text().unwrap())
                .collect();
            if !allowed.is_empty() {
                assert!(allowed.contains(value), "{name}");
            }
        }
    }
}

#[test]
fn bounded_soap_mutation_smoke_never_panics_or_bypasses_known_action_shape() {
    let seed = soap_request(
        "/upnp/control/avtransport",
        AVTRANSPORT_SERVICE,
        "GetTransportInfo",
        INSTANCE,
    );
    let mut service = acknowledging_test_service();
    for index in 0..seed.body.len() {
        for byte in [0, b'<', b'>', b'&', b'\'', 255] {
            let mut request = seed.clone();
            request.body[index] = byte;
            let response = service.handle(&request);
            assert!(matches!(response.status, 200 | 500));
            assert_eq!(service.playback_report().attempted, 0);
            assert_eq!(service.state(), &MediaRendererState::default());
        }
    }
}

#[test]
fn required_actions_state_types_and_advertised_outputs_match_v1_profile() {
    // Expected action inventory is independently transcribed from the v1 specs,
    // not derived from the implementation's declaration arrays.
    for (kind, expected, count) in [
        (
            AVTRANSPORT_SERVICE,
            "SetAVTransportURI GetMediaInfo GetTransportInfo GetPositionInfo GetDeviceCapabilities GetTransportSettings Stop Play Pause Seek Next Previous",
            29,
        ),
        (
            RENDERING_CONTROL_SERVICE,
            "ListPresets SelectPreset GetVolume SetVolume",
            6,
        ),
        (
            CONNECTION_MANAGER_SERVICE,
            "GetProtocolInfo GetCurrentConnectionIDs GetCurrentConnectionInfo",
            10,
        ),
    ] {
        let xml = scpd_xml(kind);
        assert!(xml.len() < MAX_HTTP_BYTES);
        let doc = roxmltree::Document::parse(&xml).unwrap();
        assert!(
            doc.root_element()
                .has_tag_name(("urn:schemas-upnp-org:service-1-0", "scpd"))
        );
        let field = |node: roxmltree::Node<'_, '_>, name: &str| {
            node.children()
                .find(|n| n.is_element() && n.tag_name().name() == name)
                .unwrap()
                .text()
                .unwrap()
                .to_string()
        };
        let variables: BTreeMap<_, _> = doc
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "stateVariable")
            .map(|n| (field(n, "name"), n))
            .collect();
        assert_eq!(variables.len(), count);
        let actions: Vec<_> = doc
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "action")
            .collect();
        assert_eq!(
            actions
                .iter()
                .map(|n| field(*n, "name"))
                .collect::<Vec<_>>(),
            expected.split_whitespace().collect::<Vec<_>>()
        );
        for action in actions {
            let mut seen_output = false;
            let mut names = std::collections::BTreeSet::new();
            for argument in action
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "argument")
            {
                assert!(names.insert(field(argument, "name")));
                let direction = field(argument, "direction");
                assert!(matches!(direction.as_str(), "in" | "out"));
                if direction == "out" {
                    seen_output = true;
                } else {
                    assert!(!seen_output);
                }
                assert!(variables.contains_key(&field(argument, "relatedStateVariable")));
            }
        }
        for (name, node) in variables {
            let evented = if kind == CONNECTION_MANAGER_SERVICE {
                matches!(
                    name.as_str(),
                    "SourceProtocolInfo" | "SinkProtocolInfo" | "CurrentConnectionIDs"
                )
            } else {
                name == "LastChange"
            };
            assert_eq!(
                node.attribute("sendEvents"),
                Some(if evented { "yes" } else { "no" })
            );
            let kind = field(node, "dataType");
            assert!(matches!(kind.as_str(), "string" | "ui2" | "ui4" | "i4"));
        }
    }
    let xml = scpd_xml(AVTRANSPORT_SERVICE);
    assert!(xml.contains("<allowedValue>TRACK_NR</allowedValue>"));
    assert!(!xml.contains("<name>Record</name>"));
    assert!(!xml.contains("<name>SetNextAVTransportURI</name>"));
    assert!(!scpd_xml(RENDERING_CONTROL_SERVICE).contains("<name>SetMute</name>"));
}

#[test]
fn query_outputs_are_complete_ordered_and_consistent_with_single_track_state() {
    let mut service = acknowledging_test_service();
    let position = outputs(call(&mut service, "GetPositionInfo", INSTANCE));
    assert_eq!(
        position.iter().map(|p| p.0.as_str()).collect::<Vec<_>>(),
        [
            "Track",
            "TrackDuration",
            "TrackMetaData",
            "TrackURI",
            "RelTime",
            "AbsTime",
            "RelCount",
            "AbsCount"
        ]
    );
    assert_eq!(position[0].1, "0");
    assert_eq!(position[6].1, i32::MAX.to_string());
    assert_eq!(position[7].1, i32::MAX.to_string());
    let info = outputs(call(&mut service, "GetMediaInfo", INSTANCE));
    assert_eq!(
        info.iter().map(|p| p.0.as_str()).collect::<Vec<_>>(),
        [
            "NrTracks",
            "MediaDuration",
            "CurrentURI",
            "CurrentURIMetaData",
            "NextURI",
            "NextURIMetaData",
            "PlayMedium",
            "RecordMedium",
            "WriteStatus"
        ]
    );
    assert_eq!(info[6].1, "NONE");
    assert_eq!(info[4].1, "NOT_IMPLEMENTED");
    let metadata =
        "<DIDL-Lite><item><res duration=\"00:01:00\">title &amp; stuff</res></item></DIDL-Lite>";
    let args = format!(
        "{INSTANCE}<CurrentURI>http://192.168.1.2/a?x=1&amp;y=2</CurrentURI><CurrentURIMetaData>{}</CurrentURIMetaData>",
        xml_escape(metadata)
    );
    assert_eq!(call(&mut service, "SetAVTransportURI", &args).status, 200);
    assert_eq!(
        call(
            &mut service,
            "Seek",
            &format!("{INSTANCE}<Unit>REL_TIME</Unit><Target>00:00:09</Target>")
        )
        .status,
        200
    );
    let position = outputs(call(&mut service, "GetPositionInfo", INSTANCE));
    assert_eq!(position[0].1, "1");
    assert_eq!(position[1].1, "00:01:00");
    assert_eq!(position[2].1, metadata);
    assert_eq!(position[3].1, "http://192.168.1.2/a?x=1&y=2");
    assert_eq!(position[4].1, "00:00:09");
    assert_eq!(position[4].1, position[5].1);
    let info = outputs(call(&mut service, "GetMediaInfo", INSTANCE));
    assert_eq!(info[6].1, "NETWORK");
    assert_eq!(info[2].1, position[3].1);
    assert_eq!(info[3].1, position[2].1);
    assert_eq!(
        outputs(call(&mut service, "GetDeviceCapabilities", INSTANCE)),
        [
            ("PlayMedia".into(), "NETWORK".into()),
            ("RecMedia".into(), "NOT_IMPLEMENTED".into()),
            ("RecQualityModes".into(), "NOT_IMPLEMENTED".into())
        ]
    );
    assert_eq!(
        outputs(call(&mut service, "GetTransportSettings", INSTANCE)),
        [
            ("PlayMode".into(), "NORMAL".into()),
            ("RecQualityMode".into(), "NOT_IMPLEMENTED".into())
        ]
    );
}

#[test]
fn strict_arguments_and_service_specific_faults_do_not_dispatch() {
    let mut service = acknowledging_test_service();
    for (name, args, code) in [
        ("GetTransportInfo", "", 402),
        (
            "GetTransportInfo",
            "<InstanceID>4294967296</InstanceID>",
            402,
        ),
        ("GetTransportInfo", "<InstanceID>1</InstanceID>", 718),
        ("ListPresets", "<InstanceID>1</InstanceID>", 702),
        ("ListPresets", "<InstanceID>-1</InstanceID>", 402),
        (
            "SelectPreset",
            "<InstanceID>0</InstanceID><PresetName>unknown</PresetName>",
            701,
        ),
        ("SelectPreset", INSTANCE, 402),
        ("GetCurrentConnectionInfo", "", 402),
        (
            "GetCurrentConnectionInfo",
            "<ConnectionID>2147483648</ConnectionID>",
            402,
        ),
        (
            "GetCurrentConnectionInfo",
            "<ConnectionID>-1</ConnectionID>",
            706,
        ),
        (
            "GetCurrentConnectionInfo",
            "<ConnectionID>1</ConnectionID>",
            706,
        ),
        ("GetProtocolInfo", "<Ignored>extension</Ignored>", 402),
        (
            "GetVolume",
            "<InstanceID>0</InstanceID><Channel>LF</Channel>",
            501,
        ),
        ("Play", INSTANCE, 402),
        ("Play", "<InstanceID>0</InstanceID><Speed>2</Speed>", 717),
        (
            "SetAVTransportURI",
            "<InstanceID>0</InstanceID><CurrentURI>http://192.168.1.2/a</CurrentURI>",
            402,
        ),
        ("NoSuchAction", INSTANCE, 401),
        ("Record", INSTANCE, 401),
    ] {
        fault(call(&mut service, name, args), code);
    }
    assert_eq!(service.playback_report().attempted, 0);
    let connection = outputs(call(
        &mut service,
        "GetCurrentConnectionInfo",
        "<ConnectionID>0</ConnectionID>",
    ));
    assert_eq!(
        connection.iter().map(|p| p.0.as_str()).collect::<Vec<_>>(),
        [
            "RcsID",
            "AVTransportID",
            "ProtocolInfo",
            "PeerConnectionManager",
            "PeerConnectionID",
            "Direction",
            "Status"
        ]
    );
    assert_eq!(connection[5].1, "Input");
    assert_eq!(connection[6].1, "Unknown");
    assert!(connection[2].1.is_empty());
}

#[test]
fn soap_structure_namespaces_duplicates_and_resource_bounds_are_enforced() {
    let valid = soap_request(
        "/upnp/control/avtransport",
        AVTRANSPORT_SERVICE,
        "GetTransportInfo",
        INSTANCE,
    );
    let body = String::from_utf8(valid.body.clone()).unwrap();
    let mut cases = vec![
        body.replace("GetTransportInfo", "GetPositionInfo"),
        body.replace(AVTRANSPORT_SERVICE, RENDERING_CONTROL_SERVICE),
        body.replace(
            "http://schemas.xmlsoap.org/soap/envelope/",
            "urn:wrong-envelope",
        ),
        body.replace(
            INSTANCE,
            "<InstanceID>0</InstanceID><InstanceID>0</InstanceID>",
        ),
        body.replace(INSTANCE, "<InstanceID><nested>0</nested></InstanceID>"),
        body.replace(
            INSTANCE,
            "<x:InstanceID xmlns:x=\"urn:foreign\">0</x:InstanceID>",
        ),
        body.replace(INSTANCE, "<InstanceID junk=\"0\">0</InstanceID>"),
        body.replace(INSTANCE, "<InstanceID>0</InstanceID><Extra/>"),
        body.replace(INSTANCE, "<InstanceID>&notDefined;</InstanceID>"),
        body.replace(INSTANCE, "<InstanceID>&#0;</InstanceID>"),
        body.replace(INSTANCE, "<InstanceID>0</InstanceID>not-an-argument"),
        body.replace(
            INSTANCE,
            &format!("<InstanceID>0</InstanceID>{}", "<X/>".repeat(129)),
        ),
        body.replace(
            INSTANCE,
            &format!(
                "{}0{}",
                "<InstanceID>".repeat(129),
                "</InstanceID>".repeat(129)
            ),
        ),
        format!("<!DOCTYPE x>{body}"),
        body.replace(
            INSTANCE,
            &format!("<InstanceID>{}</InstanceID>", "0".repeat(MAX_BODY_BYTES)),
        ),
    ];
    let action = format!(
        "<u:GetTransportInfo xmlns:u=\"{AVTRANSPORT_SERVICE}\">{INSTANCE}</u:GetTransportInfo>"
    );
    cases.push(format!("<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>{action}{action}</s:Body></s:Envelope>"));
    cases.push(format!("<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Header>{action}</s:Header><s:Body>{action}</s:Body></s:Envelope>"));
    let mut service = acknowledging_test_service();
    for text in cases {
        let mut request = valid.clone();
        request.body = text.into_bytes();
        fault(service.handle(&request), 402);
    }
    assert_eq!(service.playback_report().attempted, 0);
    let mut request = valid;
    request.body = format!("<e:Envelope xmlns:e=\"http://schemas.xmlsoap.org/soap/envelope/\"><e:Body><GetTransportInfo xmlns=\"{AVTRANSPORT_SERVICE}\"><InstanceID>&#48;</InstanceID></GetTransportInfo></e:Body></e:Envelope>").into_bytes();
    assert_eq!(service.handle(&request).status, 200);
}

#[test]
fn controller_reads_scpd_and_invokes_every_advertised_action_over_tcp() {
    let service = acknowledging_test_service();
    let description = renderer_description();
    let expected_requests = 3 + 19;
    let mut server = MediaRendererTcpServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        service,
        Duration::from_secs(2),
    )
    .unwrap();
    let address = server.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        server
            .serve_for(Duration::from_secs(15), expected_requests)
            .unwrap()
    });
    let send = |request: HttpRequest| {
        let mut socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut wire = format!(
            "{} {} HTTP/1.1\r\nHost: fixture\r\nContent-Length: {}\r\n",
            request.method,
            request.target,
            request.body.len()
        );
        if let Some(action) = request.header("soapaction") {
            wire.push_str(&format!("SOAPAction: {action}\r\n"));
        }
        wire.push_str("\r\n");
        socket.write_all(wire.as_bytes()).unwrap();
        socket.write_all(&request.body).unwrap();
        read_http_response(&mut socket).unwrap()
    };
    for description in description.services {
        let response = send(
            HttpRequest::parse(
                format!(
                    "GET {} HTTP/1.1\r\nHost: fixture\r\n\r\n",
                    description.scpd_url
                )
                .as_bytes(),
            )
            .unwrap(),
        );
        assert_eq!(response.status, 200);
        let xml = String::from_utf8(response.body).unwrap();
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let field = |node: roxmltree::Node<'_, '_>, name: &str| {
            node.children()
                .find(|n| n.is_element() && n.tag_name().name() == name)
                .unwrap()
                .text()
                .unwrap()
                .to_string()
        };
        for node in doc
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "action")
        {
            let name = field(node, "name");
            let mut args = String::new();
            let mut expected_outputs = Vec::new();
            for argument in node
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "argument")
            {
                let name = field(argument, "name");
                if field(argument, "direction") == "out" {
                    expected_outputs.push(name);
                    continue;
                }
                let value = match name.as_str() {
                    "InstanceID" | "ConnectionID" => "0",
                    "CurrentURI" => "http://192.168.1.2/sample.mp4",
                    "CurrentURIMetaData" => "",
                    "Speed" => "1",
                    "Channel" => "Master",
                    "DesiredVolume" => "27",
                    "PresetName" => "FactoryDefaults",
                    "Unit" => "TRACK_NR",
                    "Target" => "1",
                    _ => panic!("unhandled advertised input"),
                };
                push_tag(&mut args, &name, value);
            }
            let response = send(soap_request(
                &description.control_url,
                &description.service_type,
                &name,
                &args,
            ));
            if matches!(name.as_str(), "Next" | "Previous") {
                fault(response, 711);
            } else {
                assert_eq!(
                    outputs(response)
                        .into_iter()
                        .map(|p| p.0)
                        .collect::<Vec<_>>(),
                    expected_outputs,
                    "{name}"
                );
            }
        }
    }
    assert_eq!(worker.join().unwrap(), expected_requests);
}
