//! Independently implemented UPnP AV v1 single-resource service contracts.
//! See ADR 0132 for specification sources and the intentionally limited profile.

use super::*;

pub(super) struct Argument {
    pub name: &'static str,
    pub variable: &'static str,
}

pub(super) struct Action {
    pub name: &'static str,
    pub inputs: &'static [Argument],
    pub outputs: &'static [Argument],
}

const fn arg(name: &'static str, variable: &'static str) -> Argument {
    Argument { name, variable }
}

const INSTANCE: Argument = arg("InstanceID", "A_ARG_TYPE_InstanceID");
const CHANNEL: Argument = arg("Channel", "A_ARG_TYPE_Channel");

pub(super) fn actions(service: &str) -> &'static [Action] {
    match service {
        AVTRANSPORT_SERVICE => {
            const {
                &[
                    Action {
                        name: "SetAVTransportURI",
                        inputs: &[
                            INSTANCE,
                            arg("CurrentURI", "AVTransportURI"),
                            arg("CurrentURIMetaData", "AVTransportURIMetaData"),
                        ],
                        outputs: &[],
                    },
                    Action {
                        name: "GetMediaInfo",
                        inputs: &[INSTANCE],
                        outputs: &[
                            arg("NrTracks", "NumberOfTracks"),
                            arg("MediaDuration", "CurrentMediaDuration"),
                            arg("CurrentURI", "AVTransportURI"),
                            arg("CurrentURIMetaData", "AVTransportURIMetaData"),
                            arg("NextURI", "NextAVTransportURI"),
                            arg("NextURIMetaData", "NextAVTransportURIMetaData"),
                            arg("PlayMedium", "PlaybackStorageMedium"),
                            arg("RecordMedium", "RecordStorageMedium"),
                            arg("WriteStatus", "RecordMediumWriteStatus"),
                        ],
                    },
                    Action {
                        name: "GetTransportInfo",
                        inputs: &[INSTANCE],
                        outputs: &[
                            arg("CurrentTransportState", "TransportState"),
                            arg("CurrentTransportStatus", "TransportStatus"),
                            arg("CurrentSpeed", "TransportPlaySpeed"),
                        ],
                    },
                    Action {
                        name: "GetPositionInfo",
                        inputs: &[INSTANCE],
                        outputs: &[
                            arg("Track", "CurrentTrack"),
                            arg("TrackDuration", "CurrentTrackDuration"),
                            arg("TrackMetaData", "CurrentTrackMetaData"),
                            arg("TrackURI", "CurrentTrackURI"),
                            arg("RelTime", "RelativeTimePosition"),
                            arg("AbsTime", "AbsoluteTimePosition"),
                            arg("RelCount", "RelativeCounterPosition"),
                            arg("AbsCount", "AbsoluteCounterPosition"),
                        ],
                    },
                    Action {
                        name: "GetDeviceCapabilities",
                        inputs: &[INSTANCE],
                        outputs: &[
                            arg("PlayMedia", "PossiblePlaybackStorageMedia"),
                            arg("RecMedia", "PossibleRecordStorageMedia"),
                            arg("RecQualityModes", "PossibleRecordQualityModes"),
                        ],
                    },
                    Action {
                        name: "GetTransportSettings",
                        inputs: &[INSTANCE],
                        outputs: &[
                            arg("PlayMode", "CurrentPlayMode"),
                            arg("RecQualityMode", "CurrentRecordQualityMode"),
                        ],
                    },
                    Action {
                        name: "Stop",
                        inputs: &[INSTANCE],
                        outputs: &[],
                    },
                    Action {
                        name: "Play",
                        inputs: &[INSTANCE, arg("Speed", "TransportPlaySpeed")],
                        outputs: &[],
                    },
                    Action {
                        name: "Pause",
                        inputs: &[INSTANCE],
                        outputs: &[],
                    },
                    Action {
                        name: "Seek",
                        inputs: &[
                            INSTANCE,
                            arg("Unit", "A_ARG_TYPE_SeekMode"),
                            arg("Target", "A_ARG_TYPE_SeekTarget"),
                        ],
                        outputs: &[],
                    },
                    Action {
                        name: "Next",
                        inputs: &[INSTANCE],
                        outputs: &[],
                    },
                    Action {
                        name: "Previous",
                        inputs: &[INSTANCE],
                        outputs: &[],
                    },
                ]
            }
        }
        RENDERING_CONTROL_SERVICE => {
            const {
                &[
                    Action {
                        name: "ListPresets",
                        inputs: &[INSTANCE],
                        outputs: &[arg("CurrentPresetNameList", "PresetNameList")],
                    },
                    Action {
                        name: "SelectPreset",
                        inputs: &[INSTANCE, arg("PresetName", "A_ARG_TYPE_PresetName")],
                        outputs: &[],
                    },
                    Action {
                        name: "GetVolume",
                        inputs: &[INSTANCE, CHANNEL],
                        outputs: &[arg("CurrentVolume", "Volume")],
                    },
                    Action {
                        name: "SetVolume",
                        inputs: &[INSTANCE, CHANNEL, arg("DesiredVolume", "Volume")],
                        outputs: &[],
                    },
                ]
            }
        }
        CONNECTION_MANAGER_SERVICE => {
            const {
                &[
                    Action {
                        name: "GetProtocolInfo",
                        inputs: &[],
                        outputs: &[
                            arg("Source", "SourceProtocolInfo"),
                            arg("Sink", "SinkProtocolInfo"),
                        ],
                    },
                    Action {
                        name: "GetCurrentConnectionIDs",
                        inputs: &[],
                        outputs: &[arg("ConnectionIDs", "CurrentConnectionIDs")],
                    },
                    Action {
                        name: "GetCurrentConnectionInfo",
                        inputs: &[arg("ConnectionID", "A_ARG_TYPE_ConnectionID")],
                        outputs: &[
                            arg("RcsID", "A_ARG_TYPE_RcsID"),
                            arg("AVTransportID", "A_ARG_TYPE_AVTransportID"),
                            arg("ProtocolInfo", "A_ARG_TYPE_ProtocolInfo"),
                            arg("PeerConnectionManager", "A_ARG_TYPE_ConnectionManager"),
                            arg("PeerConnectionID", "A_ARG_TYPE_ConnectionID"),
                            arg("Direction", "A_ARG_TYPE_Direction"),
                            arg("Status", "A_ARG_TYPE_ConnectionStatus"),
                        ],
                    },
                ]
            }
        }
        _ => &[],
    }
}

