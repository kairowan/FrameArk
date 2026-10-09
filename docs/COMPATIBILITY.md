# Compatibility matrix

FrameArk is currently pre-alpha. No protocol or device combination is Stable yet. This file records verified behavior once executable receivers and senders exist; roadmap intent belongs in [PLAN.md](../PLAN.md).

## Maturity labels

| Label | Meaning |
|---|---|
| Stable | Covered by regression tests and a named device/client matrix with version compatibility commitments |
| Beta | Main path works, but device coverage, recovery, or performance remains incomplete |
| Experimental | Disabled or opt-in; behavior and interfaces may change |
| Conditional | Depends on device hardware, drivers, permissions, codecs, or display chain |
| Unsupported | Outside project promises, including DRM or HDCP bypass |

## Current status

| Protocol or capability | Current status | Evidence |
|---|---|---|
| FrameArk DNS-SD discovery (`_frameark._udp.local.`) | Experimental | `frameark-discovery` validation and event-mapping tests; multicast smoke coverage is host-dependent |
| FrameArk Native Protocol control plane | Experimental | `frameark-transport` pairing/capability tests plus `frameark-native` fake-backend Offer/Start/Status/Stop lifecycle; no platform matrix yet |
| FrameArk Native encoded media contract | Experimental | `frameark-media` bounded packet and backpressure unit tests; no platform decoder yet |
| DLNA/UPnP MediaRenderer | Planned | No executable implementation yet |
| AirPlay/RAOP audio | Planned | No executable implementation yet |
| AirPlay screen mirroring | Planned | No executable implementation yet |
| Cast V2 compatibility | Planned/Experimental | No executable implementation yet |
| Miracast/Wi-Fi Display | Planned/Conditional | No executable implementation yet |
| FairPlay, Widevine, HDCP bypass | Unsupported | Intentionally outside project scope |

## Evidence required for an entry

Each tested entry must identify receiver build/commit, sender device and OS, network type, codec and media mode, expected and observed result, session duration, diagnostics reference, and limitations. Do not submit private media, credentials, persistent device identifiers, or unsanitized packet captures.
