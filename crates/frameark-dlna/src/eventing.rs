//! Instance-zero LastChange snapshots for the implemented renderer profile.

use super::*;

pub(super) fn valid_xml_text(value: &str) -> bool {
    value.chars().all(|c| matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}'))
}

fn attribute(value: &str) -> String {
    xml_escape(value)
        .replace('\t', "&#9;")
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
}

fn value(xml: &mut String, name: &str, content: &str) {
    xml.push_str(&format!("<{name} val=\"{}\"/>", attribute(content)));
}

pub(super) fn state_event_body(
    service: &str,
    state: &MediaRendererState,
    capabilities: &MediaFormatCapabilities,
) -> Result<String, DlnaError> {
    let mut properties = BTreeMap::new();
    match service {
        AVTRANSPORT_SERVICE => {
            let mut xml =
                "<Event xmlns=\"urn:schemas-upnp-org:metadata-1-0/AVT/\"><InstanceID val=\"0\">"
                    .to_string();
            value(&mut xml, "TransportState", state.transport_state.as_upnp());
            value(&mut xml, "TransportStatus", "OK");
            value(&mut xml, "TransportPlaySpeed", "1");
            let tracks = if state.current_uri.is_empty() {
                "0"
            } else {
                "1"
            };
            value(&mut xml, "NumberOfTracks", tracks);
            value(&mut xml, "CurrentTrack", tracks);
            value(&mut xml, "CurrentTrackURI", &state.current_uri);
            value(&mut xml, "AVTransportURI", &state.current_uri);
            value(
                &mut xml,
                "CurrentTrackMetaData",
                &state.current_uri_metadata,
            );
            value(
                &mut xml,
                "AVTransportURIMetaData",
                &state.current_uri_metadata,
            );
            let duration = format_upnp_time(state.duration_seconds.unwrap_or(0));
            value(&mut xml, "CurrentTrackDuration", &duration);
            value(&mut xml, "CurrentMediaDuration", &duration);
            // Positions are deliberately queried, not evented via LastChange.
            xml.push_str("</InstanceID></Event>");
            properties.insert("LastChange".to_string(), xml);
        }
        RENDERING_CONTROL_SERVICE => {
            properties.insert("LastChange".to_string(), format!(
                "<Event xmlns=\"urn:schemas-upnp-org:metadata-1-0/RCS/\"><InstanceID val=\"0\"><Volume channel=\"Master\" val=\"{}\"/></InstanceID></Event>", state.volume
            ));
        }
        CONNECTION_MANAGER_SERVICE => {
            properties.insert("SourceProtocolInfo".to_string(), String::new());
            properties.insert(
                "SinkProtocolInfo".to_string(),
                capabilities.sink_protocol_info(),
            );
            properties.insert("CurrentConnectionIDs".to_string(), "0".to_string());
        }
        _ => return Err(DlnaError::Invalid("GENA service type")),
    }
    gena_property_set(&properties)
}