pub(super) fn validate_inputs(action: &SoapAction) -> Result<(), SoapFault> {
    let definition = actions(&action.service)
        .iter()
        .find(|a| a.name == action.name)
        .ok_or_else(|| SoapFault::new(401, "Invalid Action"))?;
    if action.args.len() != definition.inputs.len()
        || definition
            .inputs
            .iter()
            .any(|arg| !action.args.contains_key(arg.name))
    {
        return Err(SoapFault::new(402, "Invalid Args"));
    }
    Ok(())
}

struct Variable {
    name: &'static str,
    kind: &'static str,
    evented: bool,
    allowed: &'static [&'static str],
    range: Option<(&'static str, &'static str)>,
}

const fn variable(name: &'static str, kind: &'static str) -> Variable {
    Variable {
        name,
        kind,
        evented: false,
        allowed: &[],
        range: None,
    }
}

impl Variable {
    const fn values(mut self, allowed: &'static [&'static str]) -> Self {
        self.allowed = allowed;
        self
    }
    const fn evented(mut self) -> Self {
        self.evented = true;
        self
    }
    const fn range(mut self, min: &'static str, max: &'static str) -> Self {
        self.range = Some((min, max));
        self
    }
}

fn variables(service: &str) -> &'static [Variable] {
    match service {
        AVTRANSPORT_SERVICE => {
            const {
                &[
                    variable("TransportState", "string").values(&[
                        "STOPPED",
                        "PLAYING",
                        "PAUSED_PLAYBACK",
                        "NO_MEDIA_PRESENT",
                    ]),
                    variable("TransportStatus", "string").values(&["OK", "ERROR_OCCURRED"]),
                    variable("PlaybackStorageMedium", "string").values(&["NONE", "NETWORK"]),
                    variable("RecordStorageMedium", "string").values(&["NOT_IMPLEMENTED"]),
                    variable("PossiblePlaybackStorageMedia", "string"),
                    variable("PossibleRecordStorageMedia", "string"),
                    variable("CurrentPlayMode", "string").values(&["NORMAL"]),
                    variable("TransportPlaySpeed", "string").values(&["1"]),
                    variable("RecordMediumWriteStatus", "string").values(&["NOT_IMPLEMENTED"]),
                    variable("CurrentRecordQualityMode", "string").values(&["NOT_IMPLEMENTED"]),
                    variable("PossibleRecordQualityModes", "string"),
                    variable("NumberOfTracks", "ui4").range("0", "1"),
                    variable("CurrentTrack", "ui4").range("0", "1"),
                    variable("CurrentTrackDuration", "string"),
                    variable("CurrentMediaDuration", "string"),
                    variable("CurrentTrackMetaData", "string"),
                    variable("CurrentTrackURI", "string"),
                    variable("AVTransportURI", "string"),
                    variable("AVTransportURIMetaData", "string"),
                    variable("NextAVTransportURI", "string"),
                    variable("NextAVTransportURIMetaData", "string"),
                    variable("RelativeTimePosition", "string"),
                    variable("AbsoluteTimePosition", "string"),
                    variable("RelativeCounterPosition", "i4"),
                    variable("AbsoluteCounterPosition", "i4"),
                    variable("LastChange", "string").evented(),
                    variable("A_ARG_TYPE_SeekMode", "string").values(&["TRACK_NR", "REL_TIME"]),
                    variable("A_ARG_TYPE_SeekTarget", "string"),
                    variable("A_ARG_TYPE_InstanceID", "ui4"),
                ]
            }
        }
        RENDERING_CONTROL_SERVICE => {
            const {
                &[
                    variable("LastChange", "string").evented(),
                    variable("PresetNameList", "string"),
                    variable("Volume", "ui2").range("0", "100"),
                    variable("A_ARG_TYPE_Channel", "string").values(&["Master"]),
                    variable("A_ARG_TYPE_PresetName", "string").values(&["FactoryDefaults"]),
                    variable("A_ARG_TYPE_InstanceID", "ui4"),
                ]
            }
        }
        CONNECTION_MANAGER_SERVICE => {
            const {
                &[
                    variable("SourceProtocolInfo", "string").evented(),
                    variable("SinkProtocolInfo", "string").evented(),
                    variable("CurrentConnectionIDs", "string").evented(),
                    variable("A_ARG_TYPE_ConnectionStatus", "string").values(&[
                        "OK",
                        "ContentFormatMismatch",
                        "InsufficientBandwidth",
                        "UnreliableChannel",
                        "Unknown",
                    ]),
                    variable("A_ARG_TYPE_ConnectionManager", "string"),
                    variable("A_ARG_TYPE_Direction", "string").values(&["Input", "Output"]),
                    variable("A_ARG_TYPE_ProtocolInfo", "string"),
                    variable("A_ARG_TYPE_ConnectionID", "i4"),
                    variable("A_ARG_TYPE_AVTransportID", "i4"),
                    variable("A_ARG_TYPE_RcsID", "i4"),
                ]
            }
        }
        _ => &[],
    }
}

