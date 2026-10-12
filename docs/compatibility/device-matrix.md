# Device compatibility evidence template

This template is the source format for M0–M5 device and controller evidence.
It is a reporting aid, not a compatibility claim. A row may be marked `Pass`
only when the exact receiver build, sender build, OS version, network, codec,
and duration are recorded and the result is reproducible. Use `Unavailable`
when hardware, permissions, or a proprietary pairing flow cannot be exercised;
never replace an unavailable run with a fixture result.

## Required record

Copy this block for each tested combination:

```yaml
id: <stable-local-record-id>
milestone: M0 | M1 | M2 | M3 | M4 | M5
protocol: FANP | DLNA | RAOP | AirPlay-HLS | AirPlay-Mirror
receiver:
  product: FrameArk Receiver | FrameArk Desktop | framearkd
  build: <git-sha-or-release>
  platform: Android | Windows | macOS | Linux
  version: <os-version-and-device-model>
sender:
  product: <named-controller-or-device>
  version: <app-or-os-version>
network:
  topology: loopback | wired-lan | wifi-lan | other
  security: isolated-lab | trusted-lan | other
media:
  mode: video | audio | hls | mirror | control-only
  codec: <codec-or-none>
  resolution: <value-or-none>
  rate: <fps-or-sample-rate-or-none>
expectation: <single-sentence core use case>
observed: Pass | Fail | Partial | Unavailable
duration_seconds: <integer>
recovery: <disconnect/reconnect or N/A>
diagnostics: <redacted support bundle or test-log reference>
limitations: <known limitations and unsupported paths>
privacy_review: passed | needs-review
reviewed_by: <maintainer-or-issue-reference>
```

## Acceptance rules

- `Pass` requires the named core use case to complete without bypassing the
  receiver's trust, pairing, certificate, codec, or network policy.
- `Partial` records a useful subset and must name the exact missing behavior.
- `Fail` records a reproducible defect and links the issue or regression test.
- `Unavailable` is valid evidence of a missing test environment, not evidence
  of protocol support.
- `duration_seconds` must be the actual observed run; do not infer a soak
  result from a short unit or loopback test.
- `diagnostics` must not contain packet payloads, media, credentials, pairing
  codes, private IP addresses, persistent device identifiers, or raw keys.

## Redacted diagnostic checklist

Support bundles should contain stable event codes, elapsed time, protocol
state, bounded byte/packet counters, and compatibility decision codes only.
Use the existing diagnostics exporter where possible. Replace device names,
addresses, URLs, tokens, PINs, certificates, keys, and media titles with
`<redacted>` before committing or attaching a record.

## Review checklist

- [ ] Receiver and sender versions are exact and reproducible.
- [ ] The network and codec path are named.
- [ ] The expected and observed behavior are separate.
- [ ] Duration and recovery behavior are measured, not inferred.
- [ ] Unsupported DRM/FairPlay/AWDL/HDCP paths are reported as boundaries.
- [ ] The record contains no private media, credentials, or raw packet capture.
- [ ] A second maintainer or issue review covers AirPlay/security evidence.