pub(super) fn scpd_xml(service: &str) -> String {
    let mut xml = "<?xml version=\"1.0\" encoding=\"utf-8\"?><scpd xmlns=\"urn:schemas-upnp-org:service-1-0\"><specVersion><major>1</major><minor>0</minor></specVersion><actionList>".to_string();
    for action in actions(service) {
        xml.push_str("<action>");
        push_tag(&mut xml, "name", action.name);
        xml.push_str("<argumentList>");
        for (direction, arguments) in [("in", action.inputs), ("out", action.outputs)] {
            for argument in arguments {
                xml.push_str("<argument>");
                push_tag(&mut xml, "name", argument.name);
                push_tag(&mut xml, "direction", direction);
                push_tag(&mut xml, "relatedStateVariable", argument.variable);
                xml.push_str("</argument>");
            }
        }
        xml.push_str("</argumentList></action>");
    }
    xml.push_str("</actionList><serviceStateTable>");
    for variable in variables(service) {
        let evented = if variable.evented { "yes" } else { "no" };
        xml.push_str(&format!("<stateVariable sendEvents=\"{evented}\">"));
        push_tag(&mut xml, "name", variable.name);
        push_tag(&mut xml, "dataType", variable.kind);
        if !variable.allowed.is_empty() {
            xml.push_str("<allowedValueList>");
            for value in variable.allowed {
                push_tag(&mut xml, "allowedValue", value);
            }
            xml.push_str("</allowedValueList>");
        }
        if let Some((min, max)) = variable.range {
            xml.push_str("<allowedValueRange>");
            push_tag(&mut xml, "minimum", min);
            push_tag(&mut xml, "maximum", max);
            push_tag(&mut xml, "step", "1");
            xml.push_str("</allowedValueRange>");
        }
        xml.push_str("</stateVariable>");
    }
    xml.push_str("</serviceStateTable></scpd>");
    xml
}
